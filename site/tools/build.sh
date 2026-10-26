#!/usr/bin/env bash
# Parity build for the engram GitHub Pages site (plan DEC-P01).
# Builds site/ through the exact pinned container the pages.yml workflow uses,
# with Actions-shaped environment, so local evidence equals CI evidence.
# Usage: bash site/tools/build.sh   (from the repository root)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
IMAGE="ghcr.io/actions/jekyll-build-pages:v1.0.13"
SRC_REL="site"
DEST_REL="site/_site"
DEST="$REPO_ROOT/$DEST_REL"

command -v docker >/dev/null 2>&1 || { echo "build.sh: docker is required" >&2; exit 1; }

rm -rf "$DEST"

# Env mirrors .github/workflows/pages.yml: the container's entrypoint reads
# GITHUB_WORKSPACE plus the action's INPUT_* variables (see its entrypoint.sh).
docker run --rm \
  -e GITHUB_WORKSPACE=/workspace \
  -e INPUT_SOURCE="$SRC_REL" \
  -e INPUT_DESTINATION="$DEST_REL" \
  -e INPUT_TOKEN= \
  -e INPUT_BUILD_REVISION="$(git -C "$REPO_ROOT" rev-parse HEAD)" \
  -e INPUT_VERBOSE=false \
  -e INPUT_FUTURE=false \
  -e GITHUB_REPOSITORY=phanijapps/engram \
  -e GITHUB_API_URL=https://api.github.com \
  -e HOME=/tmp \
  -u "$(id -u):$(id -g)" \
  -v "$REPO_ROOT":/workspace \
  -w /workspace \
  "$IMAGE"

test -f "$DEST/index.html" || { echo "build.sh: $DEST/index.html missing after build" >&2; exit 1; }
echo "build.sh: parity build OK -> $DEST_REL"
