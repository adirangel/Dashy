#!/usr/bin/env bash
# Copies the bundles a tauri-action build produced into a clean payload
# directory beside a SHA-256 checksum each, and refuses anything unexpected.
#
# Runs on the macOS and Linux build runners. macOS ships bash 3.2 and no
# sha256sum, so this script stays on bash 3.2 features and falls back to
# shasum; both tools print the same "<hash>  <name>" line that
# `sha256sum --check` verifies later.
#
# Inputs (environment):
#   ARTIFACT_PATHS    JSON array of paths from tauri-action's artifactPaths output
#   RELEASE_VERSION   MAJOR.MINOR.PATCH every file name must contain
#   EXPECTED_PATTERN  extended regex every bundle file name must match
#   EXPECTED_COUNT    exact number of bundles expected
#   RUNNER_TEMP       where the payload directory is created
set -euo pipefail

: "${ARTIFACT_PATHS:?}" "${RELEASE_VERSION:?}" "${EXPECTED_PATTERN:?}" "${EXPECTED_COUNT:?}" "${RUNNER_TEMP:?}"

sha256_line() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1"
  else
    shasum -a 256 "$1"
  fi
}

release_directory="$RUNNER_TEMP/dashy-release-assets"
if [[ -e "$release_directory" ]]; then
  echo "The release payload directory already exists." >&2
  exit 1
fi
mkdir -p "$release_directory"

staged=0
while IFS= read -r artifact; do
  [[ -n "$artifact" ]] || continue
  name="$(basename "$artifact")"
  # tauri-action lists the .app bundle directory next to the DMG; only files ship.
  [[ -f "$artifact" ]] || continue
  if [[ ! "$name" =~ $EXPECTED_PATTERN ]]; then
    echo "Unexpected bundle name: $name" >&2
    exit 1
  fi
  if [[ "$name" != *"$RELEASE_VERSION"* ]]; then
    echo "Bundle $name does not carry version $RELEASE_VERSION." >&2
    exit 1
  fi
  cp "$artifact" "$release_directory/$name"
  (cd "$release_directory" && sha256_line "$name" > "$name.sha256")
  staged=$((staged + 1))
done < <(jq -r '.[]' <<<"$ARTIFACT_PATHS")

if [[ "$staged" -ne "$EXPECTED_COUNT" ]]; then
  echo "Expected $EXPECTED_COUNT bundles, staged $staged." >&2
  exit 1
fi

ls -l "$release_directory"
