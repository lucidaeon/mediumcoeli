# ─────────────────────────────────────────────────────────────────────────────
# docker — multi-arch image builds, one per scripts/Dockerfile.<crate>
# Loaded as a module from the root justfile: `mod docker 'scripts/docker.just'`,
# so these run as `just docker build`, `just docker push`, `just docker release`.
#
# Auto-discovers Dockerfiles: drop in a scripts/Dockerfile.<crate> and it is
# picked up with zero edits — there is no crate list to maintain. Every recipe
# cd's to the repo root first (a module recipe otherwise runs from this file's
# dir, scripts/), so the build context `.` is exactly your working tree and the
# scripts/Dockerfile.* glob resolves correctly.
#
# Targeting: no CRATE → every discovered image; one CRATE → just that one.
# Native amd64: set DOCKER_AMD64_HOST=tcp://host:2375 to build amd64 on a remote
# node instead of QEMU emulation (faster, avoids OOM on heavy dep graphs).
#
#   just docker build                    → build every image :latest (no push)
#   just docker build 0.1.2              → build every image :0.1.2
#   just docker build 0.1.2 starcat      → build only starcat:0.1.2
#   just docker build-no-cache           → build every image, ignoring the layer cache
#   just docker push 0.1.2               → build + push every image :0.1.2 to all registries
#   just docker push 0.1.2 starcat       → build + push only starcat:0.1.2
#   just docker release "${mc_tags[@]}"  → per-release: push crate/x.y.z, then move :latest by digest
# ─────────────────────────────────────────────────────────────────────────────

registries := 'ghcr.io docker.io'
namespace  := 'lucidaeon'
platforms  := 'linux/amd64,linux/arm64'
tag        := 'latest'

# OPTIMIZATION SWITCH. While the Dockerfiles' layers are still being tuned, every
# build/push/release rebuilds from scratch (--no-cache) so what you get is exactly
# what the Dockerfile says — no stale BuildKit layers masking changes or hiding a
# bad cache split. Flip to 'false' once the layers are dialed in to restore fast,
# cache-reusing builds. (The explicit `build-no-cache` recipe ignores this and is
# always uncached.)
no_cache := 'true'

# Build every discovered image (or one CRATE) for all platforms — no push.
build TAG=tag CRATE='': (_buildx '' TAG CRATE)

# Build every discovered image (or one CRATE), ignoring the BuildKit layer cache.
build-no-cache TAG=tag CRATE='': (_buildx '--no-cache' TAG CRATE)

# Build + push every discovered image (or one CRATE), multi-arch, to all registries.
push TAG=tag CRATE='': (_buildx '--push' TAG CRATE)

# Per-release publish, driven by crate/x.y.z args (the release tag array). For
# each discovered Dockerfile that has a matching version: build all platforms and
# push :x.y.z to every registry in one buildx pass (buildx pushes only after a
# clean multi-arch build, so a failed arch tags nothing), then re-point :latest
# at that same manifest by digest — no rebuild, no layer re-push. A Dockerfile
# with no matching arg is skipped; a crate/x.y.z with no Dockerfile (e.g. the
# jzod library) is simply never discovered.
#
#   just docker release "${mc_tags[@]}"
#   just docker release starcat/0.1.2 blackmoon/0.1.2
#   just docker release starcat/0.1.2
release +TAGS:
	#!/usr/bin/env bash
	set -euo pipefail
	cargo generate-lockfile
	cd "{{justfile_directory()}}"   # repo root: `.` context + Dockerfile glob resolve here
	# Optional native-amd64 remote builder (vs. slow / OOM-prone QEMU emulation).
	builder_args=()
	if [[ -n "${DOCKER_AMD64_HOST:-}" ]]; then
	    # Builder identity derives from the host var, so changing DOCKER_AMD64_HOST
	    # always selects a fresh builder: a stale builder pinned to an old host
	    # never shadows the var, and is never inspected (no hang on a dead node).
	    builder="multiarch-$(printf '%s' "${DOCKER_AMD64_HOST}" | tr -c 'a-zA-Z0-9' '-')"
	    if ! docker buildx inspect "${builder}" &>/dev/null; then
	        docker buildx create --name "${builder}" --driver docker-container \
	            --platform linux/arm64
	        docker buildx create --append --name "${builder}" \
	            --platform linux/amd64 "${DOCKER_AMD64_HOST}"
	    fi
	    builder_args=(--builder "${builder}")
	fi
	# Parse the crate/x.y.z arguments into a crate -> version map.
	declare -A ver=()
	for t in {{TAGS}}; do ver["${t%/*}"]="${t#*/}"; done
	shopt -s nullglob
	found=0
	for dockerfile in scripts/Dockerfile.*; do
	    found=1
	    crate="${dockerfile##*/Dockerfile.}"
	    v="${ver[$crate]:-}"
	    if [[ -z "$v" ]]; then
	        echo "skip $crate: no $crate/x.y.z in args" >&2
	        continue
	    fi
	    # Phase 1 — build both arches and push :$v to every registry in one pass.
	    ver_tags=(); for r in {{registries}}; do ver_tags+=( -t "$r/{{namespace}}/$crate:$v" ); done
	    echo "==> $crate:$v — build {{platforms}} + push to all registries"
	    docker buildx build "${builder_args[@]}" --platform {{platforms}} \
	        "${ver_tags[@]}" --push {{ if no_cache == 'true' { '--no-cache' } else { '' } }} -f "$dockerfile" .
	    # Phase 2 — re-point :latest at that manifest by digest, per registry
	    # (source and target same registry, so no cross-registry blob copy).
	    for r in {{registries}}; do
	        echo "==> $r/{{namespace}}/$crate:latest -> :$v (retag by digest, no rebuild)"
	        docker buildx imagetools create -t "$r/{{namespace}}/$crate:latest" \
	            "$r/{{namespace}}/$crate:$v"
	    done
	done
	[[ "$found" == 1 ]] || { echo "no scripts/Dockerfile.* found (run from repo root)" >&2; exit 1; }

# ── internals ─────────────────────────────────────────────────────────────────

# Shared buildx driver behind build / build-no-cache / push. EXTRA is one of
# '' | '--no-cache' | '--push'. Resolves the image set (one CRATE, or every
# scripts/Dockerfile.*) and builds each for all platforms, tagging :TAG in every
# registry.
[private]
_buildx EXTRA TAG CRATE:
	#!/usr/bin/env bash
	set -euo pipefail
	cd "{{justfile_directory()}}"   # repo root: `.` context + Dockerfile glob resolve here
	# Optional native-amd64 remote builder (vs. slow / OOM-prone QEMU emulation).
	builder_args=()
	if [[ -n "${DOCKER_AMD64_HOST:-}" ]]; then
	    # Builder identity derives from the host var, so changing DOCKER_AMD64_HOST
	    # always selects a fresh builder: a stale builder pinned to an old host
	    # never shadows the var, and is never inspected (no hang on a dead node).
	    builder="multiarch-$(printf '%s' "${DOCKER_AMD64_HOST}" | tr -c 'a-zA-Z0-9' '-')"
	    if ! docker buildx inspect "${builder}" &>/dev/null; then
	        docker buildx create --name "${builder}" --driver docker-container \
	            --platform linux/arm64
	        docker buildx create --append --name "${builder}" \
	            --platform linux/amd64 "${DOCKER_AMD64_HOST}"
	    fi
	    builder_args=(--builder "${builder}")
	fi
	# Resolve the image set: one CRATE, or auto-discover every Dockerfile.
	if [[ -n "{{CRATE}}" ]]; then
	    crates=( "{{CRATE}}" )
	else
	    shopt -s nullglob
	    crates=(); for f in scripts/Dockerfile.*; do crates+=( "${f##*/Dockerfile.}" ); done
	fi
	[[ ${#crates[@]} -gt 0 ]] || { echo "no scripts/Dockerfile.* found (run from repo root)" >&2; exit 1; }
	for crate in "${crates[@]}"; do
	    tag_flags=(); for r in {{registries}}; do tag_flags+=( -t "$r/{{namespace}}/$crate:{{TAG}}" ); done
	    echo "==> $crate:{{TAG}} ({{platforms}}) {{EXTRA}}"
	    docker buildx build "${builder_args[@]}" --platform {{platforms}} \
	        "${tag_flags[@]}" {{EXTRA}} {{ if no_cache == 'true' { '--no-cache' } else { '' } }} -f "scripts/Dockerfile.$crate" .
	done

# Reads instance files directly, so it never hangs on a builder with a dead node.
# Remove stray multiarch-* buildx builders left behind when DOCKER_AMD64_HOST changes.
prune-builders:
	#!/usr/bin/env bash
	set -euo pipefail
	dir="$HOME/.docker/buildx/instances"
	[[ -d "$dir" ]] || { echo "no buildx instances to prune"; exit 0; }
	found=0
	for f in "$dir"/multiarch-*; do
	    [[ -e "$f" ]] || continue
	    found=1; b="$(basename "$f")"
	    echo "removing stale builder $b"
	    timeout 15 docker buildx rm -f "$b" 2>/dev/null || rm -f "$f"
	done
	[[ "$found" == 1 ]] || echo "no multiarch-* builders found"
