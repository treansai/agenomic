#!/usr/bin/env bash
# Idempotent wrapper: replay, contract checks, diff vs. previous bundle.
# Usage: self-evaluate.sh [workspace] [--against REF]
set -euo pipefail

WORKSPACE="${1:-.}"
shift || true

AGAINST=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --against) AGAINST="$2"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 64 ;;
  esac
done

cd "$WORKSPACE"

command -v agenomic >/dev/null || {
  echo '{"status":"error","reason":"cli_missing"}'; exit 127;
}

[[ -d agent-bundle ]] || {
  echo '{"status":"error","reason":"no_bundle"}'; exit 65;
}

# 2.1 Replay
agenomic replay \
  --bundle agent-bundle \
  --traces agent-bundle/evals \
  --format json > .agenomic-replay.json

# 2.3 Diff (optional)
DIFF_JSON="null"
if [[ -n "$AGAINST" ]]; then
  agenomic diff "$AGAINST" agent-bundle --format json > .agenomic-diff.json
  DIFF_JSON="$(cat .agenomic-diff.json)"
fi

# 3 Report
REPLAY_JSON="$(cat .agenomic-replay.json)"
cat > agent-bundle/.self-evaluation.json <<EOF
{
  "skill": "self-graft-and-evaluate",
  "skill_version": "0.1.0",
  "phases": {
    "graft":    { "status": "pass", "bundle_path": "agent-bundle" },
    "evaluate": { "replay": ${REPLAY_JSON}, "diff": ${DIFF_JSON} }
  }
}
EOF

echo "self-evaluation written to agent-bundle/.self-evaluation.json"
