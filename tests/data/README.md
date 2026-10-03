# Test streams

Short AAC streams (two seconds each) that the decoder is checked against,
made by an encoder other than this crate's so the decoder meets syntax this
crate never writes. `tests/faad_oracle.rs` decodes each with the decoder and
with faad2's `faad`, as a black box, and compares the PCM (HE-AAC and HE-AAC
v2 included, decoded in full); `tests/conformance.rs` checks that the
HE-AAC ones decode with SBR and PS, and that core-only mode gives their
AAC-LC core at half the rate. The decoder itself is held to ISO's
conformance streams (`tests/conformance.rs`).

Every stream was made by `tools/make_test_streams.py`, which drives the
Fraunhofer FDK AAC encoder library (libfdk-aac 2.0.3) through its public C
API with ctypes, from a test signal it makes itself (two tones, a beating
tone, noise bursts and decaying clicks per channel, different in every
channel), and writes the MP4 files itself; nothing else touched them:

```sh
FDK_AAC=/path/to/libfdk-aac.so.2 python3 tools/make_test_streams.py
```

| file | rate | layout | encoder settings |
|---|---|---|---|
| `fdk-lc-8000-mono-12k.aac` | 8000 | mono | AAC-LC, CBR 12 kb/s, ADTS |
| `fdk-lc-11025-mono-16k.aac` | 11025 | mono | AAC-LC, CBR 16 kb/s, ADTS |
| `fdk-lc-12000-mono-16k.aac` | 12000 | mono | AAC-LC, CBR 16 kb/s, ADTS |
| `fdk-lc-16000-stereo-32k.aac` | 16000 | stereo | AAC-LC, CBR 32 kb/s, ADTS |
| `fdk-lc-22050-stereo-40k.aac` | 22050 | stereo | AAC-LC, CBR 40 kb/s, ADTS |
| `fdk-lc-32000-mono-24k.aac` | 32000 | mono | AAC-LC, CBR 24 kb/s, ADTS |
| `fdk-lc-44100-stereo-vbr.m4a` | 44100 | stereo | AAC-LC, VBR mode 3, MP4 |
| `fdk-lc-48000-5_1-256k.aac` | 48000 | 5.1 | AAC-LC, CBR 256 kb/s, ADTS |
| `fdk-lc-48000-7_1-448k.m4a` | 48000 | 7.1 | AAC-LC, CBR 448 kb/s, MP4; MODE_7_1_REAR_SURROUND (a program_config_element) |
| `fdk-lc-64000-stereo-192k.aac` | 64000 | stereo | AAC-LC, CBR 192 kb/s, ADTS |
| `fdk-lc-88200-stereo-256k.aac` | 88200 | stereo | AAC-LC, CBR 256 kb/s, ADTS |
| `fdk-lc-96000-stereo-256k.m4a` | 96000 | stereo | AAC-LC, CBR 256 kb/s, MP4 |
| `he-aac-44100-stereo-implicit.aac` | 44100 | stereo | HE-AAC, 48 kb/s, ADTS (implicit signalling) |
| `he-aac-44100-stereo-backcompat.m4a` | 44100 | stereo | HE-AAC, 48 kb/s, MP4, backward-compatible explicit signalling |
| `he-aac-48000-stereo-explicit.m4a` | 48000 | stereo | HE-AAC, 64 kb/s, MP4, hierarchical explicit signalling |
| `he-aac-48000-5_1-implicit.aac` | 48000 | 5.1 | HE-AAC, 160 kb/s, ADTS (implicit) |
| `he-aac-v2-32000-stereo-implicit.aac` | 32000 | stereo | HE-AAC v2, 24 kb/s, ADTS (implicit) |
| `he-aac-v2-44100-stereo.m4a` | 44100 | stereo | HE-AAC v2, 32 kb/s, MP4, hierarchical explicit signalling |


The streams are encoder output only; no encoder's source is part of this
repository or was consulted to write it. A rerun with another libfdk-aac
version need not reproduce them byte for byte. The larger matrix
`tests/faad_oracle.rs` makes at test time (every layout from mono to 7.1,
every coding rate, 32–320 kb/s, AAC-LC, HE-AAC and HE-AAC v2) comes from
this crate's encoder and needs no committed files.
