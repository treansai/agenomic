#!/usr/bin/env bash
# rename-agentlock-to-agenomic.sh
#
# Case-preserving rename of "agentlock" → "agenomic" across one repo.
# Used for the umbrella-wide rename driven from .scratch/agentlock-rename-plan.md.
#
# Defaults locked in:
#   D1 — cryptographic domain-separation tags (AGENTLOCK-LEAF-v, AGENTLOCK-NODE-v,
#        AGENTLOCK-FP-v, AGENTLOCK-ATTESTATION-v) are PRESERVED.
#   D2 — `agent.lock.yaml` and `agent-lock.schema.json` filenames are PRESERVED.
#   D3 — env-var and cookie renames happen here; dual-read shims are added by
#        hand in cloud/web (see .scratch/agentlock-rename-pr-templates.md).
#   D4 — git remote URLs are NOT rewritten by default. Use --remotes after the
#        GitHub repos have been renamed.
#   D5 — Pulumi project/stack name `agentlock-staging` and any pg role/db named
#        `agentlock` are PRESERVED via the carve-out list below.
#
# Usage:
#   scripts/rename-agentlock-to-agenomic.sh                      # dry-run
#   scripts/rename-agentlock-to-agenomic.sh --apply              # write changes
#   scripts/rename-agentlock-to-agenomic.sh --apply --repo PATH  # operate on another repo
#   scripts/rename-agentlock-to-agenomic.sh --apply --remotes    # also rewrite git remote URL
#   scripts/rename-agentlock-to-agenomic.sh --apply --no-dirs    # skip directory renames
#
# Extra preserve strings:
#   PRESERVE_EXTRA="foo,bar baz" scripts/rename-agentlock-to-agenomic.sh --apply

set -euo pipefail

apply=0
repo="."
do_remotes=0
do_dirs=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --apply)    apply=1; shift;;
    --repo)     repo="$2"; shift 2;;
    --remotes)  do_remotes=1; shift;;
    --no-dirs)  do_dirs=0; shift;;
    -h|--help)  sed -n '2,30p' "$0"; exit 2;;
    *) echo "unknown arg: $1" >&2; exit 2;;
  esac
done

cd "$repo"
echo "==> repo: $(pwd)"
echo "==> mode: $([[ $apply -eq 1 ]] && echo APPLY || echo DRY-RUN)"

skip_paths=(
  './.git'
  './.git/*'
  '*/.git/*'
  '*/.git'
  './node_modules/*'
  '*/node_modules/*'
  './target/*'
  '*/target/*'
  './dist/*'
  '*/dist/*'
  './.next/*'
  '*/.next/*'
  './build/*'
  '*/build/*'
  './__pycache__/*'
  '*/__pycache__/*'
  './.venv/*'
  '*/.venv/*'
  './Cargo.lock'
  '*/Cargo.lock'
  './.claude/worktrees/*'
  './scripts/rename-agentlock-to-agenomic.sh'
  './.scratch/*'
  './agentlock-cli/*' './agentlock-cloud/*' './agentlock-examples/*'
  './agentlock-infra/*' './agentlock-python/*' './agentlock-spec/*'
  './agentlock-typescript/*' './agentlock-web/*'
  './agenomic-cli/*' './agenomic-cloud/*' './agenomic-examples/*'
  './agenomic-infra/*' './agenomic-python/*' './agenomic-spec/*'
  './agenomic-typescript/*' './agenomic-web/*'
)

preserve=(
  'AGENTLOCK-LEAF-v'
  'AGENTLOCK-NODE-v'
  'AGENTLOCK-FP-v'
  'AGENTLOCK-ATTESTATION-v'
  'agent.lock.yaml'
  'agent-lock.schema.json'
  'agentlock-staging'
  'agentlock-bringup'
  'agentlock-bundles'
  'agentlock-bundles-app'
  'agentlock-bundles-key'
  'agentlock-bundles-policy'
  'agentlock-db'
  'agentlock-db-grant'
  'agentlock-host'
  'agentlock-ip'
  'agentlock-pg'
  'agentlock-dev'
  'agentlock-local'
)

if [[ -n "${PRESERVE_EXTRA:-}" ]]; then
  IFS=',' read -ra extras <<< "$PRESERVE_EXTRA"
  for e in "${extras[@]}"; do
    e_trimmed="$(echo "$e" | awk '{$1=$1};1')"
    [[ -n "$e_trimmed" ]] && preserve+=("$e_trimmed")
  done
fi

rename_string() {
  echo "$1" | LC_ALL=C sed \
    -e 's/AGENTLOCK/AGENOMIC/g' \
    -e 's/AgentLock/Agenomic/g' \
    -e 's/Agentlock/Agenomic/g' \
    -e 's/agent_lock/agenomic/g' \
    -e 's/agent-lock/agenomic/g' \
    -e 's/agentlock/agenomic/g'
}

sed_inplace() {
  if sed --version >/dev/null 2>&1; then
    LC_ALL=C sed -i "$@"
  else
    LC_ALL=C sed -i '' "$@"
  fi
}

find_args=(. -type f)
for p in "${skip_paths[@]}"; do find_args+=( -not -path "$p" ); done

mapfile -t targets < <(
  find "${find_args[@]}" -print0 \
    | xargs -0 grep -l -i -I 'agentlock' 2>/dev/null \
    | sort -u || true
)

echo "==> files with content hits: ${#targets[@]}"

if [[ $apply -eq 0 ]]; then
  printf '   %s\n' "${targets[@]}"
fi

if [[ $apply -eq 1 ]]; then
  for f in "${targets[@]}"; do
    i=0
    for p in "${preserve[@]}"; do
      sed_inplace "s|${p}|__AGNX_KEEP_${i}__|g" "$f"
      i=$((i+1))
    done

    sed_inplace \
      -e 's/AGENTLOCK/AGENOMIC/g' \
      -e 's/AgentLock/Agenomic/g' \
      -e 's/Agentlock/Agenomic/g' \
      -e 's/agent_lock/agenomic/g' \
      -e 's/agent-lock/agenomic/g' \
      -e 's/agentlock/agenomic/g' \
      "$f"

    i=0
    for p in "${preserve[@]}"; do
      sed_inplace "s|__AGNX_KEEP_${i}__|${p}|g" "$f"
      i=$((i+1))
    done
  done
  echo "==> content rewrite done"
fi

mapfile -t fname_targets < <(git ls-files | grep -i 'agentlock' || true)

filtered=()
for f in "${fname_targets[@]}"; do
  base=$(basename "$f")
  skip=0
  for p in "${preserve[@]}"; do
    if [[ "$base" == *"$p"* ]]; then skip=1; break; fi
  done
  [[ $skip -eq 0 ]] && filtered+=("$f")
done

echo "==> file renames pending: ${#filtered[@]}"

for f in "${filtered[@]}"; do
  newf=$(rename_string "$f")
  [[ "$f" == "$newf" ]] && continue
  if [[ $apply -eq 1 ]]; then
    mkdir -p "$(dirname "$newf")"
    git mv "$f" "$newf"
  else
    echo "   $f → $newf"
  fi
done

if [[ $do_dirs -eq 1 ]]; then
  mapfile -t dir_targets < <(
    find . -type d -name '*agentlock*' \
      -not -path './.git/*' \
      -not -path '*/.git/*' \
      -not -path './.claude/worktrees/*' \
      -not -path '*/node_modules/*' \
      -not -path '*/target/*' \
    | awk '{ print length, $0 }' | sort -rn | cut -d' ' -f2-
  )

  echo "==> directory renames pending: ${#dir_targets[@]}"

  for d in "${dir_targets[@]}"; do
    newd=$(rename_string "$d")
    [[ "$d" == "$newd" ]] && continue
    if [[ $apply -eq 1 ]]; then
      if git ls-files --error-unmatch "$d" >/dev/null 2>&1 \
        || [[ -n "$(git ls-files "$d" 2>/dev/null)" ]]; then
        git mv "$d" "$newd"
      else
        mv "$d" "$newd"
      fi
    else
      echo "   $d → $newd"
    fi
  done
fi

if [[ $do_remotes -eq 1 ]]; then
  if git remote get-url origin >/dev/null 2>&1; then
    cur=$(git remote get-url origin)
    new=$(rename_string "$cur")
    if [[ "$cur" != "$new" ]]; then
      echo "==> remote: $cur → $new"
      [[ $apply -eq 1 ]] && git remote set-url origin "$new"
    fi
  fi
fi

mapfile -t insta_files < <(
  git ls-files '*/snapshots/agenomic_*__*.snap' \
              '*/snapshots/agentlock_*__*.snap' 2>/dev/null || true
)
if [[ ${#insta_files[@]} -gt 0 ]]; then
  echo "==> stale insta snapshots to delete: ${#insta_files[@]}"
  for f in "${insta_files[@]}"; do
    [[ $apply -eq 1 ]] && git rm -f "$f" || echo "   delete: $f"
  done
fi

echo
echo "==> done."
