# rivet-aac

[![CI](https://github.com/rivet-transcoder/rivet-aac/actions/workflows/ci.yml/badge.svg)](https://github.com/rivet-transcoder/rivet-aac/actions/workflows/ci.yml)

An **AAC-LC** encoder and decoder in Rust: no C, no system libraries, no
build script, nothing to install on a build host. Written from ISO/IEC
13818-7 and ISO/IEC 14496-3 and published literature, not translated from
any other implementation. The decoder agrees with ffmpeg's to float rounding
on every stream it was checked on (the figures are [below](#how-it-is-checked)).

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
| **Object types** | AAC-LC (2); the AAC-LC core of HE-AAC (5) and HE-AAC v2 (29) | AAC Main (1), SSR (3), LTP (4), the error-resilient types, USAC (42) |
| **Rates** | 8, 11.025, 12, 16, 22.05, 24, 32, 44.1, 48, 64, 88.2 and 96 kHz (and 7.35); any explicit rate, by the standard's table mapping | — |
| **Channels** | channel configurations 1–7 (mono to 7.1) and program_config_element layouts | coupling channel elements (CCE) |
| **Tools** | long, start, short and stop windows with sine and KBD shapes and window grouping; M/S; intensity stereo (both phases, `ms_used` inversion); PNS (with the correlated noise of a pair); TNS; pulse data | prediction (AAC Main), gain control (SSR) |
| **Skipped** | data stream elements; fill elements (SBR data among them) | — |

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

### HE-AAC: the AAC-LC core only

**Spectral band replication, parametric stereo and USAC are not implemented,
on purpose** — so there is no HE-AAC, HE-AAC v2 or xHE-AAC decoding here.
An HE-AAC stream carries an AAC-LC core plus SBR (and PS) data in its fill
elements, and this decoder decodes that core: **half the stream's sample
rate** (22.05 kHz for a 44.1 kHz HE-AAC stream), a bandwidth below a quarter
of the full rate, and, for HE-AAC v2, the core's single channel.
`Decoder::he_aac()` reports it — from the AudioSpecificConfig for explicit
signalling (object type 5 or 29, or the backward-compatible sync extension),
from the first access unit that carries SBR data for implicit signalling —
and `decode::HE_AAC_CORE_NOTE` is the sentence to show a user:
*HE-AAC decoded as its AAC-LC core (lower bandwidth)*. A caller that would
rather keep the stream whole (pass it through) can ask
`decode::probe(asc, first_access_unit)` before decoding.

## What it encodes

AAC-LC at 22.05, 24, 32, 44.1 and 48 kHz (`encode::coding_rate` names the
rate to resample other input to), mono to 7.1 (channel configurations
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

## How it is checked

- **Against ffmpeg's decoder, as a black box** (`tests/ffmpeg_oracle.rs`;
  CI installs ffmpeg for it). ffmpeg's own encoder makes a matrix of streams
  at test time, the committed streams in [`tests/data`](tests/data/README.md)
  come from fdk-aac (through ffmpeg's command line), and both decoders
  decode each; the PCM must agree to at least 90 dB SNR, channel by channel.
  Figures: see the table below.
- **Round trips** through this crate's encoder, in every layout, and the
  encoder's own suite (a small reference decoder written from the standard,
  and ffmpeg decoding every rate × bit rate × layout without error).
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

Measured 2026-09-28 against the `ffmpeg` of Debian bookworm (5.1). SNR is
this decoder's output against ffmpeg's, the worst channel of each stream:

| streams | made by | rates | layouts and tools | worst SNR | largest \|difference\| |
|---|---|---|---|---|---|
| 44, at test time | ffmpeg's encoder | 22.05–48 kHz | mono, stereo, 3.0, 4.0, 5.0, 5.1, 7.1; PCE quad, hexagonal, 6.1; 32–320 kb/s, CBR and VBR, ADTS and MP4; M/S, intensity stereo, TNS, short windows, KBD | 137.7 dB | 2.4e-7 |
| 12, committed | fdk-aac | 8, 11.025, 12, 16, 22.05, 32, 44.1, 48, 64, 88.2, 96 kHz | mono to 7.1 (a PCE 7.1), CBR and VBR; TNS, M/S, intensity, KBD | 128.1 dB (7.1; the others 136.6 dB or better) | 2.0e-6 |
| 9, at test time | this crate's encoder, exercising KBD and pulses | 32–48 kHz | mono, stereo, 5.1 | 138.1 dB | 1.8e-7 |
| 2, at test time | ffmpeg's encoder with PNS | 48 kHz | stereo, 5.1 | noise bands: block energy within 0.00 dB | — |
| 6, committed | fdk-aac, HE-AAC and HE-AAC v2 | 32–48 kHz (cores 16–24 kHz) | stereo, 5.1; implicit, explicit and backward-compatible signalling | core at half the rate, level within 0.6 dB of ffmpeg's full SBR decode | — |

Float rounding is the whole difference: about 2^-23 of full scale, 138 dB
below a full-scale signal. PNS is random by definition, so its bands are
compared by energy.

## Provenance and licensing

Written from the standards' text and published literature; **no AAC
implementation's source was read** — not FFmpeg's, faad2, fdk-aac, FAAC,
symphonia or any other — and ffmpeg was used only as a command-line tool, to
make and decode test streams. The normative tables were transcribed from a
copy of ISO/IEC 13818-7:2004 whose use the owner approved.
[docs/PROVENANCE.md](docs/PROVENANCE.md) records every source, clause by
clause and table by table.

**Patents.** AAC may be subject to patent licensing in some jurisdictions;
Via LA administers a licensing programme for AAC. Nothing here is a licence
to any patent, and the authors make no claim about whether anyone needs one.
SBR, parametric stereo and USAC are deliberately absent.

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
```

## License

Open Encoding Attribution License v1.0 — a source-available (not OSI open-source)
license, royalty-free, with a commercial-attribution requirement. See
[LICENSE.md](LICENSE.md) and [NOTICE](NOTICE).
