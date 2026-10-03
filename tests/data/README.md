# Test streams

Short AAC streams (two seconds each) that the decoder is checked against,
made by encoders other than this crate's so the decoder meets syntax this
crate never writes. `tests/ffmpeg_oracle.rs` decodes each with the decoder
and with ffmpeg's, as a black box, and compares the PCM (or, for HE-AAC,
checks that the decoder's core-only mode gives the AAC-LC core at half the
rate). The HE-AAC decoding itself is checked against ISO's conformance
streams instead (`tests/conformance.rs`).

Every stream was made with the `ffmpeg` in the `linuxserver/ffmpeg` image
(ffmpeg 9.0, built with `libfdk_aac`), run only as a command-line tool, from
the same test signal `tests/ffmpeg_oracle.rs` uses (`source_expr`: two tones,
noise bursts and decaying clicks per channel, different in every channel):

```sh
ffmpeg -f lavfi -i "aevalsrc='<source_expr(channels)>':s=<rate>:c=<layout>:d=2" <encoder options> <file>
```

| file | rate | layout | encoder options |
|---|---|---|---|
| `fdk-lc-8000-mono-12k.aac` | 8000 | mono | `-c:a libfdk_aac -b:a 12k -f adts` |
| `fdk-lc-11025-mono-16k.aac` | 11025 | mono | `-c:a libfdk_aac -b:a 16k -f adts` |
| `fdk-lc-12000-mono-16k.aac` | 12000 | mono | `-c:a libfdk_aac -b:a 16k -f adts` |
| `fdk-lc-16000-stereo-32k.aac` | 16000 | stereo | `-c:a libfdk_aac -b:a 32k -f adts` |
| `fdk-lc-22050-stereo-40k.aac` | 22050 | stereo | `-c:a libfdk_aac -b:a 40k -f adts` |
| `fdk-lc-32000-mono-24k.aac` | 32000 | mono | `-c:a libfdk_aac -b:a 24k -f adts` |
| `fdk-lc-44100-stereo-vbr.m4a` | 44100 | stereo | `-c:a libfdk_aac -vbr 3` |
| `fdk-lc-48000-5_1-256k.aac` | 48000 | 5.1 | `-c:a libfdk_aac -b:a 256k -f adts` |
| `fdk-lc-48000-7_1-448k.m4a` | 48000 | 7.1 | `-c:a libfdk_aac -b:a 448k` |
| `fdk-lc-64000-stereo-192k.aac` | 64000 | stereo | `-c:a libfdk_aac -b:a 192k -f adts` |
| `fdk-lc-88200-stereo-256k.aac` | 88200 | stereo | `-c:a libfdk_aac -b:a 256k -f adts` |
| `fdk-lc-96000-stereo-256k.m4a` | 96000 | stereo | `-c:a libfdk_aac -b:a 256k` |
| `he-aac-44100-stereo-implicit.aac` | 44100 | stereo | `-c:a libfdk_aac -profile:a aac_he -b:a 48k -f adts` |
| `he-aac-44100-stereo-backcompat.m4a` | 44100 | stereo | `-c:a libfdk_aac -profile:a aac_he -b:a 48k -signaling explicit_sbr` |
| `he-aac-48000-stereo-explicit.m4a` | 48000 | stereo | `-c:a libfdk_aac -profile:a aac_he -b:a 64k -signaling explicit_hierarchical` |
| `he-aac-48000-5_1-implicit.aac` | 48000 | 5.1 | `-c:a libfdk_aac -profile:a aac_he -b:a 160k -f adts` |
| `he-aac-v2-32000-stereo-implicit.aac` | 32000 | stereo | `-c:a libfdk_aac -profile:a aac_he_v2 -b:a 24k -f adts` |
| `he-aac-v2-44100-stereo.m4a` | 44100 | stereo | `-c:a libfdk_aac -profile:a aac_he_v2 -b:a 32k -signaling explicit_hierarchical` |

The streams are encoder output only; no encoder's source is part of this
repository or was consulted to write it. ffmpeg's own AAC encoder makes the
larger matrix `tests/ffmpeg_oracle.rs` generates at test time (every layout
from mono to 7.1, program_config_element layouts, 22.05–48 kHz, 32–320
kb/s, CBR and VBR, ADTS and MP4, with and without PNS, intensity stereo and
TNS), which needs no committed files.
