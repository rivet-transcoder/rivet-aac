#!/usr/bin/env python3
"""Fetch the HE-AAC and HE-AAC v2 conformance streams of ISO/IEC 14496-26
(second edition) and their reference waveforms, for tests/conformance.rs.

ISO publishes the conformance package as electronic inserts at
https://standards.iso.org/iso-iec/14496/-26/ed-2/en/ (several large ZIP
archives). This script reads each archive's central directory with HTTP range
requests and extracts only the members the test uses, unmodified, into one
directory:

    python3 tools/fetch_conformance.py DIR
    AAC_CONFORMANCE_DIR=DIR cargo test --release --test conformance

The streams (compressedMp4.zip): the AAC-LC + SBR ones (al_sbr_*) with 1024-
sample frames and channel configurations 1 to 6, and the PS ones
(al_sbr_ps_*). The references (referencesWav.zip): the high quality SBR
decoder's (al_sbr_hq_*) and the unrestricted PS decoder's (al_sbr_ps_*_ur).
Nothing is decoded here; ISO's licence terms for the inserts apply to the
files.
"""

import os
import struct
import sys
import urllib.request
import zlib

BASE = "https://standards.iso.org/iso-iec/14496/-26/ed-2/en/"

STREAMS = [
    "al_sbr_cm_48_1", "al_sbr_cm_48_2", "al_sbr_cm_48_4", "al_sbr_cm_48_5", "al_sbr_cm_48_5.1",
    "al_sbr_cm_96_5",
    "al_sbr_e_32_1", "al_sbr_e_32_2", "al_sbr_e_44_1", "al_sbr_e_44_2", "al_sbr_e_48_1", "al_sbr_e_48_2",
    "al_sbr_gh_32_1", "al_sbr_gh_32_2", "al_sbr_gh_44_1", "al_sbr_gh_44_2", "al_sbr_gh_48_1", "al_sbr_gh_48_2",
    "al_sbr_i_32_1", "al_sbr_i_32_1_new", "al_sbr_i_32_2", "al_sbr_i_44_1", "al_sbr_i_44_1_new", "al_sbr_i_44_2",
    "al_sbr_i_48_1", "al_sbr_i_48_1_new", "al_sbr_i_48_2",
    "al_sbr_qmf_32_1", "al_sbr_qmf_44_1", "al_sbr_qmf_48_1",
    "al_sbr_s_32_1", "al_sbr_s_32_2", "al_sbr_s_44_1", "al_sbr_s_44_2", "al_sbr_s_48_1", "al_sbr_s_48_2",
    "al_sbr_sig_24_2_fsaac24_sig1", "al_sbr_sig_48_2_sig0", "al_sbr_sig_48_2_sig1", "al_sbr_sig_48_2_sig2",
    "al_sbr_sr_16_2_fsaac08", "al_sbr_sr_16_2_fsaac16", "al_sbr_sr_22_2_fsaac11", "al_sbr_sr_22_2_fsaac22",
    "al_sbr_sr_24_2_fsaac12", "al_sbr_sr_24_2_fsaac24", "al_sbr_sr_32_2_fsaac16", "al_sbr_sr_32_2_fsaac32",
    "al_sbr_sr_44_2_fsaac22", "al_sbr_sr_44_2_fsaac44", "al_sbr_sr_48_2_fsaac24", "al_sbr_sr_48_2_fsaac48",
    "al_sbr_sr_64_2_fsaac32", "al_sbr_sr_88_2_fsaac44", "al_sbr_sr_96_2_fsaac48",
    "al_sbr_twi_22_1_fsaac22", "al_sbr_twi_48_1_fsaac24",
] + [f"al_sbr_ps_0{n}{s}" for n in range(7) for s in ("", "_new")] + ["al_sbr_ps_03_sig1", "al_sbr_ps_03_sig2"]


def rng(url, a, b):
    req = urllib.request.Request(url, headers={"Range": f"bytes={a}-{b}"})
    with urllib.request.urlopen(req, timeout=600) as r:
        return r.read()


def size(url):
    req = urllib.request.Request(url, method="HEAD")
    with urllib.request.urlopen(req, timeout=60) as r:
        return int(r.headers["Content-Length"])


def central_directory(url):
    """`(name, method, compressed size, offset)` of every member (ZIP64 aware)."""
    n = size(url)
    tail = rng(url, max(0, n - 70000), n - 1)
    i = tail.rfind(b"PK\x05\x06")
    cd_size, cd_off = struct.unpack("<II", tail[i + 12:i + 20])
    j = tail.rfind(b"PK\x06\x06")
    if j >= 0:
        _, cd_size, cd_off = struct.unpack("<QQQ", tail[j + 32:j + 56])
    cd = rng(url, cd_off, cd_off + cd_size - 1)
    out, p = [], 0
    while p < len(cd) and cd[p:p + 4] == b"PK\x01\x02":
        meth, = struct.unpack("<H", cd[p + 10:p + 12])
        csz, usz = struct.unpack("<II", cd[p + 20:p + 28])
        fl, el, cl = struct.unpack("<HHH", cd[p + 28:p + 34])
        off, = struct.unpack("<I", cd[p + 42:p + 46])
        name = cd[p + 46:p + 46 + fl].decode("utf8", "replace")
        ex, q = cd[p + 46 + fl:p + 46 + fl + el], 0
        while q + 4 <= len(ex):
            hid, hl = struct.unpack("<HH", ex[q:q + 4])
            d, k = ex[q + 4:q + 4 + hl], 0
            if hid == 1:
                if usz == 0xFFFFFFFF:
                    usz, = struct.unpack("<Q", d[k:k + 8]); k += 8
                if csz == 0xFFFFFFFF:
                    csz, = struct.unpack("<Q", d[k:k + 8]); k += 8
                if off == 0xFFFFFFFF:
                    off, = struct.unpack("<Q", d[k:k + 8]); k += 8
            q += 4 + hl
        out.append((name, meth, csz, off))
        p += 46 + fl + el + cl
    return out


def extract(url, entry, dest):
    name, meth, csz, off = entry
    h = rng(url, off, off + 29)
    fl, el = struct.unpack("<HH", h[26:30])
    data = rng(url, off + 30 + fl + el, off + 30 + fl + el + csz - 1)
    if meth == 8:
        data = zlib.decompress(data, -15)
    with open(dest, "wb") as f:
        f.write(data)


def fetch(archive, wanted, outdir):
    url = BASE + archive
    for entry in central_directory(url):
        base = entry[0].rsplit("/", 1)[-1]
        if base in wanted and not os.path.exists(os.path.join(outdir, base)):
            print("fetch", archive, base, flush=True)
            extract(url, entry, os.path.join(outdir, base))


def main():
    outdir = sys.argv[1] if len(sys.argv) > 1 else "conformance"
    os.makedirs(outdir, exist_ok=True)
    fetch("compressedMp4.zip", {s + ".mp4" for s in STREAMS}, outdir)
    refs = set()
    for s in STREAMS:
        if s.startswith("al_sbr_ps_"):
            refs.add(f"al_sbr_ps_{s[10:12]}_ur.wav")
        else:
            rest = s[len("al_sbr_"):]
            refs.add(f"al_sbr_hq_{rest}.wav")
            refs.update(f"al_sbr_hq_{rest}_f0{c}.wav" for c in range(6))
            refs.add(f"al_sbr_hq_{rest}_l00.wav")
    fetch("referencesWav.zip", refs, outdir)


if __name__ == "__main__":
    main()
