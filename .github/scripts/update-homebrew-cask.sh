#!/usr/bin/env bash
# Rewrites the version and both architecture checksums of the Homebrew cask in place.
# Usage: update-homebrew-cask.sh <cask-file> <version> <arm-sha256> <intel-sha256>
set -euo pipefail

if [ "$#" -ne 4 ]; then
  echo "usage: $0 <cask-file> <version> <arm-sha256> <intel-sha256>" >&2
  exit 2
fi

cask="$1"
version="$2"
arm="$3"
intel="$4"

[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "invalid version: $version" >&2; exit 1; }
for sum in "$arm" "$intel"; do
  [[ "$sum" =~ ^[0-9a-f]{64}$ ]] || { echo "invalid sha256: $sum" >&2; exit 1; }
done

expect_one() {
  local pattern="$1" file="$2" count
  count="$(grep -cE "$pattern" "$file" || true)"
  if [ "$count" -ne 1 ]; then
    echo "expected exactly one line matching '$pattern' in $file, found $count" >&2
    exit 1
  fi
}

# The cask layout this script edits; fail loudly if the tap changes shape.
expect_one '^  version "[^"]*"$' "$cask"
expect_one '^  sha256 arm: +"[0-9a-f]{64}",$' "$cask"
expect_one '^ +intel: "[0-9a-f]{64}"$' "$cask"

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
sed -E \
  -e "s/^(  version \")[^\"]*(\")$/\\1${version}\\2/" \
  -e "s/^(  sha256 arm: +\")[0-9a-f]{64}(\",)$/\\1${arm}\\2/" \
  -e "s/^( +intel: \")[0-9a-f]{64}(\")$/\\1${intel}\\2/" \
  "$cask" > "$tmp"

expect_one "^  version \"${version}\"$" "$tmp"
expect_one "^  sha256 arm: +\"${arm}\",$" "$tmp"
expect_one "^ +intel: \"${intel}\"$" "$tmp"

cat "$tmp" > "$cask"
