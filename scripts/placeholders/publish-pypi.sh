#!/usr/bin/env bash
# Publish the 4 PyPI placeholders.
#
# Auth (one of):
#   ~/.pypirc with [pypi] username = __token__ password = pypi-…
#   or:     export TWINE_USERNAME=__token__ TWINE_PASSWORD=pypi-…
#   or:     uv publish supports the same env vars.
#
# Usage:
#   scripts/placeholders/publish-pypi.sh            # dry-run (build only)
#   scripts/placeholders/publish-pypi.sh --apply    # build + upload

set -euo pipefail

apply=0
[[ "${1:-}" == "--apply" ]] && apply=1

here="$(cd "$(dirname "$0")" && pwd)"
cd "$here/pypi"

command -v python3 >/dev/null || { echo "python3 not found"; exit 1; }
venv="$here/.publish-venv"
if [[ ! -x "$venv/bin/python" ]]; then
  echo "Creating publish venv at $venv"
  python3 -m venv "$venv"
fi
"$venv/bin/pip" install --quiet --upgrade build twine
py="$venv/bin/python"

for pkg in agenomic agenomic-python agenomic-sdk agenomic-cli; do
  echo "=== $pkg ==="
  (
    cd "$pkg"
    rm -rf dist
    "$py" -m build --quiet
    ls dist/
    if [[ $apply -eq 1 ]]; then
      "$py" -m twine upload --non-interactive dist/*
    else
      echo "(dry-run; pass --apply to upload)"
    fi
  )
done

echo
echo "Done. After upload:"
echo "  pip install agenomic agenomic-python agenomic-sdk agenomic-cli"
