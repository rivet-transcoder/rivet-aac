#!/usr/bin/env python3
"""Make the committed test streams in tests/data with fdk-aac's encoder.

The encoder is the Fraunhofer FDK AAC library (libfdk-aac), driven as a black
box through its public C API (aacenc_lib.h: aacEncOpen, aacEncoder_SetParam,
aacEncEncode, aacEncInfo) with ctypes; nothing of its source is read or used
here. The input is the test signal below, made here; the MP4 files are
written here too (one track, one chunk). No other tool is involved.

    python3 tools/make_test_streams.py [OUTDIR]       # default tests/data
    FDK_AAC=/path/to/libfdk-aac.so.2 python3 tools/make_test_streams.py

The encoder's output depends on its version, so a rerun need not reproduce
the committed files byte for byte; the tests only need streams with the
properties tests/data/README.md lists.
"""

import ctypes
import ctypes.util
import math
import os
import struct
import sys

# --- the test signal ---------------------------------------------------------


def signal(rate, channels, seconds=2.0):
    """Interleaved 16-bit PCM: per channel two tones, a beating tone, noise
    bursts (for PNS) and decaying clicks (for short windows and TNS),
    different in every channel. The noise is a fixed linear congruential
    sequence per channel, so the signal is the same on every run."""
    n = int(rate * seconds)
    state = [12345 + 7919 * c for c in range(channels)]
    out = []
    for i in range(n):
        t = i / rate
        for c in range(channels):
            state[c] = (state[c] * 1103515245 + 12345) & 0x7FFFFFFF
            rnd = state[c] / 0x80000000
            ph = 0.05 * c
            v = (
                0.22 * math.sin(2 * math.pi * (180 + 97 * c) * t)
                + 0.1 * math.sin(2 * math.pi * (1800 + 333 * c) * t) * math.sin(2 * math.pi * 0.7 * t)
                + (0.25 * (rnd - 0.5) if (t + ph) % 1.0 > 0.6 else 0.0)
                + 0.45 * math.exp(-60 * ((t + ph) % 0.37)) * math.sin(2 * math.pi * (2500 + 150 * c) * t)
            )
            out.append(max(-32768, min(32767, round(v * 32767))))
    return out


# --- fdk-aac, through its C API -----------------------------------------------

AACENC_AOT = 0x0100
AACENC_BITRATE = 0x0101
AACENC_BITRATEMODE = 0x0102
AACENC_SAMPLERATE = 0x0103
AACENC_CHANNELMODE = 0x0106
AACENC_CHANNELORDER = 0x0107
AACENC_AFTERBURNER = 0x0200
AACENC_TRANSMUX = 0x0300
AACENC_SIGNALING_MODE = 0x0302

IN_AUDIO_DATA = 0
OUT_BITSTREAM_DATA = 3

TT_MP4_RAW = 0
TT_MP4_ADTS = 2

AOT = {"lc": 2, "he": 5, "he2": 29}
# Channel modes (FDK_audio.h): 7.1 as MODE_7_1_REAR_SURROUND, which fdk-aac
# signals with a program_config_element (channel configuration 0).
MODE = {1: 1, 2: 2, 6: 6, 8: 33}
SIGNALLING = {"implicit": 0, "backcompat": 1, "explicit": 2}


class BufDesc(ctypes.Structure):
    _fields_ = [
        ("numBufs", ctypes.c_int),
        ("bufs", ctypes.POINTER(ctypes.c_void_p)),
        ("bufferIdentifiers", ctypes.POINTER(ctypes.c_int)),
        ("bufSizes", ctypes.POINTER(ctypes.c_int)),
        ("bufElSizes", ctypes.POINTER(ctypes.c_int)),
    ]


class InArgs(ctypes.Structure):
    _fields_ = [("numInSamples", ctypes.c_int), ("numAncBytes", ctypes.c_int)]


class OutArgs(ctypes.Structure):
    _fields_ = [
        ("numOutBytes", ctypes.c_int),
        ("numInSamples", ctypes.c_int),
        ("numAncBytes", ctypes.c_int),
        ("bitResState", ctypes.c_int),
    ]


class Info(ctypes.Structure):
    _fields_ = [
        ("maxOutBufBytes", ctypes.c_uint),
        ("maxAncBytes", ctypes.c_uint),
        ("inBufFillLevel", ctypes.c_uint),
        ("inputChannels", ctypes.c_uint),
        ("frameLength", ctypes.c_uint),
        ("nDelay", ctypes.c_uint),
        ("nDelayCore", ctypes.c_uint),
        ("confBuf", ctypes.c_ubyte * 64),
        ("confSize", ctypes.c_uint),
    ]


def load_fdk():
    path = os.environ.get("FDK_AAC") or ctypes.util.find_library("fdk-aac") or "libfdk-aac.so.2"
    lib = ctypes.CDLL(path)
    lib.aacEncOpen.argtypes = [ctypes.POINTER(ctypes.c_void_p), ctypes.c_uint, ctypes.c_uint]
    lib.aacEncoder_SetParam.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_uint]
    lib.aacEncEncode.argtypes = [ctypes.c_void_p] + [ctypes.c_void_p] * 4
    lib.aacEncInfo.argtypes = [ctypes.c_void_p, ctypes.POINTER(Info)]
    lib.aacEncClose.argtypes = [ctypes.POINTER(ctypes.c_void_p)]
    return lib


def buf_desc(buf, ident, size, el):
    bufs = (ctypes.c_void_p * 1)(ctypes.cast(buf, ctypes.c_void_p))
    return BufDesc(1, bufs, (ctypes.c_int * 1)(ident), (ctypes.c_int * 1)(size), (ctypes.c_int * 1)(el)), bufs


def encode(lib, pcm, rate, channels, profile, bitrate, vbr, transmux, signalling):
    """`(access units or ADTS frames, AudioSpecificConfig, frame length at the
    output rate)`."""
    h = ctypes.c_void_p()
    assert lib.aacEncOpen(ctypes.byref(h), 0, channels) == 0
    for param, value in [
        (AACENC_AOT, AOT[profile]),
        (AACENC_SAMPLERATE, rate),
        (AACENC_CHANNELMODE, MODE[channels]),
        (AACENC_CHANNELORDER, 1),  # WAV order in
        (AACENC_BITRATEMODE, vbr),
        (AACENC_AFTERBURNER, 1),
        (AACENC_TRANSMUX, transmux),
        (AACENC_SIGNALING_MODE, SIGNALLING[signalling]),
    ] + ([(AACENC_BITRATE, bitrate)] if not vbr else []):
        err = lib.aacEncoder_SetParam(h, param, value)
        assert err == 0, f"parameter {param:#x} = {value}: error {err:#x}"
    err = lib.aacEncEncode(h, None, None, None, None)
    assert err == 0, f"initialisation: error {err:#x}"
    info = Info()
    assert lib.aacEncInfo(h, ctypes.byref(info)) == 0
    asc = bytes(info.confBuf[: info.confSize])
    frame = info.frameLength * (2 if profile != "lc" else 1)

    out = (ctypes.c_ubyte * 20000)()
    out_desc, _keep_out = buf_desc(out, OUT_BITSTREAM_DATA, len(out), 1)
    units, pos, step = [], 0, info.frameLength * channels * (2 if profile != "lc" else 1)
    while True:
        chunk = pcm[pos : pos + step]
        arr = (ctypes.c_int16 * max(1, len(chunk)))(*chunk)
        in_desc, _keep_in = buf_desc(arr, IN_AUDIO_DATA, 2 * len(chunk), 2)
        in_args = InArgs(len(chunk) if chunk else -1, 0)
        out_args = OutArgs()
        err = lib.aacEncEncode(h, ctypes.byref(in_desc), ctypes.byref(out_desc), ctypes.byref(in_args), ctypes.byref(out_args))
        if err == 0x80:  # AACENC_ENCODE_EOF
            break
        assert err == 0, f"encode: error {err:#x}"
        pos += out_args.numInSamples
        if out_args.numOutBytes:
            units.append(bytes(out[: out_args.numOutBytes]))
    lib.aacEncClose(ctypes.byref(h))
    return units, asc, frame


# --- MP4 (ISO/IEC 14496-12 / -14), one AAC track ---------------------------------


def box(kind, body):
    return struct.pack(">I", len(body) + 8) + kind + body


def full(kind, body):
    return box(kind, b"\0\0\0\0" + body)


def mp4(units, asc, rate, channels, frame):
    n = len(units)
    duration = n * frame

    def desc(tag, body):
        return bytes([tag, len(body)]) + body

    dcd = desc(4, bytes([0x40, 0x15, 0, 0, 0]) + struct.pack(">II", 0, 0) + desc(5, asc))
    esds = full(b"esds", desc(3, b"\0\x01\0" + dcd + desc(6, b"\x02")))
    mp4a = box(
        b"mp4a",
        b"\0" * 6 + b"\0\x01" + b"\0" * 8 + struct.pack(">HH", channels, 16) + b"\0" * 4 + struct.pack(">I", min(rate, 65535) << 16) + esds,
    )
    stsd = full(b"stsd", struct.pack(">I", 1) + mp4a)
    stts = full(b"stts", struct.pack(">III", 1, n, frame))
    stsc = full(b"stsc", struct.pack(">IIII", 1, 1, n, 1))
    stsz = full(b"stsz", struct.pack(">II", 0, n) + b"".join(struct.pack(">I", len(u)) for u in units))
    ftyp = box(b"ftyp", b"M4A \0\0\0\0M4A mp42isom")
    matrix = struct.pack(">9I", 0x10000, 0, 0, 0, 0x10000, 0, 0, 0, 0x40000000)

    def moov(offset):
        stco = full(b"stco", struct.pack(">II", 1, offset))
        stbl = box(b"stbl", stsd + stts + stsc + stsz + stco)
        dinf = box(b"dinf", full(b"dref", struct.pack(">I", 1) + full(b"url ", b"")))
        minf = box(b"minf", full(b"smhd", b"\0" * 4) + dinf + stbl)
        hdlr = full(b"hdlr", b"\0" * 4 + b"soun" + b"\0" * 12 + b"\0")
        mdhd = full(b"mdhd", struct.pack(">IIII", 0, 0, rate, duration) + b"\x55\xc4\0\0")
        mdia = box(b"mdia", mdhd + hdlr + minf)
        tkhd = box(b"tkhd", b"\0\0\0\x07" + struct.pack(">IIIII", 0, 0, 1, 0, duration) + b"\0" * 8 + b"\0\0\0\0\x01\0\0\0" + matrix + b"\0" * 8)
        mvhd = full(b"mvhd", struct.pack(">IIIII", 0, 0, rate, duration, 0x10000) + b"\x01\0" + b"\0" * 10 + matrix + b"\0" * 24 + struct.pack(">I", 2))
        return box(b"moov", mvhd + box(b"trak", tkhd + mdia))

    size = len(moov(0))
    return ftyp + moov(len(ftyp) + size + 8) + box(b"mdat", b"".join(units))


# --- the set ----------------------------------------------------------------------

# (file, rate, channels, profile, bit rate (0: VBR mode 3), signalling)
STREAMS = [
    ("fdk-lc-8000-mono-12k.aac", 8000, 1, "lc", 12000, "implicit"),
    ("fdk-lc-11025-mono-16k.aac", 11025, 1, "lc", 16000, "implicit"),
    ("fdk-lc-12000-mono-16k.aac", 12000, 1, "lc", 16000, "implicit"),
    ("fdk-lc-16000-stereo-32k.aac", 16000, 2, "lc", 32000, "implicit"),
    ("fdk-lc-22050-stereo-40k.aac", 22050, 2, "lc", 40000, "implicit"),
    ("fdk-lc-32000-mono-24k.aac", 32000, 1, "lc", 24000, "implicit"),
    ("fdk-lc-44100-stereo-vbr.m4a", 44100, 2, "lc", 0, "implicit"),
    ("fdk-lc-48000-5_1-256k.aac", 48000, 6, "lc", 256000, "implicit"),
    ("fdk-lc-48000-7_1-448k.m4a", 48000, 8, "lc", 448000, "implicit"),
    ("fdk-lc-64000-stereo-192k.aac", 64000, 2, "lc", 192000, "implicit"),
    ("fdk-lc-88200-stereo-256k.aac", 88200, 2, "lc", 256000, "implicit"),
    ("fdk-lc-96000-stereo-256k.m4a", 96000, 2, "lc", 256000, "implicit"),
    ("he-aac-44100-stereo-implicit.aac", 44100, 2, "he", 48000, "implicit"),
    ("he-aac-44100-stereo-backcompat.m4a", 44100, 2, "he", 48000, "backcompat"),
    ("he-aac-48000-stereo-explicit.m4a", 48000, 2, "he", 64000, "explicit"),
    ("he-aac-48000-5_1-implicit.aac", 48000, 6, "he", 160000, "implicit"),
    ("he-aac-v2-32000-stereo-implicit.aac", 32000, 2, "he2", 24000, "implicit"),
    ("he-aac-v2-44100-stereo.m4a", 44100, 2, "he2", 32000, "explicit"),
]


def main():
    outdir = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(__file__), "..", "tests", "data")
    lib = load_fdk()
    for name, rate, channels, profile, bitrate, signalling in STREAMS:
        adts = name.endswith(".aac")
        units, asc, frame = encode(
            lib, signal(rate, channels), rate, channels, profile, bitrate, 0 if bitrate else 3, TT_MP4_ADTS if adts else TT_MP4_RAW, signalling
        )
        data = b"".join(units) if adts else mp4(units, asc, rate, channels, frame)
        with open(os.path.join(outdir, name), "wb") as f:
            f.write(data)
        print(f"{name}: {len(units)} frames, {len(data)} bytes, ASC {asc.hex()}")


if __name__ == "__main__":
    main()
