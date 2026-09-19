#!/usr/bin/env bash
# Every licence check, in one command. This is what CI calls and what a
# contributor runs before opening a pull request.
# Usage: scripts/license/check-all.sh
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
"$here/check-license-metadata.sh"
"$here/check-public-packages.sh"
"$here/check-stale-license-references.sh"
echo "license: ok"
