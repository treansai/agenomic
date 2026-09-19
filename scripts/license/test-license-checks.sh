#!/usr/bin/env bash
# Tests for the licence checks: each case builds a throwaway repository, plants
# one defect and asserts the corresponding check rejects it. The last case is
# the clean tree, which must pass. Run locally and in CI before the checks
# themselves are trusted to gate a release.
# Usage: scripts/license/test-license-checks.sh
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
failures=0

# fixture: a minimal repository with one OSS package, one cloud row and a
# valid AGPL LICENSE, in $tmp/tree.
fixture() {
  rm -rf "$tmp/tree"
  mkdir -p "$tmp/tree/scripts/license" "$tmp/tree/pkg"
  cp "$here"/lib.sh "$here"/check-*.sh "$tmp/tree/scripts/license/"
  cp "$here/../../LICENSE" "$tmp/tree/LICENSE"
  cp "$tmp/tree/LICENSE" "$tmp/tree/pkg/LICENSE"
  printf '{\n  "name": "pkg",\n  "license": "AGPL-3.0-only"\n}\n' > "$tmp/tree/pkg/package.json"
  printf '# License\n\n## License\n\nAGPL-3.0-only. See [LICENSE](LICENSE).\n' > "$tmp/tree/pkg/README.md"
  printf '# path\tedition\tlicence\tpublished\tlicence_file\npkg/package.json\toss\tAGPL-3.0-only\tnpm\tyes\nagenomic-cloud\tcloud\tproprietary\tno\tno\n' \
    > "$tmp/tree/scripts/license/packages.tsv"
  git -C "$tmp/tree" init -q
  git -C "$tmp/tree" add -A
  git -C "$tmp/tree" -c user.email=t@t -c user.name=t commit -qm init
}

# expect <fail|pass> <check> <description>
expect() {
  local want="$1" check="$2" desc="$3" got=pass
  git -C "$tmp/tree" add -A >/dev/null 2>&1 || true
  "$tmp/tree/scripts/license/$check" >/dev/null 2>&1 || got=fail
  if [ "$got" = "$want" ]; then
    echo "ok   $desc"
  else
    echo "FAIL $desc (expected $want, got $got)" >&2
    failures=$((failures + 1))
  fi
}

fixture; rm "$tmp/tree/LICENSE"
expect fail check-license-metadata.sh "root LICENSE missing is rejected"

fixture; printf 'MIT License\n\nCopyright (c) 2026\n' > "$tmp/tree/LICENSE"
expect fail check-license-metadata.sh "a LICENSE that is not the AGPL text is rejected"

fixture; sed -i.bak 's/AGPL-3.0-only/MIT/' "$tmp/tree/pkg/package.json"
expect fail check-license-metadata.sh "an OSS package declaring a stale licence is rejected"

fixture; rm "$tmp/tree/pkg/LICENSE"
expect fail check-license-metadata.sh "a package with no LICENSE file is rejected"

fixture
printf '[project]\nname = "p2"\nlicense = { text = "AGPL-3.0-only" }\nclassifiers = [\n    "License :: OSI Approved :: Apache Software License",\n]\n' > "$tmp/tree/pkg/pyproject.toml"
printf 'pkg/pyproject.toml\toss\tAGPL-3.0-only\tpypi\tyes\n' >> "$tmp/tree/scripts/license/packages.tsv"
expect fail check-license-metadata.sh "a trove classifier contradicting the licence is rejected"

fixture; mkdir -p "$tmp/tree/other"; printf '{\n  "name": "other",\n  "license": "MIT"\n}\n' > "$tmp/tree/other/package.json"
expect fail check-public-packages.sh "an unclassified package is rejected"

fixture; mkdir -p "$tmp/tree/agenomic-cloud"; printf 'x\n' > "$tmp/tree/agenomic-cloud/main.rs"
expect fail check-public-packages.sh "a cloud component in the public tree is rejected"

fixture; mkdir -p "$tmp/tree/crate"
printf '[package]\nname = "c"\nlicense.workspace = true\n' > "$tmp/tree/crate/Cargo.toml"
expect fail check-public-packages.sh "a published crate with no LICENSE file is rejected"

fixture; mkdir -p "$tmp/tree/crate"
printf '[package]\nname = "c"\nlicense.workspace = true\npublish = false\n' > "$tmp/tree/crate/Cargo.toml"
expect pass check-public-packages.sh "an unpublished crate needs no LICENSE file"

fixture
printf '# p\n\n[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)\n' > "$tmp/tree/pkg/README.md"
expect fail check-stale-license-references.sh "a stale licence badge is rejected"

fixture; printf '# p\n\n## License\n\nApache-2.0. See [LICENSE](LICENSE).\n' > "$tmp/tree/pkg/README.md"
expect fail check-stale-license-references.sh "a stale License section is rejected"

fixture; printf '# deps\n\n| dep | MIT |\n' > "$tmp/tree/THIRD_PARTY_LICENSES.md"
expect pass check-stale-license-references.sh "a third-party report naming MIT is accepted"

fixture
expect pass check-license-metadata.sh "the clean tree passes the metadata check"
expect pass check-public-packages.sh "the clean tree passes the boundary check"
expect pass check-stale-license-references.sh "the clean tree passes the stale-reference check"

if [ "$failures" -gt 0 ]; then
  echo "test-license-checks: $failures failure(s)" >&2
  exit 1
fi
echo "test-license-checks: ok"
