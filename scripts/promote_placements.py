#!/usr/bin/env python3
"""Read name<TAB>note pairs from stdin; flip supported=false→true and update
the note in crates/pericynthion/src/placements.rs. Exits 0 if any change was
made, 1 if nothing changed (already promoted or empty input)."""
import re
import sys
from pathlib import Path

CATALOG = Path(__file__).parent.parent / "crates/pericynthion/src/placements.rs"

entries = []
for line in sys.stdin:
    line = line.rstrip("\n")
    if not line:
        continue
    parts = line.split("\t", 1)
    if len(parts) != 2:
        print(f"warning: malformed input line: {line!r}", file=sys.stderr)
        continue
    entries.append((parts[0], parts[1]))

if not entries:
    sys.exit(1)

content = CATALOG.read_text()
changed = False

for name, note in entries:
    # Match the multi-line pm("Name", Category::X, false, MPC, "old note") block.
    # Group 1: everything up to and including "false,"
    # Group 2: the MPC number line(s)
    # The note may contain backticks so use [^"]* to match any non-quote content.
    pattern = re.compile(
        r'(pm\(\s*"' + re.escape(name) + r'",\s*Category::\w+,\s*)false,'
        r'(\s*[\d_]+,\s*)"[^"]*"',
        re.DOTALL,
    )
    replacement = rf'\1true,\2"{note}"'
    new_content = pattern.sub(replacement, content)
    if new_content == content:
        print(f"  skip {name!r}: not found or already promoted", file=sys.stderr)
    else:
        content = new_content
        changed = True
        print(f"  promoted: {name}  ({note})", file=sys.stderr)

if changed:
    CATALOG.write_text(content)
    print(f"updated: {CATALOG}", file=sys.stderr)
    sys.exit(0)
else:
    sys.exit(1)
