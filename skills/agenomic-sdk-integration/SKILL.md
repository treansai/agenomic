---
name: agenomic-sdk-integration
description: >
  Procedure for integrating Agenomic into an AI-agent codebase you did not
  write. Inspects the repository, maps how the agent actually runs, selects
  the minimal correct set of Agenomic surfaces, wires them at real behavioral
  boundaries, validates coverage, and reports the gaps honestly.
inputs: [workspace, "mode?", "language?", "agent_id?", "environment?"]
outputs: [plan, coverage, report, status]
---

# Agenomic SDK integration

You are integrating Agenomic into **someone else's running agent**. The code
predates you. It works. Your job is to make its behavior observable,
replayable, and auditable **without changing what it does**.

> **Not this skill?** If the agent is registering *itself* as a bundle from
> its own runtime, use [`self-graft-and-evaluate`](../self-graft-and-evaluate/)
> instead. That skill owns bundle scaffolding and self-evaluation. This one
> owns *instrumenting unfamiliar code* and delegates scaffolding to
> `agm init` / `agm enrich`. Do not re-derive what `agm init` already detects.

## The one rule

**Do not write code before you understand the project.** `analyze` is the
default mode for exactly this reason. Producing a plan is not a formality:
an integration built on a wrong architecture map instruments the wrong
boundaries and produces traces that cannot be replayed.

## Preconditions

1. `agm --version` succeeds. If not: stop, `status: error`, `reason: cli_missing`.
   (`agenomic` and `agm` are the same binary; this skill uses `agm`.)
2. `workspace` is readable, and writable if `mode != analyze`.
3. The repository actually runs an AI agent. If inspection finds no model
   call anywhere, stop with `reason: not_an_agent`; do not instrument a
   plain web service.

## Ground truth before you start

Read [`references/capability-matrix.md`](references/capability-matrix.md)
**before planning**. It records, per surface, what the Python SDK, the
TypeScript SDK, and the CLI can actually do today. Four facts change most
plans, and all four contradict common assumptions:

- **Turns are not a trace unit.** Trace envelopes and the CLI tracking
  vocabulary have no turn; the tracking granularity is `agent.step.started` /
  `agent.step.completed`, and per-turn ledger chains are deferred. Only the
  Python tracking stream emits `turn.*`, and `agm track` rejects it. Never put
  a `turn_id` in a trace envelope. Map conversational turns onto steps, or
  onto separate runs.
- **The SDKs are close to parity, not identical.** Both have tracing, online
  tracking, RMP, cloud Protect, the tool router and benchmarks. ATEP signing,
  the Anthropic adapter and the LangChain / LangGraph tracking handler are
  Python only; memory, policy and human feedback inside the trace envelope
  and the Next.js adapter are TypeScript only. Plan for the SDK you actually
  have.
- **Proofs are CLI-side.** No SDK writes to the ledger, replays traces or
  produces evidence. An SDK emits traces and events; the CLI turns them into
  ledger entries, replay reports and evidence. Local replay is deterministic
  only; statistical replay is Agenomic Cloud.
- **The CLI does not read SDK traces as is.** `agm trace validate` and
  `agm replay` reject Python traces that contain tool calls (the SDK writes
  `tool`, the CLI wants `name`) and every TypeScript trace. Python traces
  convert with one `jq` line; TypeScript traces cannot feed the CLI today
  (capability matrix §2).

## Decision tree

```
Does the repo make model calls?
 ├─ No  → stop: not_an_agent
 └─ Yes
     ↓
Runtime (`runtime.runtime_kind` from `agm init --dry-run`)
 ├─ python      → Python SDK      → recipes/python.md
 ├─ node        → TypeScript SDK  → recipes/typescript.md
 ├─ rust        → CLI + JSONL     → recipes/rust.md   (no Rust tracing SDK today)
 └─ go / other  → CLI + JSONL trace emission in the CLI trace shape
     ↓
Framework
 ├─ LangGraph / LangChain → Python `TrackingCallbackHandler` + manual tool calls
 │                          (`instrument_langgraph` is a no-op) → recipes/langgraph.md
 ├─ MCP       → `trace_mcp_call` / `recordMCPToolCall` → recipes/mcp.md
 ├─ CrewAI    → manifest + compile target only; MANUAL instrumentation
 │                                                     → recipes/crewai.md
 ├─ Temporal  → topology detected by `agm init`; MANUAL instrumentation
 │                                                     → recipes/temporal.md
 └─ Custom    → instrument the four boundaries below
     ↓
Environment
 ├─ local      → JSONL exporter, `agm validate`, `agm trace validate` (Python, converted)
 └─ production → + ATEP store (Python), + `agm ledger`, + `agm track`
     ↓
Critical workflow? (money, health, legal, irreversible effects)
 ├─ No  → Monitor
 └─ Yes → Review + Monitor + Protect, and `agm gate check` at tool boundaries
```

## Phase 1: Inspect

Let the CLI do the detection it already does well:

```sh
cd "$workspace"
mkdir -p .agenomic
agm init --dry-run --format json > .agenomic/detect.json
agm init --dry-run > .agenomic/detect.txt
```

`--dry-run` writes nothing (not even `.agenomic/`) and exits 0. The JSON is a
**flat array of inference records**, not a nested genome:

```json
[
  {"field": "agent.name",             "value": "demo",   "source": "pyproject",
   "evidence": "project.name"},
  {"field": "runtime.framework",      "value": "custom", "source": "defaults",
   "evidence": "no known framework dependency"},
  {"field": "runtime.model_provider", "value": "openai", "source": "defaults",
   "evidence": "built-in default"}
]
```

`field` is a dotted genome path; `source` is the detection source, applied
lowest to highest priority: `defaults` < `git` < `readme` < `go-mod` <
`cargo` < `package-json` < `pyproject` < `dockerfile` < `agenomic-yaml`. The
runtime is `runtime.runtime_kind` (`python`, `node`, `rust`, `go`); there is
no `language` field, and TypeScript projects report `node`.

**Read `source` before trusting `value`.** `source: "defaults"` means *not
detected*: `agent://example/new`, `Example Agent`, `openai`, `gpt-4o` and
`framework: custom` are placeholders, not findings. Only records whose
`source` is a real file are evidence. Treat every `defaults` row as a
question to answer yourself. Detection only runs when a `pyproject.toml`,
`package.json`, `Cargo.toml`, `go.mod` or `agenomic.yaml` exists; without one,
every row is a default.

The JSON omits the richer detection. The human output (`detect.txt`) also
lists recovered workflow topology (`would write workflows/<slug>.yaml
(langgraph engine, from app.py:app)`), a synthesized `system.yaml`, and the
detected env vars (a `detected env vars` line listing required and optional
variables).
Read both files.

Then read what the CLI cannot infer, with your own eyes:

- the request entrypoint (HTTP route, queue consumer, CLI `main`, cron);
- the **agent loop**: where a model response decides the next action;
- retries, recursion, and any `while` around a model call;
- background workers and async fan-out (traces must not cross task boundaries
  silently; see the contextvar note in `recipes/python.md`);
- existing observability (OpenTelemetry, LangSmith, Langfuse, Datadog);
- secret handling.

`scripts/inspect-target.sh` runs the mechanical part of this and writes
`.agenomic/inspect.json`.

Produce an architecture map. Keep it small and true:

```text
POST /claims  →  Orchestrator (loop, max 6 iters)
                  ├─ research_agent   → tools: web_search, kb_lookup (MCP)
                  ├─ risk_agent       → model: claude-sonnet-5
                  └─ decision_agent   → tool: payout_api  ⚠ irreversible
```

Mark irreversible effects with ⚠. They drive the Protect/gate decision later.

## Phase 2: Detect

Resolve four things and write them into the plan:

| What | How | Recipe |
|---|---|---|
| Providers + models | read the client construction sites; `agm init` reports one provider at most | `recipes/providers.md` |
| Framework | imports and builder calls, not file names | per-framework recipe |
| Boundaries | see below | `references/architecture.md` |
| Existing observability | dependency manifest + tracer init | `recipes/observability.md` |

**Instrument exactly four boundaries.** Not every function:

1. **run**: one agent execution, end to end;
2. **model call**: every LLM invocation;
3. **tool call**: every effect on the outside world;
4. **memory access**: every read/write of persistent agent state. The
   TypeScript trace records it; in Python it goes to the tracking stream (or
   a canonical run), and an envelope-only Python integration records it as a
   gap.

A fifth, **workflow step**, applies when the runtime has an explicit graph.

## Phase 3: Plan

Write `.agenomic/integration-plan.json` and print the human form. **In
`analyze` mode you stop here** and report `status: analyzed`.

```text
Agenomic Integration Plan

Project:      acme-claims
Runtime:      python 3.12 (uv)
Framework:    LangGraph 1.2
Providers:    anthropic/claude-sonnet-5, openai/text-embedding-3-small
Architecture: single graph, 6 nodes, 1 irreversible tool, memory in Postgres
Environment:  production

[Required]
  run tracing     @trace_agent_run on api/routes.py:handle_claim:41
  model calls     instrument_anthropic on llm/client.py:18
  tool calls      @traced_tool on tools/payout.py:12, tools/kb.py:30
  export          MultiExporter(JsonlExporter, AtepLocalExporter)

[Recommended]
  node tracking   TrackingCallbackHandler on api/routes.py:47 (local session)
  ledger          agm ledger init + agm track start --ledger   (production)
  online tracking agm track (drift + loop detection)
  replay          evals/ fixtures converted to the CLI trace shape

[Optional / blocked]
  memory          ⚠ tracking `memory_write` only; no envelope field in Python
  policy harness  ⚠ needs business rules: TODO, see Phase 3 safety rule
  langgraph adapt ❌ instrument_langgraph records nothing on LangGraph 1.x
```

For each line give: why, target file:line, expected impact, complexity.

### Safety rule: never fabricate policy

Technical facts you may discover. Business policy you may not. If you cannot
determine it from the code, emit a `TODO` in the plan and ask:

- which actions are forbidden;
- which require human approval;
- regulatory classification and sensitive-data rules;
- risk thresholds and alert recipients.

`criticality`, `domain` and behavior-contract rules are proposed by
`agm enrich` and confirmed by a human; `allowed_intents` / `forbidden_intents`
are tracking configuration (`agm track start --config`) set by a human. None
of them are filled by you. `agm enrich --dry-run` still calls an LLM; it only
skips the write.

## Phase 4: Integrate

Minimal correct integration, in this order. Stop after step 4 in `minimal` mode.

1. **Bundle manifests**: `agm init` when no bundle exists (it exits 2 if
   `genome.yaml` is already there; never `--force` over a hand-edited
   manifest). When a bundle exists, `agm update --dry-run`, then
   `agm update --no-commit`: by default `agm update` **commits** inside a git
   repo, refuses on `main` / `master` / `release/*`, and exits 1 when nothing
   changed. In someone else's repo, never let it commit for them.
2. **Run tracing** at the entrypoint.
3. **Provider instrumentation** at client construction.
4. **Tool instrumentation** at the effect boundary.
5. **Exporter wiring**: JSONL locally; add `AtepLocalExporter` (Python) for
   signed history. Cloud export is opt-in and must not be able to fail the
   agent (capability matrix §8).
6. **Ledger**: `agm ledger init`; bind with `agm track start --ledger`.
7. **Online tracking**: `agm track` with drift/loop config.
8. **Replay fixtures**: capture traces into `evals/`, converted to the CLI
   trace shape (Python). TypeScript traces cannot feed `agm replay` today.

Follow the recipe for the language and framework. The recipes contain the
real API; do not improvise call signatures.

### Prefer the native path

```
Agenomic adapter  →  framework hook  →  existing OTel span  →  manual recording
```

Only fall through when the level above genuinely does not exist.
`references/capability-matrix.md` says which adapters are real: **openai,
anthropic, huggingface, mcp, and the LangChain / LangGraph
`TrackingCallbackHandler`** (Python); **openai, huggingface, mcp, next**
(TypeScript). `instrument_langgraph` exists but records nothing on current
LangGraph. Everything else is manual.

### Non-negotiables while editing

- **Never crash the agent for telemetry.** Export failures are logged, not
  raised. The Python exporters already behave this way; do not add a
  `raise`. Cloud-mode `Client` calls (Python) and trace emission with an
  `endpoint` (TypeScript) do **not**: guard them, or keep the agent's client
  local (capability matrix §8). Strict enforcement is a deliberate,
  separately-configured choice.
- **Additive only.** Wrap; do not restructure. No prompt content is edited.
- **No secrets anywhere.** Not in `genome.yaml`, not in traces, not in ledger
  payloads. Ledger payloads are hash-committed by default; keep it that way.
- **Redact at the boundary**: `RedactionEngine` on `@trace_agent_run`
  (Python), the `redact` option on `traceAgentRun` / `createTrace`
  (TypeScript, applied at `build()`). Keep secrets out of metadata, which
  neither redacts.
- **Preserve existing observability.** Add alongside; reuse the existing
  trace id as a label. Never rip out OpenTelemetry.
- **Never sample** policy violations, approvals, security events, release
  decisions, or critical failures.

## Phase 5: Validate

```sh
scripts/validate-integration.sh "$workspace"
```

It runs `agm validate --level strict`, `agm trace validate` (advisory, on a
converted copy for Python), `agm doctor` (machine health, informational) and
`agm ledger status`, counts model and tool calls in the raw trace JSONL, and
greps the target for actual instrumentation call sites, then writes
`.agenomic/coverage.json`. Coverage is **measured, not asserted**.

Four statuses, and the distinction matters:

| Status | Meaning | Glyph |
|---|---|---|
| `ok` | call site exists **and** the capability was observed in output | ✅ |
| `partial` | instrumented but not exercised by the smoke run | ⚠ |
| `sites_only` | instrumented, but the script cannot observe it: verify by hand | ⚙ |
| `missing` | no call site found | ❌ |

Reporting `✅` for a wired-but-never-exercised path is the single most
damaging thing this skill can do. But systematic under-reporting is nearly as
bad: it sends a team hunting a gap that does not exist. `sites_only` exists
so an unmeasurable capability (redaction) is never confused with an
unexercised one.

## Phase 6: Test

```sh
scripts/smoke-run.sh "$workspace"
```

Run the project's own tests first: an integration that breaks the suite is a
failed integration, regardless of coverage. Then exercise: a normal run, a
model failure, a tool failure, a loop, and **Agenomic unreachable** (the agent
must still complete). The unreachable check needs the name of the variable
the project uses for its Agenomic URL
(`AGENOMIC_SMOKE_ENDPOINT_VARS=MYAPP_AGENOMIC_URL`);
without it the script reports the check as not exercised. If the project has
no test entrypoint, say so rather than inventing one.

## Phase 7: Report

Write `.agenomic/integration-report.json` and print the summary from
[`templates/integration-report.md`](templates/integration-report.md). Then
create or extend `docs/agenomic-integration.md` from
[`templates/agenomic-integration.md`](templates/agenomic-integration.md);
extend it if it exists, never duplicate.

The report must state, explicitly:

- what is instrumented, with file:line;
- what was reused rather than replaced;
- **what is not covered and why**, split into "blocked on a missing Agenomic
  surface" versus "blocked on a human decision";
- replay readiness, and whether replay is deterministic or statistical;
- remaining manual actions.

Never report `integrated` when coverage is partial. Use `partial`.

## Validation checklist

- [ ] `.agenomic/detect.json` and `detect.txt` read; architecture map matches the real call path
- [ ] Plan written before any edit
- [ ] Instrumentation only at the four boundaries
- [ ] Native adapter used wherever one exists
- [ ] No secrets in manifests, traces, or ledger payloads
- [ ] Redaction configured at the boundary
- [ ] Existing observability intact
- [ ] `agm validate . --level strict` green
- [ ] `agm update` never committed on the project's behalf
- [ ] Smoke run produced a trace containing model **and** tool calls
- [ ] Agent still completes with Agenomic unreachable (or the check is reported as not exercised)
- [ ] Project's own test suite still passes
- [ ] Coverage matrix generated by script, not narrated
- [ ] Every `❌` and `⚠` has a stated reason
- [ ] Business-policy fields left as `TODO`, not invented

## Failure handling

- **`agm init` detects nothing**: the repo may use an unsupported framework.
  Fall back to manual boundary discovery; do not abort if model calls exist.
- **`agm validate` fails 3× **: stop, surface the diagnostics, ask the user.
- **The entrypoint is ambiguous** (several plausible run boundaries): ask.
  Guessing here produces traces that replay against the wrong unit.
- **A capability the user asked for does not exist**: say so, cite
  `references/capability-matrix.md`, and offer the CLI path if there is one.
  Never stub a fake SDK call to satisfy a request.
- **The agent is non-deterministic**: expected. Local `agm replay` is a
  deterministic lower bound over recorded traces (`--runs-per-trace` > 1 is a
  no-op); statistical replay is `agm cloud push-replay --mode statistical`.
  Do not force determinism.

## References

- [`references/capability-matrix.md`](references/capability-matrix.md): what actually exists, per surface
- [`references/architecture.md`](references/architecture.md): layering and boundary selection
- [`references/troubleshooting.md`](references/troubleshooting.md): empty traces, missing calls, queue backlog
- CLI: `agenomic-cli/docs/command-reference.md`, `atep-ledger.md`, `replay-local.md`, `tool-boundary-gate.md`, `review-monitor-protect.md`, `rmp/`, `BACKEND_GAPS.md`
- SDKs: `agenomic-python/docs/` (`integrations.md`, `tracking.md`, `protect.md`, `benchmarks.md`), `agenomic-typescript/README.md` and `docs/`
