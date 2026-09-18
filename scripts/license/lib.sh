#!/usr/bin/env bash
# Shared helpers for the licence checks. Sourced, never executed.

license_root() { cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd; }

# rows <root>: the packages.tsv rows, comments and blank lines removed.
rows() { grep -vE '^[[:space:]]*(#|$)' "$1/scripts/license/packages.tsv"; }

# declared_license <manifest>: the licence a manifest declares, or the empty
# string. Handles package.json, pyproject.toml (PEP 639 string and table
# forms) and Cargo.toml (`[package]` and `[workspace.package]`).
declared_license() {
  local f="$1"
  case "$(basename "$f")" in
    package.json)
      sed -nE 's/^[[:space:]]*"license"[[:space:]]*:[[:space:]]*"([^"]*)".*/\1/p' "$f" | awk 'NR == 1' ;;
    pyproject.toml|Cargo.toml)
      sed -nE 's/^license[[:space:]]*=[[:space:]]*\{[[:space:]]*text[[:space:]]*=[[:space:]]*"([^"]*)".*/\1/p; s/^license[[:space:]]*=[[:space:]]*"([^"]*)".*/\1/p' "$f" | awk 'NR == 1' ;;
  esac
}

# classifier_for <spdx>: the PyPI trove classifier expected for an SPDX id.
classifier_for() {
  case "$1" in
    AGPL-3.0-only) echo "GNU Affero General Public License v3" ;;
    Apache-2.0) echo "Apache Software License" ;;
    MIT) echo "MIT License" ;;
    *) echo "" ;;
  esac
}

# is_agpl_text <file>: the file is the GNU AGPL v3 text, not a paraphrase.
is_agpl_text() {
  local opening
  opening="$(head -3 "$1")"
  case "$opening" in *"GNU AFFERO GENERAL PUBLIC LICENSE"*) ;; *) return 1 ;; esac
  case "$opening" in *"Version 3, 19 November 2007"*) ;; *) return 1 ;; esac
  grep -q "13. Remote Network Interaction" "$1"
}
