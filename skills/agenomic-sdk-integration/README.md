# agenomic-sdk-integration

Integrate Agenomic into an AI-agent codebase **you did not write**: a running
service, a legacy agent, someone else's repo.

The skill inspects the project, maps how the agent actually runs, picks the
smallest correct set of Agenomic surfaces for that architecture, wires them at
real behavioral boundaries, measures the resulting coverage, and reports the
gaps honestly.

## Which skill do I want?

| Situation | Skill |
|---|---|
| An agent registers **itself** as a bundle and evaluates itself | [`self-graft-and-evaluate`](../self-graft-and-evaluate/) |
| You are instrumenting **existing code**, yours or not | this one |

They compose: this skill drives `agm init` / `agm enrich` for manifest
scaffolding rather than reimplementing detection, and defers to the sibling
skill for self-evaluation.

## Invocation

### For a human

> Use the `agenomic-sdk-integration` skill on this repository in `analyze`
> mode. Don't modify anything. Show me the plan first.

Then, once the plan looks right:

> Proceed in `integrate` mode.

### For a coding agent

Load `SKILL.md` and follow it in order. It is the procedure; this README is
orientation.

```
skill:     agenomic-sdk-integration
workspace: /path/to/repo
mode:      analyze | minimal | integrate | full | replay | monitor | ci
```

### Modes

| Mode | Does |
|---|---|
| `analyze` **(default)** | Inspect and plan. No source changes: writes only reports under `.agenomic/`. |
| `minimal` | Init + run, model and tool tracing + exporter. Nothing else. |
| `integrate` | Inspect → plan → implement → validate → test → report. |
| `full` | Everything applicable, including ledger, tracking and RMP. |
| `replay` | Replay readiness and fixtures only (deterministic offline; statistical is cloud). |
| `monitor` | Live tracking and governance only. |
| `ci` | Release validation and CI wiring only. |

`analyze` is the default on purpose: the plan comes before the code. An
integration built on a wrong architecture map instruments the wrong
boundaries and produces traces that will not replay.

## What it produces

```
.agenomic/inspect.json             mechanical inspection
.agenomic/detect.json              agm init --dry-run --format json output
.agenomic/detect.txt               agm init --dry-run human output (workflows, env vars)
.agenomic/integration-plan.json    the plan (analyze stops here)
.agenomic/coverage.json            measured coverage matrix
.agenomic/integration-report.json  final report
docs/agenomic-integration.md       operator documentation
```

Plus instrumentation in the target's source, `genome.yaml` and friends from
`agm init`, and optionally a CI workflow.

## Layout

```
agenomic-sdk-integration/
├── SKILL.md                  the procedure, start here
├── manifest.yaml             inputs, outputs, phases
├── references/
│   ├── capability-matrix.md  what actually exists, per surface (read first)
│   ├── architecture.md       layering and boundary selection
│   └── troubleshooting.md
├── recipes/
│   ├── python.md  typescript.md  rust.md
│   ├── langgraph.md  crewai.md  temporal.md  mcp.md
│   └── providers.md  observability.md
├── templates/
│   ├── integration-report.md  env.example
│   └── ci-github-actions.yml  agenomic-integration.md
└── scripts/
    ├── inspect-target.sh       → .agenomic/inspect.json
    ├── validate-integration.sh → .agenomic/coverage.json
    └── smoke-run.sh            end-to-end + resilience check
```

Bundle manifest templates (`genome.yaml`, `workflows/*.yaml`, `system.yaml`,
`behavior.contract.yaml`) are **not** duplicated here: `agm init` generates
them, and the sibling skill ships the hand-authoring templates.

## Design commitments

**Honesty over coverage.** The goal is not to enable the most Agenomic
features. It is to enable the correct ones and state plainly what is not
covered. `references/capability-matrix.md` records what each SDK and the CLI
can actually do, verified against source (last on 2026-09-29), not assumed.
Four findings shape most plans:

- Turns are not a trace unit. The granularity is `agent.step.*`; only the
  Python tracking stream has `turn.*`, and per-turn ledger chains are
  deferred.
- The Python and TypeScript SDKs are close to parity (tracing, tracking, RMP,
  cloud Protect, tool router, benchmarks). ATEP signing, the Anthropic
  adapter and the LangChain / LangGraph tracking handler are Python only;
  memory, policy and feedback in the trace envelope, and the Next.js adapter,
  are TypeScript only.
- The ledger, trace replay, evidence and governance engines are CLI-side. No
  SDK writes to the ledger.
- The CLI does not read SDK traces as is: Python traces with tool calls need
  a one-line conversion, and TypeScript traces cannot feed `agm trace` or
  `agm replay` today.

**Never break the agent.** Instrumentation is additive. Telemetry failures are
logged, never raised; where an SDK surface raises (Python cloud `Client`
calls, TypeScript trace emission with an endpoint), the integration guards
it. `scripts/smoke-run.sh` fails the integration if the agent cannot
complete with Agenomic unreachable.

**Coverage is measured, not asserted.** A capability is `✅` only when a call
site exists *and* a real trace contains the corresponding field.
`scripts/validate-integration.sh` produces the matrix; the model does not
narrate it.

**Business policy is not inferred.** Criticality, forbidden actions, approval
requirements, regulatory classification and alert recipients are human
decisions. The skill emits `TODO`s and asks.

## Requirements

- `agenomic` CLI 0.3 alpha or later (release tag `v0.3.0-apha.0`; `agm` is
  the same binary)
- `agenomic` ≥ 0.1.3 (PyPI) for Python targets, optional
- `@treansai/agenomic-typescript` ≥ 0.1.1 (npm) for Node targets, optional
- `jq` for trace conversion and coverage counting
- No network access required; cloud is opt-in
