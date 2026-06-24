#!/usr/bin/env python3
"""One-off bootstrap: extract scripts/oracle_manifest.tsv from the current
crates/pericynthion/src/jpl/oracle_data.rs.

The current oracle_data.rs has rows of the OLD shape:
    OracleDir { prefix: "P", files: &[
        OracleFile { name: "N", size: S, blake3_hex: "H" },
    ] },
Emit one TSV line `P\tN\tS\tH` per file. Usage:
    extract_oracle_manifest.py <oracle_data.rs> > oracle_manifest.tsv
"""
import re, sys

src = open(sys.argv[1], encoding="utf-8").read()
# Match OracleDir blocks. The files array closes with ], (comma after bracket)
# before the closing }, of OracleDir, so we allow optional trailing comma after ].
dir_re = re.compile(
    r'OracleDir\s*\{\s*prefix:\s*"([^"]+)"\s*,\s*files:\s*&\[(.*?)\]\s*,?\s*\}',
    re.S,
)
# Rust struct fields have trailing commas; allow optional comma after blake3_hex value.
file_re = re.compile(
    r'OracleFile\s*\{\s*name:\s*"([^"]+)"\s*,\s*size:\s*(\d+)\s*,\s*blake3_hex:\s*"([0-9a-f]+)"\s*,?\s*\}',
)

rows = []
for d in dir_re.finditer(src):
    prefix, body = d.group(1), d.group(2)
    for f in file_re.finditer(body):
        name, size, h = f.group(1), f.group(2), f.group(3)
        rows.append((prefix, name, size, h))

for prefix, name, size, h in rows:
    print(f"{prefix}\t{name}\t{size}\t{h}")
