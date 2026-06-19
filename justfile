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

# ── distribution ──────────────────────────────────────────────────────────────

# Dry-run `cargo publish` for each crate in dependency order (or pass one CRATE).
[group('dist')]
publish CRATE='':
	#!/usr/bin/env bash
	set -euo pipefail
	# Dependency order: jzod → pericynthion → astrogram → starcat → blackmoon.
	crates=( jzod pericynthion astrogram starcat blackmoon )
	[[ -n "{{CRATE}}" ]] && crates=( "{{CRATE}}" )
	for crate in "${crates[@]}"; do
	    echo ">>> cargo publish --dry-run -p $crate"
	    cargo publish --dry-run -p "$crate"
	done

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
