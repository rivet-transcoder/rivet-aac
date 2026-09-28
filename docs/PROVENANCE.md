# Provenance

Where every part of this crate came from. The short version: the code is
this repository's own, written from the ISO/IEC standards and published
literature; the normative tables were transcribed from one copy of ISO/IEC
13818-7:2004 whose use the owner approved; and other AAC implementations
were used only as black boxes, never read.

## Clean-room rules

- **No AAC implementation's source was opened or read** in writing either
  half: not FFmpeg's (libavcodec), faad2, fdk-aac, FAAC, symphonia, NihAV,
  the 3GPP reference code, Apple's, Nero's, VisualOn's or any other.
- **ffmpeg and ffprobe were used only as command-line tools**, as black-box
  oracles: to make test streams, to decode them, and to compare the PCM.
  The `ffmpeg` in Debian bookworm (5.1) decodes in the tests; the one in the
  `linuxserver/ffmpeg` image (9.0, with `libfdk_aac`) made the committed
  streams in `tests/data` ([its README](../tests/data/README.md) lists each
  command). No table, constant or behaviour was derived by probing a decoder;
  every figure the tests compare was computed from the standard first.
- Where the standard leaves a choice to the decoder and a comparison showed
  another decoder choosing differently (a program_config_element whose
  elements do not fit its own position rules, an LFE that breaks subclause
  8.4's restrictions), this crate follows the standard's text and says what
  it does; it does not copy the other decoder's choice.

## One place the standard contradicts itself

13818-7 12.2 says intensity stereo's phase "is changed … if the
corresponding ms_used bit is set", and 12.1.2 defines ms_mask_present 2 as
ms_used "all ones"; but the `invert_intensity()` pseudo-code of 12.2.3
inverts only when ms_mask_present is 1. The decoder was first written to the
pseudo-code. A stream from ffmpeg's encoder with M/S forced on every band
(ms_mask_present 2) and intensity stereo then disagreed with ffmpeg's
decoder in its intensity bands (12.7 dB on the right channel, 139 dB on the
left). Rereading the two clauses showed the conflict; the decoder now
follows the prose — an ms_used bit that is set, whether sent in a mask or
implied by "all ones", inverts the band — and agrees (138.6 dB). The
comparison exposed the question; the answer is the standard's own text.

## The standards, by part

**ISO/IEC 13818-7:2004** (MPEG-2 AAC), read from the copy described under
"The tables" below:
- Clause 6: the ADTS header and frame (Tables 4–10, including the CRC and
  raw_data_block_position fields of a multi-block frame) and the
  raw_data_block syntax (Tables 11–26, 28): SCE, CPE, CCE (recognised and
  refused), LFE, DSE, PCE, FIL and extension_payload.
- Clause 7.1.6: TNS_MAX_ORDER and TNS_MAX_BANDS (Table 33).
- Clause 8: element semantics, the program_config_element (8.5), the
  extension types (Table 40), the LFE restrictions (8.4), the implicit
  speaker mapping of the channel configurations (Table 42), window sequences
  (Tables 43–44), the scalefactor band tables (Tables 45–57), grouping and
  the order of spectral data (8.3.4–8.3.5), the sampling-frequency mapping
  of an explicit rate (Table 38).
- Clause 9: noiseless coding — codeword indices, sign bits, escape
  sequences, the pulse tool, de-interleaving (9.3), Table 59's codebook
  parameters.
- Clauses 10–11: inverse quantisation (`|q|^(4/3)`) and scalefactors
  (`2^(0.25 (sf - 100))`).
- Clause 12: M/S (12.1) and intensity stereo (12.2), including the phase
  inversion by `ms_used`.
- Clause 14: TNS — `tns_data()`, `tns_decode_coef()`, the all-pole filter
  and its span.
- Clause 15: the IMDCT (15.3.1), the sine and KBD window shapes and the four
  window sequences with the previous frame's shape on the left half (15.3.2).
- Informative Annex C, for the encoder only: see "The encoder" below.

**ISO/IEC 14496-3** (MPEG-4 Audio), from the published syntax of these
clauses (no copy of the standard's text was used in writing the decoder; the
encoder's writers of the same structures came from the same knowledge):
- The AudioSpecificConfig (1.6.2.1) and GASpecificConfig (4.4.1): object
  type with its escape, explicit sampling frequency, channel configuration,
  frameLengthFlag, dependsOnCoreCoder, extensionFlag.
- SBR and PS signalling (1.6.5): explicit hierarchical signalling (object
  types 5 and 29), the backward-compatible sync extensions `0x2b7` and
  `0x548`, and implicit signalling (SBR data in fill elements). Only the
  signalling is read: the SBR and PS payloads are skipped, never parsed.
- The sampling_frequency_index 12 (7350 Hz), which uses the 8 kHz tables.
- Perceptual noise substitution (4.6.13): the noise codebook 13, the first
  noise energy as a 9-bit PCM value offset by `global_gain - 90` and 256, the
  rest Huffman-coded differences, the band's noise scaled to energy
  `2^(0.5 nrg)`, and a pair's shared noise where `ms_used` is set.

**Literature** (for the transforms, shared by both halves): Princen &
Bradley, IEEE TASSP 34(5), 1986 (TDAC); Malvar, *Signal Processing with
Lapped Transforms*, 1992, and Britanak, Yip & Rao, *Discrete Cosine and Sine
Transforms*, 2007 (the MDCT and IMDCT through a quarter-length FFT, as a
DCT-IV); the modified Bessel function's power series for the KBD kernel.

## The tables

The normative tables were transcribed, by a script reading the PDF's text
positions, from a copy of ISO/IEC 13818-7:2004 retrieved on 2026-09-27 from
`https://ossrs.net/lts/zh-cn/assets/files/ISO_IEC_13818-7-AAC-2004-67b015c6ddfc9a4af83665738477124a.pdf`.
Its footer identifies it as a licensee's copy ("Reproduced by IHS under
license with ISO … IHS Licensee=etri") re-hosted without authorisation — not
a purchased copy. **The owner reviewed this and explicitly approved using it
for the normative tables on 2026-09-28.** No other copy of any AAC standard
was fetched or used.

| table | what | where | transcribed |
|---|---|---|---|
| A.1–A.12 | the scalefactor and eleven spectrum Huffman codebooks | `tables/codebooks.rs` | 2026-09-27, for the encoder |
| 35 | sampling_frequency_index | `tables/swb.rs` | 2026-09-27, for the encoder |
| 45, 46, 47, 52, 53 | scalefactor bands, 22.05–48 kHz | `tables/swb.rs` | 2026-09-27, for the encoder |
| 59 | codebook dimension, signedness, largest value | `tables/codebooks.rs` | 2026-09-27, for the encoder |
| 48–51, 54–57 | scalefactor bands, 8–16 kHz and 64–96 kHz | `tables/swb.rs` | 2026-09-28, for the decoder |
| 33 | TNS_MAX_BANDS (AAC-LC, long and short windows) | `tables/swb.rs` | 2026-09-28, for the decoder |
| 38 | the rate ranges an explicit frequency maps by | `tables/swb.rs` | 2026-09-28, for the decoder |

Checks that a transcription slip would fail: every codebook is a complete
prefix code of the size Table 59 gives (Kraft sum exactly 1) and every
codeword decodes to its own index; every band table starts at 0, rises in
multiples of four and ends at 1024 or 128; every TNS_MAX_BANDS fits its
table; and the decoder's PCM agrees with ffmpeg's at every sampling rate the
tables cover (see the README's figures).

Nothing else was transcribed. The windows are computed from their formulas
(`tables/windows.rs`), the IMDCT from its definition (`mdct.rs`, tested
against the direct sum), and every algorithm is this crate's own.

## The encoder

Written from 13818-7 clauses 6, 8–12, 15 and Annex A as above, 14496-3's
AudioSpecificConfig and ADTS form, and, from 13818-7's informative Annex C,
the structure of the psychoacoustic model and its spreading function (C.1),
the MDCT (C.3), M/S (C.6.1), the quantiser and its rounding constant, the bit
reservoir control (C.7) and sectioning (C.8). Literature: Johnston, IEEE
JSAC 6(2), 1988; Zwicker & Terhardt, JASA 68(5), 1980; Terhardt, Hearing
Research 1, 1979; Johnston & Ferreira, ICASSP 1992; Herre & Johnston, AES
101st Convention, 1996. No encoder's source was consulted: not FDK-AAC,
FAAC, FFmpeg's, Nero's, VisualOn's or Apple's. Its history (moved here from
the rivet repository with `git filter-repo`) carries the original commits.

## What is not implemented, on purpose

Spectral band replication, parametric stereo and USAC — HE-AAC, HE-AAC v2 and
xHE-AAC. An HE-AAC stream decodes as its AAC-LC core; see the README. This
is the owner's decision (2026-09-28): implementing those tools would put the
code in the scope of patents on them that are still in force, and the owner
is avoiding that exposure. AAC Main (prediction), SSR (gain control), LTP and
the coupling channel element are refused by name: AAC-LC encoders do not
produce them.
