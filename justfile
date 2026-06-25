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

# Verify + promote unsupported catalog bodies (fetches from Horizons if needed),
# then regenerate docs/placements.md. Idempotent: already-promoted bodies are skipped.
# Requires $STARCAT_JPL_DATA (for n373 KBO perturbers); $STARCAT_HORIZONS_DATA optional.
[group('build')]
placements:
	#!/usr/bin/env bash
	set -euo pipefail
	if confirmed=$(cargo run --release -q -p starcat -- placements --verify 2>/dev/null) \
	        && [ -n "$confirmed" ]; then
	    printf '%s\n' "$confirmed" | python3 scripts/promote_placements.py || true
	    cargo build --release -q -p starcat
	fi
	cargo run --release -q -p starcat -- placements | tee docs/placements.md

# Show which unsupported catalog bodies are confirmable without modifying anything.
# Requires $STARCAT_JPL_DATA. Does not fetch from Horizons.
[group('build')]
placements-dry-run:
	cargo run --release -q -p starcat -- placements --verify --dry-run
	cargo run --release -q -p starcat -- placements

# Regenerate crates/pericynthion/src/jpl/oracle_data.rs from the manifest and
# immediately run cargo fmt so the committed file is the generate-then-fmt fixed point.
[group('build')]
oracle-regen:
	#!/usr/bin/env bash
	set -euo pipefail
	python3 scripts/gen_oracle.py scripts/oracle_manifest.tsv \
	    > crates/pericynthion/src/jpl/oracle_data.rs
	cargo fmt -p pericynthion

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
	    bsc5)
	        # Yale Bright Star Catalogue 5th Revised Ed. (Hoffleit & Warren 1991),
	        # CDS V/50. These are the regeneration inputs for the inlined
	        # crates/pericynthion/src/bsc5_catalogue.rs: the catalogue records
	        # (catalog.gz, ~9110 records) and the CDS ReadMe (record format +
	        # provenance). Both land gitignored in the workspace root; after
	        # fetching, re-inline their contents into bsc5_catalogue.rs verbatim.
	        #
	        # The HTTPS host (cdsarc.cds.unistra.fr) now sits behind an anti-bot JS
	        # wall that serves an HTML challenge to CLI downloaders — wget happily
	        # saved that page as catalog.gz. We pull over plain FTP from the
	        # u-strasbg mirror (no bot wall) and verify catalog.gz is real gzip,
	        # failing loudly rather than producing a corrupt regeneration input.
	        if [ -f catalog.gz ] && gzip -t catalog.gz 2>/dev/null; then
	            echo "catalog.gz already present and valid — skipping"
	        else
	            rm -f catalog.gz catalog.gz.aria2
	            aria2c -o catalog.gz --connect-timeout=15 --max-tries=3 --retry-wait=2 \
	                --allow-overwrite=true --auto-file-renaming=false \
	                "ftp://cdsarc.u-strasbg.fr/cats/V/50/catalog.gz"
	            if ! gzip -t catalog.gz 2>/dev/null; then
	                echo "ERROR: catalog.gz is not valid gzip (mirror down/blocked); removing" >&2
	                rm -f catalog.gz
	                exit 1
	            fi
	            echo "catalog.gz fetched + verified ($(gzip -dc catalog.gz | wc -c) bytes uncompressed)"
	        fi
	        rm -f ReadMe ReadMe.aria2
	        aria2c -o ReadMe --connect-timeout=15 --max-tries=3 --retry-wait=2 \
	            --allow-overwrite=true --auto-file-renaming=false \
	            "ftp://cdsarc.u-strasbg.fr/cats/V/50/ReadMe"
	        echo "ReadMe fetched ($(wc -c < ReadMe) bytes)"
	        ;;
	    *)
	        echo "unknown fetch source: {{SOURCE}}" >&2
	        echo "supported sources: de441 adbxml horizons bsc5" >&2
	        exit 1
	        ;;
	esac
