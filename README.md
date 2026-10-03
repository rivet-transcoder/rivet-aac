# rivet-aac

[![CI](https://github.com/rivet-transcoder/rivet-aac/actions/workflows/ci.yml/badge.svg)](https://github.com/rivet-transcoder/rivet-aac/actions/workflows/ci.yml)

An **AAC-LC, HE-AAC and HE-AAC v2** encoder and decoder in Rust: no C, no
system libraries, no build script, nothing to install on a build host.
Written from ISO/IEC 13818-7 and ISO/IEC 14496-3 and published literature,
not translated from any other implementation. The decoder meets ISO/IEC
14496-26's 16-bit conformance criterion on all 134 AAC-LC, HE-AAC and HE-AAC
v2 conformance streams it is run on, the AAC-LC ones to a thousandth of an
LSB, and agrees with faad2's decoder to float rounding (130 dB or better) on
every AAC-LC stream it was compared on (the figures are
[below](#how-it-is-checked)).

Written for the **[rivet](https://github.com/rivet-transcoder/rivet)**
transcoder, where it is the AAC codec on both sides: the encoder behind
`audio=aac`, and the decoder that lets an AAC source be downmixed, filtered
or transcoded to Opus, MP3, FLAC or ALAC. Usable on its own by anything that
has ADTS or MP4 / Matroska access units and wants PCM back, or PCM and
wants AAC.

Published as `rivet-aac`; **imported as `aac`** (`use aac::…`). One
dependency (`thiserror`), no features, no build script.

```toml
[dependencies]
aac = { package = "rivet-aac", git = "https://github.com/rivet-transcoder/rivet-aac", branch = "develop" }
```

## What it decodes

| | supported | refused with `Error::Unsupported` |
|---|---|---|
| **Transport** | ADTS (any chunking; resynchronises on the syncword; multi-block and CRC-protected frames), raw access units with an AudioSpecificConfig (MP4 `esds`, Matroska CodecPrivate), explicit sampling frequencies | 960-sample frames |
| **Object types** | AAC-LC (2), HE-AAC (5: SBR), HE-AAC v2 (29: SBR + parametric stereo), with explicit (hierarchical or backward compatible) or implicit signalling | AAC Main (1), SSR (3), LTP (4), the error-resilient types (ER AAC-LD / ELD SBR among them), USAC (42) |
| **Rates** | 8, 11.025, 12, 16, 22.05, 24, 32, 44.1, 48, 64, 88.2 and 96 kHz (and 7.35); any explicit rate, by the standard's table mapping | — |
| **Channels** | channel configurations 1–7 (mono to 7.1) and program_config_element layouts | coupling channel elements (CCE) |
| **Tools** | long, start, short and stop windows with sine and KBD shapes and window grouping; M/S; intensity stereo (both phases, `ms_used` inversion); PNS (with the correlated noise of a pair); TNS; pulse data | prediction (AAC Main), gain control (SSR) |
| **SBR** | the high quality SBR tool: every frame class, coupled and independent channel pairs, inverse filtering, noise, added sinusoids, limiter, smoothing; the downsampled SBR tool when the configuration's extension rate equals the core's; the LFE (and a stream before its first SBR header) upsampled | the low power SBR tool (its output differs; not needed), SBR in scalable / BSAC streams |
| **Parametric stereo** | the unrestricted PS tool: 10, 20 and 34 stereo bands, coarse and fine IID, ICC with mixing procedures Ra and Rb, IPD / OPD, variable borders | — |
| **Skipped** | data stream elements; fill data; dynamic range control data | — |

Output is interleaved `f32` at ±1.0 full scale, 1024 samples per access
unit, each frame naming its speakers in [`Speaker`](src/decode/layout.rs)
order — FL FR FC LFE BL BR BC SL SR, those present, the order most
multichannel PCM pipelines use:

| configuration | output |
|---|---|
| 1 | FC |
| 2 | FL FR |
| 3 | FL FR FC |
| 4 | FL FR FC BC |
| 5 | FL FR FC BL BR |
| 6 (5.1) | FL FR FC LFE BL BR |
| 7 (7.1) | FL FR FC LFE BL BR SL SR (the outside-front pair is the side pair) |

A program_config_element is placed by its element lists (front centre and
pairs, side pairs, back pairs and centre, LFE). One whose elements cannot be
placed on distinct speakers by the standard's rules — encoders do write
them — is decoded anyway, its channels in the order it lists them and its
layout reported as unknown.

### HE-AAC and HE-AAC v2

An HE-AAC stream carries an AAC-LC core at half the rate plus spectral band
replication data in its fill elements; HE-AAC v2 adds parametric stereo to a
mono core. Both are decoded in full: the output is at the SBR rate (44.1 kHz
for a 22.05 kHz core), 2048 samples per access unit, and an HE-AAC v2
stream's mono core comes out as two channels (FL FR).

- **Explicit signalling** (audio object type 5 or 29 in the
  AudioSpecificConfig, or the backward-compatible sync extensions) sets the
  output rate and channels from the start. An extension rate equal to the
  core's selects the downsampled SBR tool (output at the core's rate).
- **Implicit signalling** (ADTS, or a raw stream whose configuration is
  plain AAC-LC) is settled by the first access unit: SBR data in it makes the
  output twice the core's rate from the start. `Decoder::sample_rate()` is
  the core's until that unit is decoded.
- **Parametric stereo** without explicit signalling is recognised by its
  first PS data: a mono SBR stream turns stereo from that access unit on
  (with the mono signal on both channels until the PS tool has its first
  parameters, as 14496-3 8.6.5.1 asks). A stream whose SBR data cannot be
  read before its first SBR header may therefore change from one to two
  channels a few frames in; each `DecodedFrame` says how many it has.

`Decoder::he_aac()` reports what the stream is. A caller that wants the old
behaviour — the AAC-LC core alone, at half the rate — calls
`Decoder::set_core_only(true)`; `decode::HE_AAC_CORE_NOTE` remains the
sentence to show a user then.

## What it encodes

**AAC-LC** at 22.05, 24, 32, 44.1 and 48 kHz (`encode::coding_rate` names
the rate to resample other input to), mono to 7.1 (channel configurations
1–7), raw access units plus the AudioSpecificConfig, and `adts_frame` for
ADTS. A sine-window MDCT with long / short block switching from an
energy-ratio transient detector; a psychoacoustic model on the MDCT spectrum
(band energy, spectral-flatness tonality, the Annex C spreading function,
pre-echo control, threshold in quiet); per-band M/S; one noise-to-mask
offset per frame found by bisection against a bit-reservoir budget (constant
rate at the decoder-buffer level); optimal sectioning by dynamic
programming. Left out, all optional for an encoder: TNS (measured, and
worse on these metrics), intensity stereo, PNS, the pulse tool, KBD windows.
The encoder was written in the rivet repository first and moved here with
its history.

**HE-AAC** (`Profile::HeAac`, mono to 7.1) and **HE-AAC v2**
(`Profile::HeAacV2`, stereo input) at 32, 44.1 and 48 kHz
(`encode::HE_AAC_RATES`), through `Encoder::with_profile`. The input goes
through the 64-band QMF analysis bank of 14496-3 4.B.18; its lower 32 bands,
synthesised at half the rate, are the AAC-LC core's input (the core's
bandwidth stops at the SBR crossover, which the bit rate places between
about 4.5 and 11 kHz), and all 64 bands give the SBR data: FIXFIX frames of
one, two or four envelopes from a transient detector, envelopes in the
decoder's energy scale, noise floors and inverse filtering levels from the
tonality of the original high band and of the low band SBR will copy into it,
time or frequency delta coding (whichever is shorter), a header every eight
frames. HE-AAC v2 mixes the stereo input down in the QMF domain (each band
keeping the mean of the two channels' energy) and sends 10-band level
differences and coherences once a frame. The SBR and PS bits come out of the
core's budget. Not used, all optional: SBR coupling of channel pairs, added
sinusoids, VAR frame classes, IPD / OPD, 20 or 34-band PS.

`Encoder::audio_specific_config()` is the core's configuration (implicit
signalling, as ADTS has it); `audio_specific_config_with(Signalling::…)`
writes the backward compatible or hierarchical explicit forms.
`sample_rate()`, `frame_samples()` (2048) and `delay()` (`HE_AAC_DELAY`,
3586 samples of priming at the output rate, for the MP4 edit list) describe
the output.

## How it is checked

- **AAC-LC against the conformance streams of ISO/IEC 14496-26** (second
  edition), used as data, the same way (`tests/conformance.rs`,
  `tools/fetch_conformance.py`): 61 streams, ten test families at six rates
  each and between them all twelve rates from 8 to 96 kHz, mono to 5.1,
  program_config_element layouts up to the 48 channels of `al08`, every
  window sequence and shape, M/S, intensity stereo, TNS, pulses. The 49
  without PNS decode within the 16-bit criterion with room to spare: the
  worst RMS difference 3.7e-8 (0.001 LSB), the largest single difference
  1.8e-7 (0.006 LSB). The 12 with perceptual noise substitution, whose noise
  no two decoders generate alike, match the reference's energy (each
  channel's within 0.00 dB, every 2048-sample block within 1.9 dB). Not in
  the set: the 960-sample-frame (`sf`) streams, coupling channel elements
  (`al07`), channel configurations 11 and up (`al20`, `al21`), which the
  decoder refuses by name, and the dynamic range control families (`al14`,
  `al16`), whose references apply DRC, which this decoder does not.
- **HE-AAC and HE-AAC v2 against the conformance streams of ISO/IEC
  14496-26** (second edition), used as data (`tests/conformance.rs`;
  `tools/fetch_conformance.py` extracts the streams and reference waveforms
  from ISO's published electronic inserts, and a CI job fetches and runs
  them). All 73 of the package's AAC + SBR and AAC + SBR + PS streams with
  1024-sample frames and channel configurations 1–6 — every frame class,
  signalling form, sampling rate pair from 8/16 to 48/96 kHz, the
  downsampled SBR tool, coupled pairs, sinusoids, 4.0, 5.0 and 5.1, and all
  seven PS streams (IID only, ICC only, fine IID with mixing procedure Rb,
  IPD / OPD, 34 bands) — decode within the 16-bit criterion against the high
  quality SBR / unrestricted PS references: RMS of the difference below
  2^-15/sqrt(12) of full scale (8.8e-6) and its largest magnitude at most
  2^-14 (6.1e-5). Measured: the median stream's RMS difference is 4.8e-8
  (0.002 LSB), the worst 5.7e-7 (0.02 LSB, the 96 kHz 5.0 stream, whose
  output clips like the reference); the largest single difference 4.3e-5
  (1.4 LSB, the same stream), every other stream's at most 1.0e-5. Not in
  the set: the 960-sample (`al960_*`) streams, the 6.1 / 7.1 `gen` streams
  (channel configurations 11 and 12) and the error-resilient / low-delay
  ones, which this crate refuses by name.
- **HE-AAC round trips** through this crate's encoder and decoder
  (`src/decode/tests.rs`): at every HE-AAC rate, mono, stereo and 5.1, the
  core band comes back as a waveform 41–43 dB above its coding noise (HE-AAC
  v2: 28–36 dB) exactly `HE_AAC_DELAY` samples late; white noise comes back
  within 1.5 dB of its level in every 1 kHz band from 1 to 15 kHz, where the
  core alone stops at the crossover (85 dB down); HE-AAC v2 keeps a panned
  signal's level difference to the quantiser's step (+6 dB → +7.0 dB,
  −10 dB → −10.0 dB) and uncorrelated channels uncorrelated (0.05), correlated
  ones correlated (1.00); every signalling form decodes alike.
- **SBR and PS unit tests**: the QMF banks against the flowcharts evaluated
  in double precision (110 dB) and every analysis / synthesis pair for
  unity-gain reconstruction; the SBR and PS Huffman tables complete prefix
  codes of their sizes, round-tripping every value; the frequency band
  tables' nesting over every header at 32, 44.1 and 48 kHz; the QMF window's
  symmetry; the hybrid filters summing back to a pure delay; mixing
  procedure Ra's power and level difference.

- **Against faad2's decoder, as a black box** (`tests/faad_oracle.rs`; CI
  installs faad2's `faad` for it and sets `AAC_REQUIRE_FAAD`, so a missing
  binary fails rather than skips). The committed streams in
  [`tests/data`](tests/data/README.md) come from fdk-aac, and this crate's
  encoder makes a matrix at test time; both decoders decode each, and the
  PCM must agree to at least 90 dB SNR channel by channel for AAC-LC, 45 dB
  for HE-AAC. Figures: see the table below.
- **Round trips** through this crate's encoder, in every layout, and the
  encoder's own suite (a small reference decoder written from the standard,
  and faad decoding every rate × bit rate × layout without error).
- **Syntax no encoder above writes** — KBD windows in every frame, pulse
  data in every long window — from this encoder asked to emit it
  (`Encoder::exercise`), decoded by both decoders.
- **Malformed input**: property tests (`tests/fuzz.rs`, proptest) feed
  arbitrary bytes, and valid streams with bits flipped, bytes cut and
  garbage spliced, to every entry point — errors, never a panic — and
  `fuzz/` has cargo-fuzz targets for longer runs.
- **Tables**: every codebook is a complete prefix code and every codeword
  decodes to its own index; every band table rises in multiples of four to
  1024 or 128; the windows meet the Princen-Bradley condition.

Measured 2026-10-02 against faad2 2.11.3. SNR is this decoder's output
against faad's, the worst channel of each stream:

| streams | made by | rates | layouts and tools | worst SNR | largest \|difference\| |
|---|---|---|---|---|---|
| 12, committed | fdk-aac | 8, 11.025, 12, 16, 22.05, 32, 44.1, 48, 64, 88.2, 96 kHz | mono to 7.1 (a PCE 7.1, in band), CBR and VBR, ADTS and MP4; TNS, M/S, intensity, KBD | 131.2 dB (7.1; the others 132.3 dB or better) | 1.2e-6 |
| 41, at test time | this crate's encoder | 22.05–48 kHz | mono, stereo, 3.0, 4.0, 5.0, 5.1, 7.1; 32–320 kb/s | 132.0 dB | 2.4e-7 |
| 9, at test time | this crate's encoder, exercising KBD and pulses | 32–48 kHz | mono, stereo, 5.1 | 133.7 dB | 2.2e-7 |
| 4, committed | fdk-aac, HE-AAC | 44.1, 48 kHz | stereo, 5.1; implicit, backward-compatible and hierarchical signalling | 53.0 dB | 2.1e-2 |
| 9, at test time | this crate's encoder, HE-AAC | 32, 44.1, 48 kHz | mono, stereo, 5.1 | 96.5 dB | 5.5e-5 |
| 2 committed, 3 at test time | fdk-aac and this crate's encoder, HE-AAC v2 | 32–48 kHz | stereo | channel levels within 1.5 dB (a sanity check: faad2's parametric stereo is not ISO's reference's; the conformance streams hold this one to that) | — |

For AAC-LC float rounding is the whole difference: about 2^-23 of full
scale, 130–138 dB below a full-scale signal. PNS is random by definition, so
its bands are compared by energy. The HE-AAC figures are the two decoders'
distance where the SBR tools fdk-aac's encoder uses (and this crate's does
not) come in; ISO's references, not faad2, are what the SBR decoder is held
to.

The tests and CI use no ffmpeg: until 2026-10-02 ffmpeg's decoder was the
black-box oracle and its encoder made the AAC-LC matrix; ISO's AAC-LC
conformance streams and faad2 replaced them (which also brought HE-AAC into
the black-box comparison), and the committed streams were remade without it.

## Provenance and licensing

Written from the standards' text and published literature; **no AAC
implementation's source was read** — not FFmpeg's (aacsbr, aacps among it),
faad2, fdk-aac, FAAC, Helix, symphonia, the MPEG reference software or any
other. faad2's decoder and fdk-aac's encoder were used only as black boxes,
to decode and to make test streams. The normative tables were transcribed
from a copy of ISO/IEC 13818-7:2004 whose use the owner approved.
[docs/PROVENANCE.md](docs/PROVENANCE.md) records every source, clause by
clause and table by table.

**Patents.** AAC may be subject to patent licensing in some jurisdictions;
Via LA administers a licensing programme for AAC. Nothing here is a licence
to any patent, and the authors make no claim about whether anyone needs one.
Spectral band replication and parametric stereo (HE-AAC, HE-AAC v2) are
implemented at the owner's request of 2026-10-02, reversing the 2026-09-28
decision to leave them out; whether their use needs a patent licence is the
user's to determine. USAC (xHE-AAC) remains absent.

## Using it

```rust
// Raw access units (MP4 / Matroska): the AudioSpecificConfig first.
let mut dec = aac::decode::Decoder::new_raw(&asc)?;
for au in access_units {
    for frame in dec.decode(au)? {
        // frame.samples: interleaved f32; frame.sample_rate; frame.speakers
    }
}

// ADTS (MPEG-TS, .aac files): bytes in any chunking.
let mut dec = aac::decode::Decoder::new_adts();
let frames = dec.decode(&bytes)?;

// Encoding: interleaved f32 at a coded rate, access units back.
let mut enc = aac::encode::Encoder::new(aac::encode::EncoderConfig {
    sample_rate: 48_000, channels: 2, bitrate: 128_000,
})?;
let mut aus = enc.encode(&pcm);
aus.extend(enc.flush());
let asc = enc.audio_specific_config();

// HE-AAC v2 at 32 kb/s: stereo input at 44.1 kHz, 2048 samples per access
// unit, HE_AAC_DELAY samples of priming.
use aac::encode::{Encoder, EncoderConfig, Profile, Signalling};
let mut enc = Encoder::with_profile(
    EncoderConfig { sample_rate: 44_100, channels: 2, bitrate: 32_000 },
    Profile::HeAacV2,
)?;
let mut aus = enc.encode(&pcm);
aus.extend(enc.flush());
let asc = enc.audio_specific_config_with(Signalling::BackwardCompatible);
```

## License

Open Encoding Attribution License v1.0 — a source-available (not OSI open-source)
license, royalty-free, with a commercial-attribution requirement. See
[LICENSE.md](LICENSE.md) and [NOTICE](NOTICE).
