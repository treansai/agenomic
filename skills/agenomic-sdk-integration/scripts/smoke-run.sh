#!/usr/bin/env bash
# Phase 6: exercise the integration end to end and prove the failure paths.
#
# Requires the target repo to provide ONE traced entrypoint via
# AGENOMIC_SMOKE_CMD. There is no way to guess it; asking is correct.
#
#   AGENOMIC_SMOKE_CMD="python -m myapp.smoke" \
#   AGENOMIC_SMOKE_ENDPOINT_VARS="MYAPP_AGENOMIC_URL" \
#     scripts/smoke-run.sh /path/to/repo
#
# AGENOMIC_SMOKE_ENDPOINT_VARS names the project's own variables that carry
# Agenomic URLs (space-separated). The SDKs read no AGENOMIC_* variable, so
# without it the "Agenomic unreachable" check cannot be exercised.
set -euo pipefail

[ -d "${1:-.}" ] || { echo "error: no such directory: ${1:-.}" >&2; exit 2; }
WORKSPACE="$(cd "${1:-.}" && pwd)"
TRACES="${2:-${WORKSPACE}/.agenomic/traces.jsonl}"
case "$TRACES" in /*) ;; *) TRACES="$(pwd)/${TRACES}" ;; esac
CMD="${AGENOMIC_SMOKE_CMD:-}"
ENDPOINT_VARS="${AGENOMIC_SMOKE_ENDPOINT_VARS:-}"

if [ -z "$CMD" ]; then
  cat >&2 <<'EOF'
error: AGENOMIC_SMOKE_CMD is not set.

Set it to a command that performs exactly one traced agent run, e.g.
  AGENOMIC_SMOKE_CMD="python -m myapp.smoke"
  AGENOMIC_SMOKE_CMD="pnpm tsx src/smoke.ts"
  AGENOMIC_SMOKE_CMD="cargo run --example smoke"

Do not guess this. Ask the user, and record the answer in the report.
EOF
  exit 2
fi

cd "$WORKSPACE"
BEFORE=0
[ -f "$TRACES" ] && BEFORE=$(wc -l <"$TRACES" | tr -d ' ')

step() { printf '\n=== %s ===\n' "$1"; }
lines() { if [ -f "$TRACES" ]; then wc -l <"$TRACES" | tr -d ' '; else echo 0; fi; }
count_lines() { { grep -E "$1" "$TRACES" 2>/dev/null || true; } | wc -l | tr -d ' '; }

# --- 1. Normal run -----------------------------------------------------------
step "normal run"
if ! sh -c "$CMD"; then
  echo "error: smoke command failed on the happy path; fix before validating" >&2
  exit 1
fi
AFTER=$(lines)
DELTA=$((AFTER - BEFORE))
echo "traces produced: ${DELTA}"
if [ "$DELTA" -eq 0 ]; then
  echo "error: the run produced no trace; the run boundary is not wired" >&2
  echo "       check that the decorator sits on the code path the command exercises," >&2
  echo "       and (TypeScript) that the smoke command appends emitted envelopes to ${TRACES}" >&2
  exit 1
fi

# --- 2. Agenomic unreachable -------------------------------------------------
# The agent MUST still complete. This is the non-negotiable resilience check.
step "agenomic unreachable"
UNREACHABLE=not_exercised
if [ -z "$ENDPOINT_VARS" ]; then
  echo "not exercised: AGENOMIC_SMOKE_ENDPOINT_VARS is not set."
  echo "  Name the project's own Agenomic URL variables (e.g. MYAPP_AGENOMIC_URL) and re-run."
  echo "  If the agent never talks to Agenomic Cloud, record 'local only' in the report instead."
else
  set --
  for var in $ENDPOINT_VARS; do set -- "$@" "${var}=http://127.0.0.1:1"; done
  BEFORE_UNREACH=$(lines)
  if env "$@" AGENOMIC_ENDPOINT="http://127.0.0.1:1" sh -c "$CMD"; then
    UNREACHABLE=pass
    echo "pass: agent completed with Agenomic unreachable (${ENDPOINT_VARS})"
  else
    echo "FAIL: the agent did not complete when Agenomic was unreachable." >&2
    echo "      Telemetry must never break the hot path: see SKILL.md, Phase 4," >&2
    echo "      and references/capability-matrix.md section 8." >&2
    exit 1
  fi
  echo "traces produced while unreachable: $(( $(lines) - BEFORE_UNREACH ))"
fi

# --- 3. Trace content --------------------------------------------------------
# Python / CLI-shaped envelopes carry model_calls / tool_calls arrays;
# TypeScript envelopes carry events[] with type model_call / tool_call.
step "trace content"
HAS_MODEL=$(( $(count_lines '"model_calls"[[:space:]]*:[[:space:]]*\[[^]]') + $(count_lines '"type"[[:space:]]*:[[:space:]]*"model_call"') ))
HAS_TOOL=$(( $(count_lines '"tool_calls"[[:space:]]*:[[:space:]]*\[[^]]') + $(count_lines '"type"[[:space:]]*:[[:space:]]*"tool_call"') ))
echo "traces with model calls: ${HAS_MODEL}"
echo "traces with tool calls:  ${HAS_TOOL}"

[ "$HAS_MODEL" -gt 0 ] || echo "warn: no model calls recorded; provider instrumentation missed" >&2
[ "$HAS_TOOL" -gt 0 ]  || echo "warn: no tool calls recorded; tool boundary missed, instrument_langgraph relied on, or a thread dropped the context" >&2

# Advisory: the CLI cannot read TypeScript envelopes, and reads Python ones
# only after renaming tool -> name (references/capability-matrix.md section 2).
if command -v agm >/dev/null 2>&1; then
  if grep -q '"specVersion"' "$TRACES" 2>/dev/null; then
    echo "agm trace validate: skipped (TypeScript trace shape is not readable by the CLI today)"
  elif command -v jq >/dev/null 2>&1; then
    CONVERTED="$(dirname "$TRACES")/traces.cli.jsonl"
    jq -c '. + {output: (.output // .final_output)}
           | .tool_calls |= ((. // []) | map(. + {name: (.name // .tool),
               human_approval_present: (.human_approval_present // .approval_present)}))' \
      "$TRACES" >"$CONVERTED"
    agm trace validate "$CONVERTED" || echo "warn: converted traces do not validate against the CLI trace shape" >&2
  else
    agm trace validate "$TRACES" || echo "warn: traces do not validate (install jq to convert Python traces first)" >&2
  fi
fi

# --- 4. Project test suite ---------------------------------------------------
step "project tests"
if   [ -f pyproject.toml ] && command -v pytest >/dev/null 2>&1; then pytest -q || echo "warn: project tests failed" >&2
elif [ -f package.json ]   && grep -q '"test"' package.json;      then npm test --silent || echo "warn: project tests failed" >&2
elif [ -f Cargo.toml ];                                           then cargo test --quiet || echo "warn: project tests failed" >&2
else echo "no recognised test suite; report this rather than inventing one"
fi

step "summary"
echo "total traces: $(lines)"
echo "agenomic unreachable: ${UNREACHABLE}"
echo "next: scripts/validate-integration.sh \"$WORKSPACE\""
echo
echo "Still to exercise manually (they need failure injection the harness cannot fake):"
echo "  - model failure   → expect status=error on the model call, run still recorded"
echo "  - tool failure    → expect status=error on the tool call"
echo "  - loop            → expect agm track to raise loop.detected"
echo "  - policy blocked  → expect agm gate check exit 16"
echo "  - queue recovery  → agm ledger queue status / flush after a kill -9"
