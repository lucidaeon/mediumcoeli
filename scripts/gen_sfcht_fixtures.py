#!/usr/bin/env python3
"""
Generate JSON golden fixtures from SFcht specimens for Rust record-parser tests.

Reads:  all *.SFcht files found recursively under the specimens directory.
        Resolved in order: CLI argument > ASTRO_SPECIMENS env var.
Writes: ../crates/astrogram/tests/fixtures/sfcht/<stem>.json

All coordinates stored in ISO 6709 convention (East-positive longitude,
East-positive tz_offset) to match the canonical Chart type.
"""

import os
import struct
import json
import sys
from pathlib import Path

SCRIPT_DIR   = Path(__file__).parent.resolve()
FIXTURES_DIR = SCRIPT_DIR.parent / "crates" / "astrogram" / "tests" / "fixtures" / "sfcht"


def resolve_specimens_dir(argv):
    if len(argv) > 1:
        return Path(argv[1]).expanduser().resolve()
    env = os.environ.get("ASTRO_SPECIMENS")
    if env:
        return Path(env).expanduser().resolve()
    return None

HEADER_SIZE        = 86
MAIN_RECORD_SIZE   = 296
SUB_RECORD_SIZE    = 115


def _str(data, off, n):
    raw = data[off:off + n].replace(b"\x00", b"")
    return raw.decode("cp1252", errors="replace").rstrip() or None


def _u8(data, off):   return data[off]
def _u16(data, off):  return struct.unpack_from("<H", data, off)[0]
def _i16(data, off):  return struct.unpack_from("<h", data, off)[0]
def _u32(data, off):  return struct.unpack_from("<I", data, off)[0]
def _f32(data, off):  return struct.unpack_from("<f", data, off)[0]


def parse_header(data):
    return {
        "version":      _u16(data, 0),
        "description":  _str(data, 2, 80),
        "record_count": _u16(data, 82),
    }


def parse_sub_chart(data, p):
    lon_sf   = _f32(data, p + 90)
    tz_sf    = _f32(data, p + 105)
    notes_n  = _u32(data, p + SUB_RECORD_SIZE)
    q        = p + SUB_RECORD_SIZE + 4
    notes    = None
    if notes_n:
        raw   = data[q:q + notes_n].replace(b"\x00", b"")
        notes = raw.decode("cp1252", errors="replace") or None
    q += notes_n
    return {
        "name":      _str(data, p,       50),
        "city":      _str(data, p + 50,  20),
        "region":    _str(data, p + 70,  20),
        "longitude": round(-lon_sf, 8),   # ISO 6709: negate SF +West
        "latitude":  round(_f32(data, p + 94), 8),
        "year":      _i16(data, p + 98),
        "month":     _u8(data,  p + 100),
        "day":       _u8(data,  p + 101),
        "hour":      _u8(data,  p + 102),
        "minute":    _u8(data,  p + 103),
        "second":    _u8(data,  p + 104),
        "tz_offset": round(-tz_sf, 8),    # ISO 6709: negate SF +West
        "tz_abbrev": _str(data, p + 109, 5),
        "is_lmt":    _u8(data,  p + 114) == 1,
        "notes":     notes,
    }, q - p


def parse_record(data, p):
    lon_sf = _f32(data, p + 92)
    tz_sf  = _f32(data, p + 107)
    n_sc   = _u32(data, p + 292)
    q      = p + MAIN_RECORD_SIZE
    scs    = []
    for _ in range(n_sc):
        sc, consumed = parse_sub_chart(data, q)
        scs.append(sc)
        q += consumed
    notes_n = _u32(data, q)
    q += 4
    notes = None
    if notes_n:
        raw   = data[q:q + notes_n].replace(b"\x00", b"")
        notes = raw.decode("cp1252", errors="replace") or None
    q += notes_n
    return {
        "record_index":    _u16(data, p + 158),
        "name":            _str(data, p + 2,   50),
        "secondary_name":  _str(data, p + 162, 50),
        "city":            _str(data, p + 52,  20),
        "region":          _str(data, p + 72,  20),
        "longitude":       round(-lon_sf, 8),
        "latitude":        round(_f32(data, p + 96), 8),
        "year":            _i16(data, p + 100),
        "month":           _u8(data,  p + 102),
        "day":             _u8(data,  p + 103),
        "hour":            _u8(data,  p + 104),
        "minute":          _u8(data,  p + 105),
        "second":          _u8(data,  p + 106),
        "tz_offset":       round(-tz_sf, 8),
        "tz_abbrev":       _str(data, p + 111, 5),
        "is_lmt":          _u8(data,  p + 116) == 1,
        "event_type":      _u8(data,  p + 117),
        "source_rating":   _str(data, p + 118, 32),
        "house_system":    _u8(data,  p + 151),
        "zodiac":          _u8(data,  p + 152),
        "coordinate_system": _u8(data, p + 157),
        "sub_charts":      scs,
        "notes":           notes,
    }, q - p


def generate(path):
    data    = path.read_bytes()
    hdr     = parse_header(data)
    records = []
    pos     = HEADER_SIZE
    for _ in range(hdr["record_count"]):
        rec, size = parse_record(data, pos)
        records.append(rec)
        pos += size
    return {
        "source_file":  path.name,
        "version":      hdr["version"],
        "description":  hdr["description"],
        "record_count": hdr["record_count"],
        "records":      records,
    }


def main():
    specimens_dir = resolve_specimens_dir(sys.argv)
    if specimens_dir is None:
        print("usage: gen_sfcht_fixtures.py <specimens-dir>  (or set ASTRO_SPECIMENS)", file=sys.stderr)
        sys.exit(1)
    if not specimens_dir.exists():
        print(f"specimens dir not found: {specimens_dir}", file=sys.stderr)
        sys.exit(1)

    FIXTURES_DIR.mkdir(parents=True, exist_ok=True)

    ok = skip = err = 0
    for path in sorted(specimens_dir.rglob("*.SFcht")):
        try:
            fix = generate(path)
            out = FIXTURES_DIR / (path.stem + ".json")
            out.write_text(json.dumps(fix, indent=2, ensure_ascii=False))
            print(f"  ok    {path.name}: {fix['record_count']} records")
            ok += 1
        except Exception as e:
            print(f"  ERROR {path.name}: {e}")
            err += 1

    print(f"\n{ok} generated, {skip} skipped, {err} errors")
    sys.exit(1 if err else 0)


if __name__ == "__main__":
    main()
