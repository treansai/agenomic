#!/usr/bin/env bash
# Publish the 4 npm placeholders.
#
# Auth (one of):
#   npm adduser     (interactive, writes ~/.npmrc)
#   or:             export NPM_TOKEN=npm_…
#                   and add the following to ~/.npmrc:
#                     //registry.npmjs.org/:_authToken=${NPM_TOKEN}
#                   (or use `npm config set` per-machine)
#
# Usage:
#   scripts/placeholders/publish-npm.sh            # dry-run (npm pack only)
#   scripts/placeholders/publish-npm.sh --apply    # publish

set -euo pipefail

apply=0
[[ "${1:-}" == "--apply" ]] && apply=1

here="$(cd "$(dirname "$0")" && pwd)"
cd "$here/npm"

command -v npm >/dev/null || { echo "npm not found"; exit 1; }
if [[ $apply -eq 1 ]]; then
  npm whoami >/dev/null 2>&1 || { echo "npm not authenticated. Run \`npm adduser\` first."; exit 1; }
  echo "Publishing as: $(npm whoami)"
else
  echo "(dry-run; auth not required)"
fi

for pkg in agenomic agenomic-typescript agenomic-sdk agenomic-cli; do
  echo "=== $pkg ==="
  (
    cd "$pkg"
    if [[ $apply -eq 1 ]]; then
      npm publish --access public
    else
      npm pack --dry-run
    fi
  )
done

echo
echo "Done. After upload:"
echo "  npm install agenomic agenomic-typescript agenomic-sdk agenomic-cli"
