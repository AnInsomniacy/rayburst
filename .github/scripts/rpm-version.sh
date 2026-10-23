#!/usr/bin/env bash
# Prints the RPM version and release for an app version, separated by "-".
#
# RPM orders 4.1.0-beta.1 after 4.1.0, so a beta would never update to its
# release (#766), and rpm 0.30 (Tauri CLI 2.12.3+) rejects "-" in a version
# outright. A prerelease keeps the release version and moves its label into
# the Release field, as Fedora packaged prereleases before tilde versions:
#   4.1.0          -> 4.1.0-1
#   4.1.0-beta.11  -> 4.1.0-0.beta.11   (sorts before 4.1.0-1)
set -euo pipefail

if [ $# -ne 1 ] || ! [[ "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
  echo "Usage: $0 <semver>" >&2
  exit 1
fi
VERSION="$1"
if [[ "$VERSION" == *-* ]]; then
  PRERELEASE="${VERSION#*-}"
  echo "${VERSION%%-*}-0.${PRERELEASE//-/.}"
else
  echo "${VERSION}-1"
fi
