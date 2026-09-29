#!/usr/bin/env bash
# Phase 1: mechanical inspection of a target repository.
# Read-only: never writes inside the target except under .agenomic/.
# Idempotent: safe to run repeatedly.
set -euo pipefail

[ -d "${1:-.}" ] || { echo "error: no such directory: ${1:-.}" >&2; exit 2; }
WORKSPACE="$(cd "${1:-.}" && pwd)"
OUT_DIR="${WORKSPACE}/.agenomic"
OUT="${OUT_DIR}/inspect.json"

HAVE_AGM=true
command -v agm >/dev/null 2>&1 || {
  echo "warn: 'agm' not found on PATH; falling back to grep-only inspection" >&2
  echo "      install the Agenomic CLI for authoritative detection" >&2
  HAVE_AGM=false
}

mkdir -p "$OUT_DIR"

# --- CLI detection -----------------------------------------------------------
# detect.json: flat array of inference records (runtime, framework, provider).
# detect.txt: the human dry-run, the only output that lists recovered
# workflows, a synthesized system.yaml and detected env vars.
DETECT="${OUT_DIR}/detect.json"
DETECT_TXT="${OUT_DIR}/detect.txt"
if [ "$HAVE_AGM" = true ]; then
  if ! (cd "$WORKSPACE" && agm init --dry-run --format json) \
        >"$DETECT" 2>"${OUT_DIR}/detect.err"; then
    echo "warn: 'agm init --dry-run' failed (see .agenomic/detect.err)" >&2
    echo '[]' >"$DETECT"
  fi
  (cd "$WORKSPACE" && agm init --dry-run) >"$DETECT_TXT" 2>>"${OUT_DIR}/detect.err" || true
else
  echo '[]' >"$DETECT"
  : >"$DETECT_TXT"
fi

txt_count() { { grep -E "$1" "$DETECT_TXT" 2>/dev/null || true; } | wc -l | tr -d ' '; }
DETECTED_WORKFLOWS=$(txt_count '^would write workflows/')
DETECTED_SYSTEM=$(txt_count '^would write system\.yaml')
DETECTED_ENV=$(txt_count '^detected env vars')

# List files matching an extended regex. `grep` exits 1 on no-match, which
# `set -o pipefail` would turn into a script abort, hence the `|| true`.
match_files() {
  grep -rEl "$1" "$WORKSPACE" \
    --include='*.py' --include='*.ts' --include='*.tsx' \
    --include='*.js' --include='*.rs' \
    --exclude-dir=node_modules --exclude-dir=.git --exclude-dir=target \
    --exclude-dir=.venv --exclude-dir=venv --exclude-dir=dist \
    --exclude-dir=__pycache__ --exclude-dir=.agenomic \
    2>/dev/null || true
}

count() { match_files "$1" | wc -l | tr -d ' '; }

# Up to 20 matching files as a JSON array, relative to the workspace.
files_for() {
  match_files "$1" | head -20 \
    | sed "s|^${WORKSPACE}/||" \
    | awk 'BEGIN{printf "["} {printf "%s\"%s\"", (NR>1?",":""), $0} END{print "]"}'
}

exists() { [ -e "${WORKSPACE}/$1" ] && echo true || echo false; }

# --- Signals -----------------------------------------------------------------
MODEL_CALLS=$(count 'openai|anthropic|OpenAI\(|Anthropic\(|chat\.completions|messages\.create|bedrock|generativeai|ollama|vllm|mistralai|cohere')
LANGGRAPH=$(count 'langgraph|StateGraph|add_conditional_edges')
LANGCHAIN=$(count 'langchain_core|from langchain|langchain\.')
CREWAI=$(count 'crewai|from crewai|Crew\(')
TEMPORAL=$(count 'temporalio|@workflow\.defn|@activity\.defn')
MCP=$(count 'mcp|modelcontextprotocol|call_tool')
OTEL=$(count 'opentelemetry|OTEL_')
LANGSMITH=$(count 'langsmith|LANGCHAIN_TRACING')
LANGFUSE=$(count 'langfuse')
DATADOG=$(count 'datadog|ddtrace')
ALREADY=$(count 'agenomic|trace_agent_run|traceAgentRun')
THREADS=$(count 'ThreadPoolExecutor|threading\.Thread|ProcessPoolExecutor')
QUEUES=$(count 'celery|rq\.Queue|arq|bullmq|sidekiq')

cat >"$OUT" <<EOF
{
  "skill": "agenomic-sdk-integration",
  "phase": "inspect",
  "workspace": "${WORKSPACE}",
  "detect_json": ".agenomic/detect.json",
  "detect_txt": ".agenomic/detect.txt",
  "cli_detection": {
    "workflows_to_write": ${DETECTED_WORKFLOWS},
    "system_yaml_to_write": ${DETECTED_SYSTEM},
    "env_vars_line": ${DETECTED_ENV}
  },
  "manifests": {
    "pyproject_toml":     $(exists pyproject.toml),
    "requirements_txt":   $(exists requirements.txt),
    "package_json":       $(exists package.json),
    "cargo_toml":         $(exists Cargo.toml),
    "dockerfile":         $(exists Dockerfile),
    "docker_compose":     $(exists docker-compose.yml),
    "github_actions":     $(exists .github/workflows),
    "gitlab_ci":          $(exists .gitlab-ci.yml),
    "env_example":        $(exists .env.example)
  },
  "existing_bundle": {
    "genome_yaml":            $(exists genome.yaml),
    "system_yaml":            $(exists system.yaml),
    "workflow_yaml":          $(exists workflow.yaml),
    "workflows_dir":          $(exists workflows),
    "behavior_contract_yaml": $(exists behavior.contract.yaml),
    "agent_lock_yaml":        $(exists agent.lock.yaml)
  },
  "signals": {
    "model_calls":            ${MODEL_CALLS},
    "langgraph":              ${LANGGRAPH},
    "langchain":              ${LANGCHAIN},
    "crewai":                 ${CREWAI},
    "temporal":               ${TEMPORAL},
    "mcp":                    ${MCP},
    "already_instrumented":   ${ALREADY},
    "thread_boundaries":      ${THREADS},
    "background_queues":      ${QUEUES}
  },
  "existing_observability": {
    "opentelemetry": ${OTEL},
    "langsmith":     ${LANGSMITH},
    "langfuse":      ${LANGFUSE},
    "datadog":       ${DATADOG}
  },
  "candidate_model_call_files": $(files_for 'chat\.completions|messages\.create|OpenAI\(|Anthropic\(|invoke_model'),
  "notes": [
    "signals are file counts, not call counts; confirm by reading the files",
    "detect.json rows with source=defaults are placeholders, not findings",
    "thread_boundaries > 0: verify contextvar propagation (references/architecture.md)",
    "already_instrumented > 0 or genome_yaml: re-integration; use 'agm update --dry-run', then '--no-commit'"
  ]
}
EOF

echo "wrote ${OUT}"
if [ "$MODEL_CALLS" -eq 0 ]; then
  echo "warn: no model-call sites found; this may not be an agent project (reason: not_an_agent)" >&2
fi
