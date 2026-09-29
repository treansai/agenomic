#!/usr/bin/env bash
# Phase 5: measure integration coverage. Does not assert it.
#
# A capability is "ok" only when a call site exists AND a produced trace
# contains the corresponding field. Wired-but-never-exercised is "partial".
set -euo pipefail

[ -d "${1:-.}" ] || { echo "error: no such directory: ${1:-.}" >&2; exit 2; }
WORKSPACE="$(cd "${1:-.}" && pwd)"
TRACES="${2:-${WORKSPACE}/.agenomic/traces.jsonl}"
case "$TRACES" in /*) ;; *) TRACES="$(pwd)/${TRACES}" ;; esac
OUT_DIR="${WORKSPACE}/.agenomic"
OUT="${OUT_DIR}/coverage.json"

command -v agm >/dev/null 2>&1 || {
  echo "error: 'agm' not found on PATH" >&2; exit 127; }
mkdir -p "$OUT_DIR"

# `grep` exits 1 on no-match. Under `set -euo pipefail` that fails the whole
# pipeline and aborts the script mid-report, so each grep is neutralised with
# `|| true` INSIDE the pipeline, not after it, where `wc` has already run.
# Likewise never use `grep -c`: it prints 0 *and* exits 1, so a trailing
# `|| echo 0` emits "0\n0" and corrupts the JSON.
src_grep() {
  { grep -rEl "$1" "$WORKSPACE" \
      --include='*.py' --include='*.ts' --include='*.tsx' --include='*.js' --include='*.rs' \
      --exclude-dir=node_modules --exclude-dir=.git --exclude-dir=target \
      --exclude-dir=.venv --exclude-dir=venv --exclude-dir=dist \
      --exclude-dir=__pycache__ --exclude-dir=.agenomic \
      2>/dev/null || true; } | wc -l | tr -d ' '
}

# How many trace lines carry a non-empty array/object for this key (Python
# and CLI-shaped envelopes)?
trace_has() {
  [ -f "$TRACES" ] || { echo 0; return; }
  { grep -E "\"$1\"[[:space:]]*:[[:space:]]*[[{][^]}]" "$TRACES" 2>/dev/null || true; } \
    | wc -l | tr -d ' '
}

# How many events of a given `type` appear across the traces (TS envelopes)?
event_count() {
  [ -f "$TRACES" ] || { echo 0; return; }
  { grep -oE "\"type\"[[:space:]]*:[[:space:]]*\"$1\"" "$TRACES" 2>/dev/null || true; } \
    | wc -l | tr -d ' '
}

# ok | partial | missing: wired and exercised / wired only / neither.
verdict() {
  local sites="$1" seen="$2"
  if   [ "$sites" -gt 0 ] && [ "$seen" -gt 0 ]; then echo ok
  elif [ "$sites" -gt 0 ];                     then echo partial
  else                                              echo missing
  fi
}

# sites_only: instrumented, but this script cannot observe it in a trace.
# Distinct from `partial` so an unmeasurable capability is never mistaken for
# a wired-but-unexercised one. Under-reporting damages the matrix as much as
# over-claiming does.
verdict_unmeasurable() {
  [ "$1" -gt 0 ] && echo sites_only || echo missing
}

# --- Call sites --------------------------------------------------------------
RUN_SITES=$(src_grep 'trace_agent_run|traceAgentRun|createTrace|withTracedRoute')
MODEL_SITES=$(src_grep 'instrument_openai|instrument_anthropic|instrument_huggingface|trace_huggingface_call|instrumentOpenAI|instrumentHuggingFace|record_model_call|addModelCall')
TOOL_SITES=$(src_grep 'record_tool_call|addToolCall|trace_mcp_call|recordMCPToolCall|traced_tool')
MEMORY_SITES=$(src_grep 'addMemoryAccess|memory_write|memoryWrite|memory\.read|memory\.write|log_memory')
POLICY_SITES=$(src_grep 'addPolicyCheck|log_policy_check|policy\.evaluated')
REDACT_SITES=$(src_grep 'RedactionEngine|applyRedaction|redact:|capture_input=False')
ATEP_SITES=$(src_grep 'AtepLocalExporter|AtepStore')
TRACKING_SITES=$(src_grep 'tracking\.start|TrackingCallbackHandler')
LANGGRAPH_ADAPTER_SITES=$(src_grep 'instrument_langgraph')

# --- Trace content -----------------------------------------------------------
TRACE_LINES=0
[ -f "$TRACES" ] && TRACE_LINES=$(wc -l <"$TRACES" | tr -d ' ')
TS_SHAPE=false
if [ -f "$TRACES" ] && grep -q '"specVersion"' "$TRACES" 2>/dev/null; then TS_SHAPE=true; fi
MODEL_SEEN=$(( $(trace_has model_calls) + $(event_count model_call) ))
TOOL_SEEN=$(( $(trace_has tool_calls) + $(event_count tool_call) ))
MEMORY_SEEN=$(event_count memory_access)
POLICY_SEEN=$(event_count policy_check)

# ATEP is observed on disk, not in the trace file: count stream segments
# (<store>/streams/<stream>-NNNNNNNN.atep).
ATEP_SEEN=0
if [ -d "${WORKSPACE}/.agenomic/atep" ]; then
  ATEP_SEEN=$({ find "${WORKSPACE}/.agenomic/atep" -type f -name '*.atep' 2>/dev/null || true; } \
    | wc -l | tr -d ' ')
fi

# --- CLI checks --------------------------------------------------------------
run_check() {  # name, command...
  local name="$1"; shift
  if "$@" >"${OUT_DIR}/${name}.log" 2>&1; then echo pass; else echo "fail"; fi
}

VALIDATE=missing
if [ -f "${WORKSPACE}/genome.yaml" ] || [ -f "${WORKSPACE}/system.yaml" ]; then
  VALIDATE=$(run_check validate agm validate "$WORKSPACE" --level strict)
fi

# The CLI reads {trace_id, agent_id, input, output, tool_calls[].name}.
# TypeScript envelopes cannot be converted today; Python envelopes name the
# tool `tool` and the output `final_output`, so validate a converted copy.
TRACE_VALID=missing
if [ -f "$TRACES" ]; then
  if [ "$TS_SHAPE" = true ]; then
    TRACE_VALID=blocked_ts_shape
  elif command -v jq >/dev/null 2>&1; then
    if jq -c '. + {output: (.output // .final_output)}
              | .tool_calls |= ((. // []) | map(. + {name: (.name // .tool),
                  human_approval_present: (.human_approval_present // .approval_present)}))' \
         "$TRACES" >"${OUT_DIR}/traces.cli.jsonl" 2>"${OUT_DIR}/trace-convert.log"; then
      TRACE_VALID=$(run_check trace-validate agm trace validate "${OUT_DIR}/traces.cli.jsonl")
    else
      TRACE_VALID=fail
    fi
  else
    TRACE_VALID=$(run_check trace-validate agm trace validate "$TRACES")
  fi
fi

# Machine health, not workspace health; exits 3 on any failed check,
# including cloud_health on cloud profiles. Informational only.
DOCTOR=$(run_check doctor agm doctor)

LEDGER=missing
if [ -d "${WORKSPACE}/.agenomic/ledger" ]; then
  LEDGER=$(run_check ledger-status agm ledger status --store "${WORKSPACE}/.agenomic/ledger")
fi

# --- Report ------------------------------------------------------------------
cat >"$OUT" <<EOF
{
  "skill": "agenomic-sdk-integration",
  "phase": "validate",
  "workspace": "${WORKSPACE}",
  "traces": { "path": "${TRACES}", "lines": ${TRACE_LINES}, "typescript_shape": ${TS_SHAPE} },
  "coverage": {
    "run_tracing":    { "sites": ${RUN_SITES},    "observed": ${TRACE_LINES}, "status": "$(verdict "$RUN_SITES" "$TRACE_LINES")" },
    "model_calls":    { "sites": ${MODEL_SITES},  "observed": ${MODEL_SEEN},  "status": "$(verdict "$MODEL_SITES" "$MODEL_SEEN")" },
    "tool_calls":     { "sites": ${TOOL_SITES},   "observed": ${TOOL_SEEN},   "status": "$(verdict "$TOOL_SITES" "$TOOL_SEEN")" },
    "memory":         { "sites": ${MEMORY_SITES}, "observed": ${MEMORY_SEEN}, "status": "$(verdict "$MEMORY_SITES" "$MEMORY_SEEN")" },
    "policy_checks":  { "sites": ${POLICY_SITES}, "observed": ${POLICY_SEEN}, "status": "$(verdict "$POLICY_SITES" "$POLICY_SEEN")" },
    "atep":           { "sites": ${ATEP_SITES},   "observed": ${ATEP_SEEN},   "status": "$(verdict "$ATEP_SITES" "$ATEP_SEEN")" },
    "tracking":       { "sites": ${TRACKING_SITES}, "observed": null, "status": "$(verdict_unmeasurable "$TRACKING_SITES")" },
    "redaction":      { "sites": ${REDACT_SITES}, "observed": null, "status": "$(verdict_unmeasurable "$REDACT_SITES")" }
  },
  "cli_checks": {
    "bundle_validate": "${VALIDATE}",
    "trace_validate":  "${TRACE_VALID}",
    "doctor":          "${DOCTOR}",
    "ledger_status":   "${LEDGER}"
  },
  "warnings": {
    "instrument_langgraph_sites": ${LANGGRAPH_ADAPTER_SITES}
  },
  "status_legend": {
    "ok":         "call site exists AND the capability was observed in output",
    "partial":    "instrumented but not exercised by the smoke run; NOT a pass",
    "sites_only": "instrumented, but this script cannot observe it; verify by hand",
    "missing":    "no call site found"
  },
  "caveats": [
    "memory/policy observations come from TypeScript envelope events; Python records them only in tracking or canonical runs, which this script cannot see",
    "trace_validate runs on a converted copy (.agenomic/traces.cli.jsonl) for Python; blocked_ts_shape means the CLI cannot read TypeScript traces today",
    "instrument_langgraph_sites > 0: that adapter records nothing on current LangGraph; do not count it as tool coverage",
    "tracking is unmeasurable here: check session.events / to_jsonl() by hand",
    "atep is counted as stream segments on disk, not as trace content",
    "redaction is unmeasurable here: confirm by inspecting a trace for cleartext",
    "doctor checks the machine, not the workspace",
    "logs for each CLI check are in .agenomic/*.log"
  ]
}
EOF

echo "wrote ${OUT}"

# Surface the headline verdicts on stdout.
printf '\n%-16s %s\n' "run tracing"    "$(verdict "$RUN_SITES" "$TRACE_LINES")"
printf '%-16s %s\n'   "model calls"    "$(verdict "$MODEL_SITES" "$MODEL_SEEN")"
printf '%-16s %s\n'   "tool calls"     "$(verdict "$TOOL_SITES" "$TOOL_SEEN")"
printf '%-16s %s\n'   "agm validate"   "$VALIDATE"
printf '%-16s %s\n'   "trace validate" "$TRACE_VALID"
printf '%-16s %s\n\n' "agm doctor"     "$DOCTOR"

if [ "$LANGGRAPH_ADAPTER_SITES" -gt 0 ]; then
  echo "warn: instrument_langgraph found; it records nothing on current LangGraph (recipes/langgraph.md)" >&2
fi
if [ "$TRACE_LINES" -eq 0 ]; then
  echo "error: no traces produced; run scripts/smoke-run.sh before validating" >&2
  exit 1
fi
