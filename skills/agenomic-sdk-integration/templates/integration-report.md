# Integration report template

Two artifacts: `.agenomic/integration-report.json` (parseable) and a human
summary on stdout. Both are required.

The report's value is its honesty. A report claiming full coverage on a
partial integration causes a team to trust traces that will not replay, and
an auditor to request evidence that does not exist.

---

## JSON

```json
{
  "skill": "agenomic-sdk-integration",
  "skill_version": "0.2.0",
  "mode": "integrate",
  "workspace": "/path/to/repo",
  "agent_id": "agent://acme/claims",
  "project": {
    "runtime": "python 3.12",
    "package_manager": "uv",
    "framework": "langgraph 1.2",
    "providers": ["anthropic/claude-sonnet-5", "openai/text-embedding-3-small"],
    "environment": "production",
    "architecture": "single graph, 6 nodes, 1 irreversible tool"
  },
  "implemented": [
    { "capability": "run_tracing", "file": "api/routes.py", "line": 41,
      "how": "@trace_agent_run on handle_claim" },
    { "capability": "model_calls", "file": "llm/client.py", "line": 18,
      "how": "instrument_anthropic at client construction" },
    { "capability": "tool_calls", "file": "tools/payout.py", "line": 12,
      "how": "@traced_tool on payout_api and kb_lookup (instrument_langgraph records nothing on LangGraph 1.x)" }
  ],
  "reused": [
    { "system": "opentelemetry", "how": "trace id copied to label otel.trace_id; tracer untouched" }
  ],
  "generated_files": ["genome.yaml", "agent.lock.yaml", "workflows/claim.yaml",
                      "docs/agenomic-integration.md", ".github/workflows/agenomic.yml"],
  "modified_files": ["api/routes.py", "llm/client.py", "tools/payout.py", ".env.example"],
  "coverage": { "source": ".agenomic/coverage.json" },
  "replay": {
    "ready": true,
    "mode": "deterministic_offline",
    "reason": "local replay checks the contract over recorded traces; statistical replay needs Agenomic Cloud",
    "fixtures": "evals/traces.cli.jsonl (12 traces, converted to the CLI trace shape)"
  },
  "governance": { "enabled": ["failure_detection", "drift_detection", "loop_detection"],
                  "not_enabled": ["intent_tracking"],
                  "why_not": "forbidden_intents not defined; needs a human decision" },
  "security": [
    { "finding": "no secrets in genome.yaml", "status": "ok" },
    { "finding": "payloads redacted at boundary (customer.ssn hashed)", "status": "ok" },
    { "finding": "signing key generated per process", "status": "action_required",
      "action": "persist via `agm ledger keys generate`; a fresh key per boot breaks chain verification" }
  ],
  "performance": [
    { "note": "JSONL + local ATEP export only; no network call on the hot path" },
    { "note": "hashing adds ~0.3ms per model call (BLAKE3 over canonical CBOR)" }
  ],
  "not_covered": [
    { "capability": "memory_tracking", "reason": "no memory field in the Python trace envelope; tracking not enabled",
      "kind": "missing_agenomic_surface", "workaround": "recorded as ToolCall protocol=memory" },
    { "capability": "policy_harness", "reason": "business rules undefined",
      "kind": "needs_human_decision", "todo": "define forbidden actions and approval rules" }
  ],
  "remaining_manual_actions": [
    "Set AGENOMIC_API_KEY in the production secret store",
    "Decide approval policy for payout_api (irreversible)",
    "Persist the ATEP signing key outside the repo"
  ],
  "next_steps": [
    "Run `agm replay . evals/traces.cli.jsonl` to establish a baseline",
    "Enable `agm track start --ledger` in production",
    "Add the CI workflow gate once the baseline is stable"
  ],
  "status": "partial"
}
```

`status`: `analyzed` (plan only) · `integrated` (everything planned is `ok`) ·
`partial` (anything `⚠`/`❌`) · `error`.

**Use `partial` whenever coverage is incomplete.** `integrated` is not a
summary of effort; it is a claim about the resulting system.

Every `not_covered` entry carries a `kind`:

- `missing_agenomic_surface`: the platform cannot do it today. Cite
  `references/capability-matrix.md`.
- `needs_human_decision`: business policy. Never resolve this yourself.
- `out_of_scope`: deliberately excluded, with the reason.

---

## Human summary

```text
Agenomic SDK Integration Report          status: PARTIAL

Project:      acme-claims  (python 3.12 / uv)
Framework:    LangGraph 1.2
Providers:    anthropic/claude-sonnet-5, openai/text-embedding-3-small
Environment:  production
Agent:        agent://acme/claims

Implemented
  ✅ run tracing        api/routes.py:41       @trace_agent_run
  ✅ model calls        llm/client.py:18       instrument_anthropic
  ✅ tool calls         tools/payout.py:12     @traced_tool
  ✅ ATEP signing       llm/observability.py   AtepLocalExporter
  ✅ redaction          boundary               customer.ssn hashed

Reused
  ✅ OpenTelemetry      untouched; trace id linked via label otel.trace_id

Coverage                              (measured by scripts/validate-integration.sh)
  run tracing        ✅ 14 traces
  model calls        ✅ 14/14 traces
  tool calls         ✅ 62 across 14 traces
  memory             ⚠  no envelope field; recorded as ToolCall
  policy harness     ⚠  wired, no rules defined
  redaction          ⚙  wired; verified by hand, no cleartext SSN in traces
  ATEP               ✅ 14 signed events
  ledger             ✅ durable_low_latency, 0 dead letters
  replay             ✅ deterministic (local), 12 fixtures; statistical needs cloud
  drift / loop       ✅ agm track
  intent tracking    ❌ forbidden_intents undefined

Not covered
  memory tracking    missing surface  → recorded as ToolCall protocol=memory
  policy harness     human decision   → define forbidden actions
  intent tracking    human decision   → define forbidden intents

Security
  ✅ no secrets in manifests or traces
  ⚠  signing key generated per process: persist it (breaks chain verification)

Remaining manual actions
  1. Set AGENOMIC_API_KEY in the production secret store
  2. Decide the approval policy for payout_api (irreversible effect)
  3. Persist the ATEP signing key outside the repository

status: PARTIAL · 3 capabilities need a human decision before this is complete
```

## Rules

1. Never `✅` a capability whose call site was never exercised. That is `⚠`.
   If the script could not observe it at all, that is `⚙ sites_only`: say how
   you verified it by hand, or say that you did not.
2. Every `❌`/`⚠` states a reason and its `kind`.
3. Cite `file:line` for every implemented capability: a reviewer must be able
   to check the claim in one jump.
4. Distinguish "Agenomic cannot" from "a human has not decided yet". They have
   different owners and different fixes.
5. Report failed tests with their output. A green report over a red suite is a
   false report.
