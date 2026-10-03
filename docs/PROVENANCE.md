# Provenance

Where every part of this crate came from. The short version: the code is
this repository's own, written from the ISO/IEC standards and published
literature; the normative tables were transcribed from one copy of ISO/IEC
13818-7:2004 whose use the owner approved and, for SBR and parametric
stereo, one copy of ISO/IEC 14496-3:2009 (accepted 2026-10-02); HE-AAC is
validated against ISO's conformance streams; and other AAC implementations
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
clauses (no copy of the standard's text was used in writing the AAC-LC
decoder; the encoder's writers of the same structures came from the same
knowledge; the SBR and PS clauses, added later, were read from the copy
described under "HE-AAC and HE-AAC v2" below):
- The AudioSpecificConfig (1.6.2.1) and GASpecificConfig (4.4.1): object
  type with its escape, explicit sampling frequency, channel configuration,
  frameLengthFlag, dependsOnCoreCoder, extensionFlag.
- SBR and PS signalling (1.6.5): explicit hierarchical signalling (object
  types 5 and 29), the backward-compatible sync extensions `0x2b7` and
  `0x548`, and implicit signalling (SBR data in fill elements).
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
for the normative tables on 2026-09-28.** The only other copy of an AAC
standard fetched is the ISO/IEC 14496-3:2009 one described under "HE-AAC and
HE-AAC v2" below.

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

## HE-AAC and HE-AAC v2: spectral band replication and parametric stereo

Added on 2026-10-02 at the owner's request (see "What is not implemented"
below for the earlier decision this reverses).

**Sources.** ISO/IEC 14496-3:2009 (fourth edition), read for: the SBR
payload syntax and semantics (4.4.2.8, 4.5.2.8, Tables 4.62 to 4.74 and
4.104 to 4.122), the SBR tool's decoding process (4.6.18: frequency band
tables, time / frequency grid, envelope and noise floor decoding and
dequantisation, the QMF banks, HF generation, HF adjustment), the
informative SBR encoder description (4.B.18), the parametric stereo syntax
and semantics (8.4, 8.5.2), its decoding process (8.6.4) and its combination
with SBR (Annex 8.A), and the signalling of SBR and PS (1.6.5, 1.6.6). The
copy used was fetched on 2026-10-02 from
`https://csclub.uwaterloo.ca/~ehashman/ISO14496-3-2009.pdf`; its footer
identifies it as a licensee's copy ("LICENSED TO MECON Limited ... FOR
INTERNAL USE AT THIS LOCATION ONLY") re-hosted without ISO's authorisation,
like the 13818-7 copy below. It was accepted on 2026-10-02 on the same
basis as that copy, the owner having asked for such decisions to be settled
on their behalf.
No implementation's source was opened: not FFmpeg's aacsbr / aacps, faad2,
fdk-aac, Helix, the 3GPP or MPEG reference software or any other.

**Tables transcribed** (by a script reading the text of the PDF, as for
13818-7; checks below):

| table | what | where |
|---|---|---|
| 4.A.79–4.A.88 | the SBR envelope and noise floor Huffman tables | `tables/sbr.rs` |
| 4.A.89 | the 640 coefficients of the QMF bank window | `tables/sbr.rs` |
| 4.A.91 | the 512-entry noise table | `tables/sbr.rs` |
| 8.B.17–8.B.21 | the PS Huffman tables (IID, ICC, IPD, OPD) | `tables/ps.rs` |
| 8.24–8.29, 8.31 | PS mode configurations, quantisation grids, envelope counts | `tables/ps.rs`, by hand |
| 8.37–8.43 | hybrid filter prototypes, all-pass and fractional delay constants | `tables/ps.rs`, by hand |
| 8.45, 8.46, 8.48, 8.49 | stereo band maps | `tables/ps.rs`, by hand |

The window's `c[639]` is printed with nine decimals (`-0.000552528`), the
other 639 with ten; it is kept as printed. Checks: every Huffman table is a
complete prefix code (Kraft sum exactly 1) of the size its largest absolute
value implies, and every value round-trips through its codeword; the window
satisfies `c[i] = c[640 - i]` but at the four block boundaries, where the
table's sign pattern flips it; the hybrid prototypes' centre taps are 1/Q and
the taps a multiple of Q away are zero (the sub-bands add back to a delay);
the stereo band maps cover every band; and above all the decoder meets the
conformance references (next paragraph), which a slip in any of these tables
would break.

**Validation data.** The conformance bitstreams and reference waveforms of
ISO/IEC 14496-26 (second edition), ISO's publicly downloadable electronic
inserts at `https://standards.iso.org/iso-iec/14496/-26/ed-2/en/`, used as
data only: streams decoded, PCM compared. They are not in this repository;
`tools/fetch_conformance.py` fetches them. The conformance criterion (RMS of
the difference below 2^-15/sqrt(12), largest at most 2^-14) is the one 14496-4
and 14496-26 state for a 16-bit decoder, written from memory of those parts:
the text of 14496-26 was not available.

**Where the conformance references decided a reading.** Two places where the
text admits (or states) one thing and the references of 14496-26 show
another; the decoder follows the references, and says so in the code:

- *PS interpolation, first region* (8.6.4.6.4, special case a): the text
  writes `H(n) = H(n_-1) + n (H(n_0) - H(n_-1)) / n_0` for `n = 0 ... n_0 - 1`,
  which keeps slot 0 at the previous frame's value. With it, the PS streams
  that change their IID between frames (`al_sbr_ps_01`, `_03` to `_06`)
  were off by up to 670 LSB at those changes; the general formula with
  `n_-1 = -1`, i.e. `(n + 1) / (n_0 + 1)`, matches every reference to 0.05
  LSB.
- *Sinusoids deferred past a frame's end* (4.6.18.7.2): `delta_step` looks
  at `S'_IndexMapped` of the previous frame's last envelope. A sinusoid that
  starts in a frame whose `lA` equals its envelope count is silent there,
  and read literally stays silent in the next frame until its `lA`; in
  `al_sbr_cm_48_5` the reference continues it from the next frame's start.
  The decoder keeps the previous frame's transmitted `bs_add_harmonic` flags
  for `delta_step`, which matches the reference (and every other stream).

Two further choices the text leaves open: `bs_amp_res` as cleared by a
one-envelope FIXFIX grid applies to that channel's envelopes only (read as
one variable for a channel pair, the conformance streams do not parse); and
for implicit PS signalling the output turns stereo at the first PS data of a
mono stream, rather than for every mono SBR stream as 1.6.6.3 would have an
HE-AAC v2 decoder assume (the conformance references are mono for mono
HE-AAC streams and stereo for PS ones, which only this reading meets).

**The encoder** follows the informative 4.B.18 (analysis bank, envelope
estimation and quantisation, delta coding) and 8.C.6's outline for PS
parameters; its noise floor and inverse filtering estimate (a second-order
prediction gain of the original high band against its patch source), its
transient detector and its rate choices are this crate's own. Its timing
constant (`ALIGN`, 47 QMF slots) was derived from the banks' delays and
checked by measurement; `HE_AAC_DELAY` (3586 samples) was measured from the
round trip.

## What is not implemented, on purpose

USAC (xHE-AAC). AAC Main (prediction), SSR (gain control), LTP and the
coupling channel element are refused by name: AAC-LC encoders do not
produce them. The low power SBR tool and SBR in scalable or BSAC streams are
not implemented (a decoder may use the high quality tool everywhere).

Spectral band replication and parametric stereo were left out by the owner's
decision of 2026-09-28 (implementing them would put the code in the scope of
patents on them that may still be in force). On 2026-10-02 the owner asked
for them to be implemented; whether a use needs a patent licence is the
user's to determine.
