# Blackmoon

[![Crates.io](https://img.shields.io/crates/v/blackmoon.svg)](https://crates.io/crates/blackmoon)
[![License](https://img.shields.io/crates/l/blackmoon.svg)](https://github.com/lucidaeon/mediumcoeli#license)

A command-line astrology data manager — reads any supported target, writes any supported target, deduplicates everything in between.

## What it does

One CLI verb (run with no subcommand). Inputs are file paths or a web account; the output is another file or another web account. Target type is detected from the file extension (`.SFcht`, `.zdb`, `.xml`) or specified explicitly with `--from` / `--to` / `--target`.

```text
blackmoon input.zdb --output out.SFcht
blackmoon a.SFcht b.zdb export.xml --output merged.SFcht
blackmoon --from luna --luna-session $COOKIE --output charts.SFcht
blackmoon --from astro --astro-user me@x.com --astro-pass ... --output charts.SFcht
blackmoon --from astrotheoros --astrotheoros-user me@x.com --astrotheoros-pass ... --output charts.SFcht
blackmoon charts.SFcht --normalize
```

Targets currently wired up:

- **File:** Solar Fire `.SFcht` binary (cp1252), Zeus `.zdb` semicolon-text, Astrodatabank `.xml`.
- **Web (authenticated):** lunaastrology.com (`--from/--to luna`), astro.com (`--from/--to astro`) — full CRUD including `--delete <ids>` for astro.com.

`--normalize` strips non-cp1252 characters and collapses whitespace; with no `--output`, it edits each input file in place. `--output now.SFcht` substitutes a UTC timestamp.

## Why it exists

Astrologers can have their data spread out across many tools. Consolidating records is time consuming and error prone. Blackmoon automates the task entirely.


## How it works

`blackmoon` is a thin CLI over [`astrogram`](https://github.com/lucidaeon/mediumcoeli/blob/main/crates/astrogram/README.md). The pipeline for every conversion run is the same five steps:

1. **Read the sink first** (if it already exists or is a web account) so the resulting set never gains duplicates. For LUNA® this is a cheap listing-only fetch keyed by `(name, full datetime)`; for astro.com it's a full chart fetch with `nhor` IDs; for files it's a normal parse.
2. **Read each input** in batch order, parsing through the per-format reader in `astrogram`. Inputs already present in the sink listing are filtered out before merge so duplicates never even reach the deduper.
3. **Merge and dedup** with `consolidate::merge_reporting`: same name + same date + time within ±2h + lat/lon within 0.1°. First-seen wins; the dropped names are reportable with `--verbose`.
4. **Optionally normalize** the merged set (cp1252 cleanup for any sink, mandatory before writing `.SFcht`).
5. **Write** the merged set to the chosen sink. Web sinks (`--to luna`, `--to astro`) carry an interactive y/N confirmation before any mutation, plus per-chart progress reporting.

Credentials come from flags or env vars (`LUNAASTROLOGY_COOKIE`, `ASTROCOM_COOKIE`, `ASTROCOM_USER`, `ASTROCOM_PASS`); the help output hides their values. `--delay` rate-limits HTTP requests. `--resume-from <prefix>` resumes an interrupted LUNA® fetch.

Credential sources are **not** mutually exclusive. When several are available for a target (a browser cookie via `--grant-cookie-access` (powered by [`wristband`](https://github.com/lucidaeon/mediumcoeli/blob/main/crates/wristband/README.md)), a token, and/or login creds), `blackmoon` tries them in order — **cookie → token → login** — and falls through to the next when one is rejected as stale (e.g. an expired cookie falls back to your saved password). It discloses which source authenticated, naming a fall-through when one occurred.

For **astro.com specifically**, the session cookie authenticates *reads only*; deleting a chart re-submits your account password. So `--delete`/`--consolidate` against astro.com requires `--astrocom-user`/`--astrocom-pass` (or `ASTROCOM_USER`/`ASTROCOM_PASS`) even when a working cookie is present — the cookie reads, the password deletes.

## In-place LUNA® consolidation

`blackmoon` can dedupe a LUNA® account in place by surfacing **candidate
groups** for human decision — no record is ever auto-dropped.

### One-off delete by UUID

```
blackmoon --luna-delete <uuid1>,<uuid2> --luna-session "$LUNAASTROLOGY_COOKIE"
```

Each UUID is deleted via `POST /phenomena/delete/<uuid>` (CakePHP DELETE
method tunnel).  Per-record progress is printed; failures are tallied and
the run exits non-zero if any delete failed.  `--delay` is honoured
between deletes.

### Interactive consolidation

```
blackmoon --target luna --consolidate --luna-session "$LUNAASTROLOGY_COOKIE"
```

The flow:

1. Fetch every chart in the LUNA® account.
2. Cluster duplicate candidates by **spacetime** tolerance: date equal,
   time within ±2h, latitude and longitude within 0.1° of each other.
   Names are *not* part of the trigger — they appear in the screen as a
   labelling cue, since the bug we are fixing is exactly the case where
   two charts for the same person have different names.
3. Walk one group per screen.  Press `a`/`b`/`c`/… to mark the keeper —
   every other chart in that group becomes a Drop.  Press `s` to skip the
   group (it will be re-prompted on the next run); press `q` to quit
   immediately (decisions already logged are still applied).
4. Each keystroke is fsync'd to the **decision log** before the loop
   advances, so a crash never loses a prior choice.
5. After the loop, a single confirmation gate (`y/N`) precedes the apply
   phase, which calls `delete_phenom` for every Drop record.  `--delay`
   is honoured between deletes.

Singleton groups (no other candidate within tolerance) are silently
filtered out — there is nothing to consolidate.

### Decision log

Default path: `$XDG_CACHE_HOME/blackmoon/luna-decisions.jsonl` (or
`~/.cache/blackmoon/luna-decisions.jsonl` when `XDG_CACHE_HOME` is unset).
Override with `--decision-log <PATH>`.

Each line is one JSON `DecisionRecord` with these fields:

- `group_id` — opaque cluster identifier (first non-empty phenom UUID in
  the group).
- `phenom_id` — LUNA® phenomenon UUID this decision applies to.
- `choice` — `"keep"`, `"drop"`, or `"skip"`.
- `chart_name` — display name at decision time, informational.

Re-running `--consolidate` reads the log first and silently skips any
group whose `group_id` is already decided, so resuming is a no-op if
everything was finished.  A partially-written trailing line (the
"crash mid-write" case) is silently skipped on read — the user is
re-prompted for that one record on the next run.
