# ─────────────────────────────────────────────────────────────────────────────
# mediumcoeli — workspace task runner
#
#   just            run the test suite (the default recipe)
#   just --list     browse every recipe, grouped
#   just --groups   list the groups
#   just docker …   multi-arch image builds — build · build-no-cache · push · release
#                   (defined in scripts/docker.just)
# ─────────────────────────────────────────────────────────────────────────────

mod docker 'scripts/docker.just'

# Run the workspace test suite.
default: test

# Most build/quality recipes take an optional CRATE: omit it to act on the whole
# workspace, or pass one (e.g. `just build astrogram`) to scope to a package.

# ── build ─────────────────────────────────────────────────────────────────────

# Compile the workspace (or one CRATE).
[group('build')]
build CRATE='':
	cargo build {{ if CRATE == '' { '--workspace' } else { '-p ' + CRATE } }}

# Compile with the release profile (or one CRATE).
[group('build')]
release CRATE='':
	cargo build --release {{ if CRATE == '' { '--workspace' } else { '-p ' + CRATE } }}
	{{ if CRATE == '' { 'cargo run --release -q -p starcat -- placements > docs/placements.md' } else { 'true' } }}

# Regenerate docs/placements.md from the pericynthion catalog (deterministic).
[group('build')]
placements:
	cargo run --release -q -p starcat -- placements > docs/placements.md

# Type-check without producing binaries (or one CRATE).
[group('build')]
check CRATE='':
	cargo check {{ if CRATE == '' { '--workspace' } else { '-p ' + CRATE } }}

# ── quality ───────────────────────────────────────────────────────────────────

# Run tests (release, --nocapture). Tests self-skip without STARCAT_JPL_DATA / ASTRO_SPECIMENS.
[group('qa')]
test CRATE='':
	cargo test --release {{ if CRATE == '' { '--workspace' } else { '-p ' + CRATE } }} -- --nocapture

# Clippy across all targets — lib, bin, tests, examples, benches (deny warnings).
[group('qa')]
lint CRATE='':
	cargo clippy {{ if CRATE == '' { '--workspace' } else { '-p ' + CRATE } }} --all-targets -- -D warnings

# Clippy on lib + bin only — no tests / examples / benches (deny warnings).
[group('qa')]
lint-narrow CRATE='':
	cargo clippy {{ if CRATE == '' { '--workspace' } else { '-p ' + CRATE } }} -- -D warnings

# Format every Rust file in the workspace.
[group('qa')]
fmt:
	cargo fmt --all

# Check formatting without writing changes.
[group('qa')]
fmt-check:
	cargo fmt --all -- --check

# Build docs with broken intra-doc links as hard errors (or one CRATE).
# docs.rs builds permissively and ships dead links as-is — this gate catches
# them before publish.
[group('qa')]
doc CRATE='':
	RUSTDOCFLAGS="-D warnings" cargo doc {{ if CRATE == '' { '--workspace' } else { '-p ' + CRATE } }} --no-deps --all-features

# ── distribution ──────────────────────────────────────────────────────────────

# Emit workspace crate names in publish-safe dependency order.
[group('dist')]
publish-order:
	#!/usr/bin/env bash
	set -euo pipefail
	members=$(for f in crates/*/Cargo.toml; do
	    awk '/^\[package\]/{p=1} p && /^name = /{split($0,a,"\""); print a[2]; exit}' "$f"
	done)
	for crate in $members; do
	    grep -E 'workspace = true' "crates/$crate/Cargo.toml" \
	        | grep -oE '^[a-zA-Z0-9_-]+' \
	        | grep -Fxf <(printf '%s\n' $members) \
	        | sed "s/.*/& $crate/" \
	        || true
	done | tsort

# Publish each crate in dependency order. DRY=--dry-run by default; pass DRY='' to publish for real.
[group('dist')]
publish DRY='--dry-run':
	#!/usr/bin/env bash
	set -euo pipefail
	# Preflight: fail before any crate ships if docs have broken links.
	# docs.rs builds permissively, so this is the last gate before they go public.
	just doc
	# One coordinated workspace publish (cargo's `package-workspace` feature):
	# cargo orders the crates by dependency and resolves intra-workspace versions
	# from the local tree, so a first-time cross-version bump — and its --dry-run —
	# succeeds without each sibling already being on crates.io. Supersedes the
	# old per-crate `publish-order` loop, which could neither dry-run a fresh
	# cross-bump nor satisfy each crate's isolated verify build.
	cargo publish --workspace {{DRY}}

# ── test corpora ──────────────────────────────────────────────────────────────

# Fetch a test corpus by name: de441 | adbxml | horizons.
[group('corpus')]
fetch SOURCE:
	#!/usr/bin/env bash
	set -euo pipefail
	case "{{SOURCE}}" in
	    de441)
	        # Mirror the JPL DE441 Linux ephemeris release into the current
	        # directory. --cut-dirs=4 strips ftp/eph/planets/Linux/ so files
	        # land in de441/. Used by pericynthion/starcat tests via $STARCAT_JPL_DATA.
	        wget \
	            --continue \
	            --retry-connrefused=on \
	            --read-timeout=20 \
	            --timeout=15 \
	            --tries=0 \
	            -e robots=off \
	            --cut-dirs=4 \
	            --level=0 \
	            --no-host-directories \
	            --no-parent \
	            --recursive \
	            --reject='index.html*' \
	            https://ssd.jpl.nasa.gov/ftp/eph/planets/Linux/de441/
	        ;;
	    adbxml)
	        # AdbXML public sample. Used by astrogram/blackmoon parser tests.
	        wget -q https://www.astro.com/adbexport/adb_export_sample.xml
	        ;;
	    horizons)
	        # Re-fetch every existing HORIZONS reference fixture under
	        # crates/pericynthion/tests/fixtures/ with --force. Iterates only over
	        # fixtures already on disk — won't create new ones. For a single
	        # fixture: ./scripts/horizons_fetch.py <chart_id> --mode <mode> --force
	        fixtures_dir=crates/pericynthion/tests/fixtures
	        for path in "$fixtures_dir"/horizons_*.json; do
	            rest=$(basename "$path" .json)
	            rest=${rest#horizons_}
	            case "$rest" in
	                *_topo)  chart=${rest%_topo};  mode=topocentric ;;
	                *_helio) chart=${rest%_helio}; mode=heliocentric ;;
	                *_geo)   chart=${rest%_geo};     mode=geocentric ;;
	            esac
	            printf '>>> refreshing chart=%s mode=%s\n' "$chart" "$mode"
	            ./scripts/horizons_fetch.py "$chart" --mode "$mode" --force
	        done
	        ;;
	    *)
	        echo "unknown fetch source: {{SOURCE}}" >&2
	        echo "supported sources: de441 adbxml horizons" >&2
	        exit 1
	        ;;
	esac
