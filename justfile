# `just` with no arguments runs the workspace test suite.
default: test

registries := 'ghcr.io docker.io'
namespace  := 'lucidaeon'
tag        := 'latest'
platforms  := 'linux/amd64,linux/arm64'

# Build (workspace, or one crate when CRATE is given).
build CRATE='':
	cargo build {{ if CRATE == '' { '--workspace' } else { '-p ' + CRATE } }}

# Release build (workspace, or one crate when CRATE is given).
release CRATE='':
	cargo build --release {{ if CRATE == '' { '--workspace' } else { '-p ' + CRATE } }}

# Cargo check (workspace, or one crate when CRATE is given).
check CRATE='':
	cargo check {{ if CRATE == '' { '--workspace' } else { '-p ' + CRATE } }}

# Run tests (workspace, or one crate). `--nocapture`; tests self-skip on missing STARCAT_JPL_DATA / ASTRO_SPECIMENS.
test CRATE='':
	cargo test --release {{ if CRATE == '' { '--workspace' } else { '-p ' + CRATE } }} -- --nocapture

# Lint with all targets — lib, bin, tests, examples, benches (workspace, or one crate).
lint CRATE='':
	cargo clippy {{ if CRATE == '' { '--workspace' } else { '-p ' + CRATE } }} --all-targets -- -D warnings

# Narrower lint: lib + bin only, no tests / examples / benches (workspace, or one crate).
lint-narrow CRATE='':
	cargo clippy {{ if CRATE == '' { '--workspace' } else { '-p ' + CRATE } }} -- -D warnings

# Format every Rust file in the workspace.
fmt:
	cargo fmt --all

# Verify the workspace is already correctly formatted.
fmt-check:
	cargo fmt --all -- --check

# Publish to crates.io (dry-run for now). Optionally target one crate: just publish starcat
publish CRATE='':
	#!/usr/bin/env bash
	set -euo pipefail
	crates=( starcat blackmoon )
	[[ -n "{{CRATE}}" ]] && crates=( "{{CRATE}}" )
	for crate in "${crates[@]}"; do
	    echo ">>> cargo publish --dry-run -p $crate"
	    cargo publish --dry-run -p "$crate"
	done

# Build Docker images for all platforms (no output — warms each node's BuildKit
# cache so a subsequent push reuses layers).
# Set DOCKER_AMD64_HOST=tcp://host:2375 to wire a native amd64 remote builder
# instead of QEMU emulation (faster, no OOM on heavy dep graphs).
# just docker build                    → build+tag both crates, tag latest
# just docker build 0.0.1             → build+tag both crates, tag 0.0.1
# just docker build 0.0.1 starcat     → build+tag one crate
# just docker build-no-cache          → same as build but with --no-cache
# just docker push                    → push latest to all registries (multi-arch)
# just docker push 0.0.1             → push 0.0.1 to all registries (multi-arch)
# just docker push 0.0.1 starcat     → push one crate
docker ACTION TAG=tag CRATE='':
	#!/usr/bin/env bash
	set -euo pipefail
	crates=( starcat blackmoon )
	[[ -n "{{CRATE}}" ]] && crates=( "{{CRATE}}" )
	# Wire up a native amd64 remote node when DOCKER_AMD64_HOST is set.
	builder_args=()
	if [[ -n "${DOCKER_AMD64_HOST:-}" ]]; then
	    if ! docker buildx inspect multiarch &>/dev/null; then
	        docker buildx create --name multiarch --driver docker-container \
	            --platform linux/arm64
	        docker buildx create --append --name multiarch \
	            --platform linux/amd64 "${DOCKER_AMD64_HOST}"
	    fi
	    builder_args=(--builder multiarch)
	fi
	for crate in "${crates[@]}"; do
	    case "{{ACTION}}" in
	        build)
	            tag_flags=()
	            for r in {{registries}}; do tag_flags+=( -t "$r/{{namespace}}/$crate:{{TAG}}" ); done
	            docker buildx build \
	                "${builder_args[@]}" \
	                --platform {{platforms}} \
	                "${tag_flags[@]}" \
	                -f "scripts/Dockerfile.$crate" .
	            ;;
	        build-no-cache)
	            tag_flags=()
	            for r in {{registries}}; do tag_flags+=( -t "$r/{{namespace}}/$crate:{{TAG}}" ); done
	            docker buildx build \
	                "${builder_args[@]}" \
	                --platform {{platforms}} \
	                "${tag_flags[@]}" \
	                --no-cache \
	                -f "scripts/Dockerfile.$crate" .
	            ;;
	        push)
	            tag_flags=()
	            for r in {{registries}}; do tag_flags+=( -t "$r/{{namespace}}/$crate:{{TAG}}" ); done
	            docker buildx build \
	                "${builder_args[@]}" \
	                --platform {{platforms}} \
	                "${tag_flags[@]}" \
	                --push \
	                -f "scripts/Dockerfile.$crate" .
	            ;;
	        *)     echo "unknown docker action: {{ACTION}}" >&2; exit 1 ;;
	    esac
	done

# Fetch test corpora by source name: de441 | adbxml | horizons.
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
