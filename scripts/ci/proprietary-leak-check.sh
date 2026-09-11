#!/usr/bin/env bash
# Proprietary leak detector for the public tree.
#
# The public repositories must never carry Cloud-only code, private package
# names, internal hosts, deployment configuration or credentials. This scan
# is deny-by-default on markers, not an allow-list of files: the allow-list
# is the repository itself (only public components live here). It fails on:
#   - references to private repositories / crates / packages
#   - Cloud-only capability implementations (ids from the capability registry
#     that exist only in the cloud/enterprise editions)
#   - internal hostnames and deployment material
#   - environment files and key material
# Usage: scripts/ci/proprietary-leak-check.sh [path] (default: repo root)
set -euo pipefail

root="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
status=0

# Markers that exist only in the private repositories. Public crates share
# the `agenomic-` prefix (agenomic-cloud-client, agenomic-rmp, agenomic-ledger-local
# are public), so only names with no public counterpart are listed, plus
# private module paths, deployment secrets and internal hosts. Capability
# identifiers are deliberately NOT markers: they are part of the public
# contract (the CLI renders them); what must not leak is their implementation,
# which lives behind the module paths below.
private_markers=(
  'agenomic_private' 'agenomic-governance-agents' 'agenomic_governance_agents'
  'agenomic-billing([^a-z-]|$)' 'agenomic_billing::' 'agenomic-capabilities([^a-z-]|$)' 'agenomic_capabilities::'
  'agenomic-db([^a-z-]|$)' 'agenomic_db::' 'agenomic-tracking([^a-z-]|$)' 'agenomic_tracking::'
  'AGENOMIC__STRIPE__' 'STRIPE_SECRET_KEY' 'SCW_SECRET_KEY' 'SCW_REGISTRY_URL' 'GOVERNANCE_AGENTS_TOKEN'
  'rg\.fr-par\.scw\.cloud' 'scw\.cloud/agenomic'
  'page\.cloud\.tsx' 'handlers_billing' 'handlers_rmp' 'governance_forward' 'organization_capability_overrides'
)
forbidden_files=(
  '.env' '.env.local' '.env.production' '.env.staging' '*.pem' '*.key' '*.p12' '*.pfx' 'id_rsa' 'id_ed25519' 'docker-compose.prod*.yml' 'Pulumi.*.yaml'
)

# Files that legitimately mention the private repositories by name (the
# umbrella README explains the split) are listed here explicitly.
# Prose (Markdown) may name the private repositories; lockfiles list public
# crate names that embed them (agenomic-cloud-client); test fixtures under a
# `fixtures` directory are deliberately fake key material.
allow_paths_re='(^|/)(README\.md|AGENTS\.md|CHANGELOG\.md|[^/]+\.md|Cargo\.lock|docs/architecture/repository-strategy\.md|scripts/ci/proprietary-leak-check\.sh|\.github/CODEOWNERS|\.github/workflows/[^/]+|scripts/github/.*)$|(^|/)fixtures/'

tracked() { git -C "$root" ls-files -z --recurse-submodules 2>/dev/null || git -C "$root" ls-files -z; }

while IFS= read -r -d '' file; do
  [[ "$file" =~ $allow_paths_re ]] && continue
  base="$(basename "$file")"
  for pattern in "${forbidden_files[@]}"; do
    # shellcheck disable=SC2254
    case "$base" in $pattern) echo "leak-check: forbidden file in public tree: $file" >&2; status=1 ;; esac
  done
done < <(tracked)

scan() { # scan <label> <pattern>...
  local label="$1"; shift
  local hits
  hits="$(tracked | xargs -0 grep -IlnE "$(IFS='|'; echo "$*")" 2>/dev/null | grep -vE "$allow_paths_re" || true)"
  if [ -n "$hits" ]; then
    echo "leak-check: $label:" >&2
    printf '  %s\n' "$hits" >&2
    status=1
  fi
}
scan "private repository / crate / deployment marker" "${private_markers[@]}"

if [ "$status" -eq 0 ]; then echo "proprietary-leak-check: ok"; fi
exit "$status"
