//! The decoder against an independent one, used as a black box: ffmpeg
//! makes AAC streams (its native encoder, over a matrix of layouts, rates,
//! bit rates and tools, in ADTS and MP4), and ffmpeg's decoder and this
//! crate's decode each; the PCM must agree to within float rounding. The
//! committed streams in `tests/data` (made by other encoders; see the README
//! there) are compared the same way. No ffmpeg code is used or consulted:
//! only its command-line tools' output.
//!
//! Without `ffmpeg` and `ffprobe` on PATH every test here says so and
//! passes, unless `AAC_REQUIRE_FFMPEG` is set (as in CI's oracle job).
//! `cargo test --release --test ffmpeg_oracle -- --nocapture` prints the
//! per-stream figures.

use std::path::{Path, PathBuf};
use std::process::Command;

use aac::decode::{Decoder, HE_AAC_CORE_NOTE, Speaker, ToolUse};

fn have_ffmpeg() -> bool {
    let ok = |bin: &str| {
        Command::new(bin)
            .arg("-version")
            .output()
            .is_ok_and(|o| o.status.success())
    };
    if ok("ffmpeg") && ok("ffprobe") {
        return true;
    }
    assert!(
        std::env::var_os("AAC_REQUIRE_FFMPEG").is_none(),
        "AAC_REQUIRE_FFMPEG is set but ffmpeg / ffprobe are not on PATH"
    );
    eprintln!("ffmpeg / ffprobe not on PATH: skipping the black-box comparison");
    false
}

fn scratch() -> PathBuf {
    let d = std::env::temp_dir().join(format!("rivet-aac-oracle-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn run(cmd: &mut Command) -> Vec<u8> {
    let out = cmd.output().expect("spawn");
    assert!(
        out.status.success(),
        "{cmd:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}

/// A test signal per channel: two tones, noise bursts (for PNS), decaying
/// clicks (for short windows and TNS), different in every channel.
fn source_expr(channels: usize) -> String {
    (0..channels)
        .map(|c| {
            format!(
                "0.22*sin(2*PI*{f1}*t)+0.1*sin(2*PI*{f2}*t)*sin(2*PI*0.7*t)\
                 +0.25*(random({c})-0.5)*gt(mod(t+{ph},1),0.6)\
                 +0.45*exp(-60*mod(t+{ph},0.37))*sin(2*PI*{f3}*t)",
                f1 = 180 + 97 * c,
                f2 = 1800 + 333 * c,
                f3 = 2500 + 150 * c,
                ph = 0.05 * c as f64,
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

struct Case {
    name: String,
    rate: u32,
    layout: &'static str,
    channels: usize,
    /// `-b:a` or, for VBR, `-q:a`.
    rate_arg: (&'static str, String),
    mp4: bool,
    extra: Vec<&'static str>,
}

fn make(case: &Case, dir: &Path) -> PathBuf {
    let path = dir.join(format!("{}.{}", case.name, if case.mp4 { "m4a" } else { "aac" }));
    let src = format!(
        "aevalsrc='{}':s={}:c={}:d=3",
        source_expr(case.channels),
        case.rate,
        case.layout
    );
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-v", "error", "-y", "-f", "lavfi", "-i", &src, "-c:a", "aac"])
        .args([case.rate_arg.0, &case.rate_arg.1])
        .args(&case.extra);
    if !case.mp4 {
        cmd.args(["-f", "adts"]);
    }
    cmd.arg(&path);
    run(&mut cmd);
    path
}

/// ffmpeg's decode: interleaved f32, its rate and channel count.
fn ffmpeg_decode(path: &Path) -> (Vec<f32>, u32, usize, String) {
    let probe = String::from_utf8(run(Command::new("ffprobe").args([
        "-v",
        "error",
        "-select_streams",
        "a:0",
        "-show_entries",
        "stream=sample_rate,channels,channel_layout",
        "-of",
        "default=nw=1",
    ])
    .arg(path)))
    .unwrap();
    let field = |k: &str| {
        probe
            .lines()
            .find_map(|l| l.strip_prefix(&format!("{k}=")))
            .unwrap_or("")
            .to_string()
    };
    let pcm = run(Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-f", "f32le", "-"]));
    let samples = pcm
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();
    (
        samples,
        field("sample_rate").parse().unwrap(),
        field("channels").parse().unwrap(),
        field("channel_layout"),
    )
}

/// The AudioSpecificConfig from an MP4's `esds` (14496-1 descriptors).
fn esds_asc(file: &[u8]) -> Vec<u8> {
    let at = file.windows(4).position(|w| w == b"esds").expect("an esds box") + 8;
    let mut i = at;
    let descriptor = |i: &mut usize| -> (u8, usize) {
        let tag = file[*i];
        *i += 1;
        let mut len = 0usize;
        loop {
            let b = file[*i];
            *i += 1;
            len = (len << 7) | usize::from(b & 0x7f);
            if b & 0x80 == 0 {
                break;
            }
        }
        (tag, len)
    };
    let (tag, _) = descriptor(&mut i);
    assert_eq!(tag, 3);
    let flags = file[i + 2];
    i += 3;
    if flags & 0x80 != 0 {
        i += 2;
    }
    if flags & 0x40 != 0 {
        i += 1 + usize::from(file[i]);
    }
    if flags & 0x20 != 0 {
        i += 2;
    }
    let (tag, _) = descriptor(&mut i);
    assert_eq!(tag, 4);
    i += 13;
    let (tag, len) = descriptor(&mut i);
    assert_eq!(tag, 5);
    file[i..i + len].to_vec()
}

/// The MP4's audio access units, located by ffprobe (a black box here too).
fn mp4_packets(path: &Path, file: &[u8]) -> Vec<Vec<u8>> {
    let out = String::from_utf8(run(Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "a:0", "-show_entries", "packet=pos,size", "-of", "default=nw=1"])
        .arg(path)))
    .unwrap();
    let (mut pos, mut size) = (None, None);
    let mut packets = Vec::new();
    for line in out.lines() {
        if let Some(v) = line.strip_prefix("pos=") {
            pos = v.parse::<usize>().ok();
        } else if let Some(v) = line.strip_prefix("size=") {
            size = v.parse::<usize>().ok();
        }
        if let (Some(p), Some(s)) = (pos, size) {
            packets.push(file[p..p + s].to_vec());
            pos = None;
            size = None;
        }
    }
    packets
}

struct Ours {
    samples: Vec<f32>,
    rate: u32,
    channels: usize,
    speakers: Option<Vec<Speaker>>,
    tools: ToolUse,
    he_aac: bool,
}

fn our_decode(path: &Path) -> Ours {
    let file = std::fs::read(path).unwrap();
    let is_mp4 = matches!(path.extension().and_then(|e| e.to_str()), Some("m4a" | "mp4"));
    let (mut dec, units) = if is_mp4 {
        (Decoder::new_raw(&esds_asc(&file)).unwrap(), mp4_packets(path, &file))
    } else {
        (Decoder::new_adts(), vec![file])
    };
    let mut ours = Ours {
        samples: Vec::new(),
        rate: 0,
        channels: 0,
        speakers: None,
        tools: ToolUse::default(),
        he_aac: false,
    };
    for u in &units {
        for f in dec
            .decode(u)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        {
            ours.rate = f.sample_rate;
            ours.channels = f.channels;
            ours.speakers = f.speakers;
            ours.samples.extend(f.samples);
        }
    }
    ours.tools = dec.tool_use();
    ours.he_aac = dec.he_aac().is_some();
    ours
}

/// Figures for one stream.
struct Agreement {
    /// Largest |ours - ffmpeg| over every channel.
    max_abs: f32,
    /// The worst channel's SNR of ours against ffmpeg's, dB.
    snr: f64,
    /// ffmpeg's channel for each of ours.
    mapping: Vec<usize>,
    lag: isize,
    /// Each of our channels' SNR against its ffmpeg channel.
    per_channel: Vec<f64>,
}

fn channel(x: &[f32], n: usize, c: usize) -> Vec<f32> {
    x.iter().skip(c).step_by(n).copied().collect()
}

fn compare_channels(a: &[f32], b: &[f32], lag: isize) -> (f64, f32) {
    let (mut s, mut e, mut m) = (0.0f64, 0.0f64, 0.0f32);
    for (i, &r) in b.iter().enumerate() {
        let j = i as isize + lag;
        if j < 0 || j as usize >= a.len() {
            continue;
        }
        let d = a[j as usize] - r;
        s += f64::from(r) * f64::from(r);
        e += f64::from(d) * f64::from(d);
        m = m.max(d.abs());
    }
    (10.0 * (s / e.max(1e-300)).log10(), m)
}

fn agreement(ours: &[f32], theirs: &[f32], channels: usize) -> Agreement {
    let a: Vec<Vec<f32>> = (0..channels).map(|c| channel(ours, channels, c)).collect();
    let b: Vec<Vec<f32>> = (0..channels).map(|c| channel(theirs, channels, c)).collect();
    // Alignment: whole frames of priming either side (an MP4's edit list
    // makes ffmpeg drop the first 1024 samples; ADTS has no such signal).
    let lag = [-2048isize, -1024, 0, 1024, 2048]
        .into_iter()
        .max_by(|&x, &y| {
            compare_channels(&a[0], &b[0], x)
                .0
                .total_cmp(&compare_channels(&a[0], &b[0], y).0)
        })
        .unwrap();
    let mut mapping = Vec::new();
    let mut per_channel = Vec::new();
    let (mut snr, mut max_abs) = (f64::INFINITY, 0.0f32);
    for ac in &a {
        let (j, (s, m)) = b
            .iter()
            .enumerate()
            .map(|(j, bc)| (j, compare_channels(ac, bc, lag)))
            .max_by(|x, y| x.1.0.total_cmp(&y.1.0))
            .unwrap();
        mapping.push(j);
        per_channel.push(s);
        snr = snr.min(s);
        max_abs = max_abs.max(m);
    }
    Agreement {
        max_abs,
        snr,
        mapping,
        lag,
        per_channel,
    }
}

/// Block energies (dB) of ours and theirs agree: for streams with PNS,
/// whose noise is random by definition.
fn envelope_gap_db(ours: &[f32], theirs: &[f32], channels: usize, lag: isize) -> f64 {
    let block = 2048 * channels;
    let mut worst = 0.0f64;
    for (i, t) in theirs.chunks(block).enumerate() {
        let start = i as isize * block as isize + lag * channels as isize;
        if start < 0 || start as usize + t.len() > ours.len() || t.len() < block {
            continue;
        }
        let o = &ours[start as usize..start as usize + t.len()];
        let e = |x: &[f32]| x.iter().map(|&v| f64::from(v) * f64::from(v)).sum::<f64>();
        let (eo, et) = (e(o), e(t));
        if et > 1e-3 {
            worst = worst.max((10.0 * (eo / et).log10()).abs());
        }
    }
    worst
}

fn tools_line(t: &ToolUse) -> String {
    format!(
        "short {} start/stop {} kbd {} ms {} is {} pns {} tns {} pulse {} pce {}",
        t.short,
        t.start_stop,
        t.kbd,
        t.ms_bands,
        t.intensity_bands,
        t.noise_bands,
        t.tns_filters,
        t.pulses,
        t.program_config
    )
}

fn check(path: &Path, report: &mut Vec<String>) -> Ours {
    let ours = our_decode(path);
    let (theirs, rate, channels, layout) = ffmpeg_decode(path);
    assert_eq!(ours.rate, rate, "{}: sample rate", path.display());
    assert_eq!(ours.channels, channels, "{}: channels", path.display());
    let a = agreement(&ours.samples, &theirs, channels);
    let name = path.file_name().unwrap().to_string_lossy();
    let mut sorted = a.mapping.clone();
    sorted.sort();
    assert_eq!(sorted, (0..channels).collect::<Vec<_>>(), "{name}: channel mapping {:?}", a.mapping);
    if ours.tools.noise_bands > 0 {
        let gap = envelope_gap_db(&ours.samples, &theirs, channels, a.lag);
        report.push(format!(
            "{name:<34} {rate:>6} Hz {layout:<10} PNS: block energy within {gap:.2} dB [{}]",
            tools_line(&ours.tools)
        ));
        assert!(gap < 1.0, "{name}: PNS energy differs by {gap:.2} dB");
    } else {
        report.push(format!(
            "{name:<34} {rate:>6} Hz {layout:<10} max|diff| {:.2e}  SNR {:>6.1} dB  map {:?} [{}]",
            a.max_abs,
            a.snr,
            a.mapping,
            tools_line(&ours.tools)
        ));
        assert!(
            a.snr >= 90.0,
            "{name}: {:.1} dB against ffmpeg (per channel {:.1?})",
            a.snr,
            a.per_channel
        );
    }
    // The speakers each layout reports, in ffmpeg's native order.
    let expected: Option<&[Speaker]> = match layout.as_str() {
        "mono" => Some(&[Speaker::FC]),
        "stereo" => Some(&[Speaker::FL, Speaker::FR]),
        "5.1" => Some(&[Speaker::FL, Speaker::FR, Speaker::FC, Speaker::LFE, Speaker::BL, Speaker::BR]),
        "7.1" => Some(&[
            Speaker::FL,
            Speaker::FR,
            Speaker::FC,
            Speaker::LFE,
            Speaker::BL,
            Speaker::BR,
            Speaker::SL,
            Speaker::SR,
        ]),
        _ => None,
    };
    if let Some(e) = expected {
        assert_eq!(ours.speakers.as_deref(), Some(e), "{name}");
        assert_eq!(a.mapping, (0..channels).collect::<Vec<_>>(), "{name}");
    }
    ours
}

fn cases() -> Vec<Case> {
    let mut v = Vec::new();
    let layouts: [(&str, usize, &str); 4] = [("mono", 1, "64k"), ("stereo", 2, "128k"), ("5.1", 6, "384k"), ("7.1", 8, "512k")];
    // Every layout at every rate, CBR, ADTS, PNS off.
    for rate in [22_050u32, 24_000, 32_000, 44_100, 48_000] {
        for &(layout, channels, br) in &layouts {
            v.push(Case {
                name: format!("lc-{rate}-{}-{br}", layout.replace('.', "_")),
                rate,
                layout,
                channels,
                rate_arg: ("-b:a", br.to_string()),
                mp4: false,
                extra: vec!["-aac_pns", "0"],
            });
        }
    }
    // Bit rates from 32k to 320k, stereo 44.1 kHz, ADTS and MP4.
    for br in ["32k", "64k", "96k", "160k", "256k", "320k"] {
        for mp4 in [false, true] {
            v.push(Case {
                name: format!("lc-44100-stereo-{br}{}", if mp4 { "-mp4" } else { "" }),
                rate: 44_100,
                layout: "stereo",
                channels: 2,
                rate_arg: ("-b:a", br.to_string()),
                mp4,
                extra: vec!["-aac_pns", "0"],
            });
        }
    }
    // VBR, and the tools the encoder can be asked for.
    for (name, extra) in [
        ("vbr-q1", vec!["-aac_pns", "0"]),
        ("tns-is-ms", vec!["-aac_pns", "0", "-aac_tns", "1", "-aac_is", "1", "-aac_ms", "1"]),
        ("pns", vec!["-aac_pns", "1"]),
        ("fast-coder", vec!["-aac_pns", "0", "-aac_coder", "fast"]),
    ] {
        for (layout, channels) in [("stereo", 2), ("5.1", 6)] {
            let vbr = name.starts_with("vbr");
            v.push(Case {
                name: format!("{name}-48000-{}", layout.replace('.', "_")),
                rate: 48_000,
                layout,
                channels,
                rate_arg: if vbr { ("-q:a", "1".into()) } else { ("-b:a", if channels == 2 { "96k".into() } else { "256k".into() }) },
                mp4: channels == 6,
                extra: extra.clone(),
            });
        }
    }
    // Layouts without a channel configuration: a program_config_element.
    for (layout, channels) in [("6.1", 7), ("quad", 4), ("hexagonal", 6)] {
        v.push(Case {
            name: format!("pce-{}", layout),
            rate: 48_000,
            layout,
            channels,
            rate_arg: ("-b:a", "320k".into()),
            mp4: false,
            extra: vec!["-aac_pns", "0"],
        });
    }
    // Layouts with a channel configuration other than the four above.
    for (layout, channels) in [("3.0", 3), ("4.0", 4), ("5.0", 5)] {
        v.push(Case {
            name: format!("cfg-{}", layout.replace('.', "_")),
            rate: 44_100,
            layout,
            channels,
            rate_arg: ("-b:a", "256k".into()),
            mp4: true,
            extra: vec!["-aac_pns", "0"],
        });
    }
    v
}

#[test]
fn agrees_with_ffmpeg_on_its_own_streams() {
    if !have_ffmpeg() {
        return;
    }
    let dir = scratch();
    let mut report = Vec::new();
    let mut failures = Vec::new();
    for case in cases() {
        let path = make(&case, &dir);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut lines = Vec::new();
            check(&path, &mut lines);
            lines
        }));
        match r {
            Ok(lines) => report.extend(lines),
            Err(e) => failures.push(format!(
                "{}: {}",
                case.name,
                e.downcast_ref::<String>().cloned().unwrap_or_default()
            )),
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    for l in &report {
        eprintln!("{l}");
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The committed streams (tests/data/README.md says how each was made).
#[test]
fn agrees_with_ffmpeg_on_committed_streams() {
    if !have_ffmpeg() {
        return;
    }
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let mut report = Vec::new();
    let mut failures = Vec::new();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&data)
        .map(|d| d.map(|e| e.unwrap().path()).collect())
        .unwrap_or_default();
    paths.sort();
    for path in paths {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !(name.ends_with(".aac") || name.ends_with(".m4a")) || name.starts_with("he") {
            continue;
        }
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut lines = Vec::new();
            check(&path, &mut lines);
            lines
        })) {
            Ok(lines) => report.extend(lines),
            Err(e) => failures.push(format!(
                "{name}: {}",
                e.downcast_ref::<String>().cloned().unwrap_or_default()
            )),
        }
    }
    for l in &report {
        eprintln!("{l}");
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// HE-AAC and HE-AAC v2 decode as their AAC-LC core: half the rate ffmpeg
/// (which does decode SBR) outputs, flagged, and the same signal below the
/// core's bandwidth.
#[test]
fn he_aac_decodes_as_its_core() {
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&data)
        .map(|d| d.map(|e| e.unwrap().path()).collect())
        .unwrap_or_default();
    paths.retain(|p| p.file_name().unwrap().to_string_lossy().starts_with("he"));
    paths.sort();
    assert!(!paths.is_empty(), "no HE-AAC streams in tests/data");
    let ffmpeg = have_ffmpeg();
    for path in paths {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let ours = our_decode(&path);
        assert!(ours.he_aac, "{name}: not reported as HE-AAC");
        let rms = |x: &[f32]| (x.iter().map(|&v| f64::from(v).powi(2)).sum::<f64>() / x.len() as f64).sqrt();
        let our_rms = rms(&ours.samples);
        if ffmpeg {
            let (theirs, rate, channels, layout) = ffmpeg_decode(&path);
            assert_eq!(ours.rate * 2, rate, "{name}: the core runs at half the output rate");
            let their_rms = rms(&theirs);
            let level = 20.0 * (our_rms / their_rms).log10();
            eprintln!(
                "{name:<34} core {} Hz x{} ({:?}), ffmpeg {rate} Hz x{channels} ({layout}); level {level:+.1} dB; {HE_AAC_CORE_NOTE}",
                ours.rate,
                ours.channels,
                ours.speakers
            );
            assert!(level.abs() < 3.0, "{name}: core level {level:.1} dB off the full decode");
        } else {
            assert!(our_rms > 1e-3, "{name}: silent");
        }
    }
}

/// Syntax no encoder above is known to write, from this crate's encoder
/// asked to exercise it: KBD windows (window_shape 1) in every frame, and
/// pulse data in every long window. Both decoders must agree on it.
#[test]
fn agrees_with_ffmpeg_on_kbd_windows_and_pulses() {
    if !have_ffmpeg() {
        return;
    }
    use aac::encode::{Encoder, EncoderConfig, Exercise, adts_frame};
    let dir = scratch();
    let mut report = Vec::new();
    for (rate, channels) in [(48_000u32, 2u8), (44_100, 1), (32_000, 6)] {
        for ex in [
            Exercise { kbd_windows: true, pulses: false },
            Exercise { kbd_windows: false, pulses: true },
            Exercise { kbd_windows: true, pulses: true },
        ] {
            let mut enc = Encoder::new(EncoderConfig { sample_rate: rate, channels, bitrate: 0 }).unwrap();
            enc.exercise(ex);
            let n = usize::from(channels);
            let len = rate as usize * 2;
            let samples: Vec<f32> = (0..len * n)
                .map(|i| {
                    let (t, c) = ((i / n) as f32 / rate as f32, (i % n) as f32);
                    let click = (-60.0 * ((t + 0.05 * c) % 0.37)).exp();
                    0.2 * (2.0 * std::f32::consts::PI * (180.0 + 97.0 * c) * t).sin()
                        + 0.4 * click * (2.0 * std::f32::consts::PI * (2500.0 + 150.0 * c) * t).sin()
                })
                .collect();
            let mut aus = enc.encode(&samples);
            aus.extend(enc.flush());
            let adts: Vec<u8> = aus
                .iter()
                .flat_map(|au| adts_frame(enc.sampling_index(), enc.channel_configuration(), au))
                .collect();
            let path = dir.join(format!(
                "exercise-{rate}-{channels}{}{}.aac",
                if ex.kbd_windows { "-kbd" } else { "" },
                if ex.pulses { "-pulses" } else { "" }
            ));
            std::fs::write(&path, adts).unwrap();
            let ours = check(&path, &mut report);
            if ex.kbd_windows {
                assert!(ours.tools.kbd > 0, "{}: no KBD frames", path.display());
            }
            if ex.pulses {
                assert!(ours.tools.pulses > 0, "{}: no pulses", path.display());
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    for l in &report {
        eprintln!("{l}");
    }
}
