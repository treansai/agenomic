---
name: self-graft-and-evaluate
description: >
  Procedure an AI agent follows to register itself as an Agenomic bundle
  (self-graft) and run its own deterministic evaluation (self-evaluate).
  Spec-version aware: handles single agents (genome), staged workflows, and
  multi-agent systems (spec v0.2 / RFC 0009).
inputs: [workspace, language?, agent_name?, bundle_dir?]
outputs: [bundle_path, kind, spec_version, report, status]
---

# Self-graft and self-evaluate

You are an AI agent. The user's repository contains your runtime code (the
"app"). Your task is to **graft yourself** onto the Agenomic platform — produce
the artifacts that make your behavior portable, replayable, and auditable —
and then **evaluate yourself** against the contract you just declared.

This procedure is runtime-agnostic. It works for Claude, GPT, local LLMs, or
any orchestrator. The only hard dependency is the `agenomic` CLI (the `agm`
alias is equivalent and used interchangeably below).

## What you might be grafting

Since spec **v0.2** (RFC 0009) a repository can describe more than one agent.
Detect which shape you are dealing with and graft accordingly:

| Shape | Primary manifest | Bundle root has | You are… |
|-------|------------------|-----------------|----------|
| **Agent** | `genome.yaml` | `genome.yaml` (+ `behavior.contract.yaml`) | one agent |
| **Workflow** | `workflow.yaml` / `workflows/*.yaml` | a genome **and** one or more workflows | one agent that runs inside a staged pipeline |
| **System** | `system.yaml` | `system.yaml`, **no** `genome.yaml` | a multi-agent system composed of several agents |

A single bundle may carry several of these at once (e.g. a genome plus the
`workflows/` it participates in, or a `system.yaml` that owns `workflows/`).
Graft every manifest the repository actually warrants — do not invent a system
where there is only one agent.

## Spec versions

Every manifest declares its own `spec_version`; the validator selects the
schema set from it. This skill is version-aware, not version-locked:

| `spec_version` | Adds | Artifacts |
|----------------|------|-----------|
| `0.1` | single-agent core | `genome`, `agent.lock`, `behavior.contract`, traces, replay report, attestation |
| `0.2` | orchestration (RFC 0009) | the above **plus** `workflow`, `system`, and genome orchestration fields (`triggers`, `autonomy`, `guardrails`, `escalation_rules`, `collaboration`) |
| _future `0.3`_ | _new overlay_ | _add one row here; detection and validation already route by `spec_version`, so no other change to this skill is needed_ |

> **Version literals.** `agm validate` (the pinned CLI) is authoritative for
> the exact `spec_version` string and `criticality` values it accepts. Today's
> CLI accepts `spec_version: '0.2'` for every manifest and the published spec
> writes the same versions as `agenomic/v0.2`. The templates shipped with this
> skill use the literals the current CLI accepts. If `agm validate` rejects a
> literal, switch forms — do not fight the validator.

## Preconditions

1. `agm --version` succeeds. If it doesn't, stop and report `status: error`
   with `reason: cli_missing`.
2. `workspace` is a directory you have read+write access to.
3. You have NOT been asked to commit secrets, real customer data, or
   production credentials. If the repository contains `.env` / `*.key` /
   `id_rsa`, you skip them — `agm` excludes them by default (and `validate`
   runs a security scan) but you double-check.

The bundle is written **in place**: `agm init` scaffolds `genome.yaml` (and any
`workflows/`, `system.yaml`) into the directory you point it at, defaulting to
the workspace root. Treat that directory as the **bundle root**; this skill
calls it `bundle_dir` and defaults it to the workspace.

## Phase 1 — Graft

The graft phase turns "code that happens to run an agent" into "an Agenomic
bundle the platform understands". Four steps.

### 1.1 Detect

```sh
cd "$workspace"
agm init --dry-run --format json > .agenomic-detect.json
```

Read `.agenomic-detect.json`. It tells you:

- the inferred `agent_id`, `name`, primary `language`,
- the detected `tools`, `skills`, `knowledge`, `policies`,
- the entrypoint (`execution:` block),
- **orchestration (v0.2):** any detected `workflows`, a multi-agent `system`,
  and required/optional environment variables.

If detection produced an empty capability set or a generic name (`agent`,
`main`), revise it yourself using the repository's README, `prompts/`, and tool
definitions. Do NOT invent capabilities the code cannot perform, members a
system does not have, or steps a workflow does not run.

### 1.2 Scaffold

```sh
agm init --agent-id "$AGENT_ID" --name "$AGENT_NAME"
```

`init` writes, into the bundle root, the artifacts it can infer:

- `genome.yaml` — declarative spec of who you are,
- `agent.lock.yaml` — pinned versions of prompts, tools, models,
- `behavior.contract.yaml` — invariants you promise to hold,
- `workflows/<slug>.yaml` — one per detected workflow (v0.2),
- `system.yaml` — when a multi-agent system is detected (v0.2),
- `prompts/`, `tools/`, `policies/`, `evals/` — populated stubs.

If the bundle already exists, run `agm update` instead (it re-detects, merges,
and — inside a git repo — commits). Never overwrite a hand-edited contract or
orchestration manifest without the user's consent; prefer `agm update` over
`agm init --force`.

The semantic fields static analysis cannot know (domain, criticality,
descriptions, behavior rules, **workflow/system descriptions**) can be filled
by the LLM enrichment pass — `agm enrich`, or `agm init --agent` /
`agm update --agent` to run it inline. It needs a provider API key; without
one it degrades to a hint, so enrichment never blocks the graft.

If `init` could not infer a manifest you know is warranted, hand-write it from
the templates and fill every `<PLACEHOLDER>`:

- agent → [`templates/genome.yaml`](templates/genome.yaml)
- workflow → [`templates/workflow.yaml`](templates/workflow.yaml)
- multi-agent system → [`templates/system.yaml`](templates/system.yaml)
- contract → [`templates/behavior.contract.yaml`](templates/behavior.contract.yaml)

### 1.3 Instrument

You must emit traces while running so `agm replay` can compare you to your
contract. Pick the recipe matching your language:

- Python → [`recipes/python.md`](recipes/python.md)
- TypeScript → [`recipes/typescript.md`](recipes/typescript.md)
- Rust → [`recipes/rust.md`](recipes/rust.md)

Each recipe is a ~5-line wrapper around your existing entry point. **Do not**
restructure the agent's code; instrumentation is additive. For a workflow or
system, instrument each agent that owns a `genome.yaml`; deterministic `tool`
steps are replayed exactly and need no LLM instrumentation.

### 1.4 Validate

```sh
agm validate "$bundle_dir" --level strict
```

`validate` checks the whole bundle in one pass: `genome.yaml`,
`agent.lock.yaml`, `behavior.contract.yaml`, every `workflows/*.yaml` (and a
root `workflow.yaml`), and `system.yaml`, plus cross-references and a security
scan. A bundle root that has `system.yaml` and **no** `genome.yaml` is checked
as a system bundle.

You can also validate any manifest on its own — the kind is inferred from its
top-level `workflow` / `system` / `agent` key:

```sh
agm validate workflows/<slug>.yaml
agm validate system.yaml
```

Beyond JSON-Schema, v0.2 validation enforces the RFC 0009 semantic rules:
unique step ids, resolvable `depends_on` targets, unique member roles, edges
and entrypoints that reference declared roles, and `workflows[].path` entries
that exist and validate. Warnings cover unreachable steps, shadowed
`allowed_actions`, and unused signals.

If `validate` fails, fix the offending manifest (usually a missing required
field, a dangling `depends_on`, or an edge naming an undeclared role) and
re-run. Do NOT proceed to self-evaluation until `validate` is green.

## Phase 2 — Self-evaluate

The self-evaluate phase runs your own behavior against your own declared
contract. It is deterministic where possible and honest about LLM
non-determinism where it isn't.

### 2.1 Run replay

```sh
agm replay "$bundle_dir" "$bundle_dir/evals" \
  --contract "$bundle_dir/behavior.contract.yaml" \
  --format json > .agenomic-replay.json
```

`replay` re-executes each trace under `evals/` against your current
code/prompts/tools and compares the outputs to the expected outcomes. The
positional arguments are the bundle and (optionally) the traces directory;
`--contract` wires in the invariants checked below. Tune `--runs-per-trace`
for statistical (non-deterministic) traces and `--fail-on <severity>` for the
gate.

### 2.2 Run contract checks

`behavior.contract.yaml` lists invariants ("compensation responses always
require human review", "tool calls never leak PII"). `replay` checks these when
the contract is passed with `--contract`. For v0.2 bundles, also confirm the
orchestration envelope holds across the traces:

- **guardrails** declared on the genome / `communication_guardrails` on the
  system are respected by every external message;
- no agent took an action outside its `autonomy.allowed_actions`, and none
  took anything in the system's `forbidden_autonomy` (which overrides every
  member's `allowed_actions`);
- declared `escalation_rules` fire when their `condition` holds.

### 2.3 Diff vs. previous bundle (if any)

If there is a previous bundle (e.g. the last committed version on `main`):

```sh
agm diff <previous-bundle-ref> "$bundle_dir" --format json > .agenomic-diff.json
```

Surface any change in `capabilities`, `tools`, `policies`, or `models` — and,
for v0.2, any change in **workflow steps**, **orchestration topology** (members,
edges, entrypoint, style), **system membership**, `forbidden_autonomy`, or
guardrails. These are the changes that matter for compliance.

## Phase 3 — Report

Emit a single JSON file at `<bundle_dir>/.self-evaluation.json`:

```json
{
  "skill": "self-graft-and-evaluate",
  "skill_version": "0.2.0",
  "agent_id": "<id>",
  "kind": "agent|workflow|system",
  "spec_versions": { "genome": "0.2", "workflows": ["0.2"], "system": "0.2" },
  "phases": {
    "graft":    { "status": "pass|fail", "bundle_path": "<bundle_dir>",
                  "manifests": ["genome.yaml", "workflows/...", "system.yaml"] },
    "evaluate": { "status": "pass|fail", "replay": {...}, "contract": {...},
                  "orchestration": {...}, "diff": {...} }
  },
  "status": "pass|fail|error",
  "summary": "<one-sentence verdict>",
  "next_actions": ["...", "..."]
}
```

Then print a human summary to stdout:

```
self-graft-and-evaluate  (kind: system, spec_version: 0.2)
  graft:    ✅ genome.yaml + workflows/claim-lifecycle.yaml + system.yaml, validate=strict OK
  replay:   ✅ 12/12 traces match expected outcomes
  contract: ⚠️  1/4 invariants flagged (compensation-cites-policy)
  orch:     ✅ no forbidden_autonomy violations; 9 members, 11 edges resolved
  diff:     ℹ️  +1 workflow step, +2 tools vs. previous bundle
  status:   FAIL — fix contract violation before release
```

## Failure handling

- **Detection produced nothing.** The repo has no recognisable agent code.
  Stop with `status: error`, `reason: not_an_agent`.
- **Validate keeps failing after 3 attempts.** Stop, surface the validator
  diagnostics, ask the user to inspect the bundle.
- **Orchestration manifest is structurally broken** (cyclic `depends_on`,
  duplicate step ids or member roles, an edge or entrypoint naming an
  undeclared role, a `workflows[].path` that does not resolve). Fix the
  manifest — these are deterministic schema/semantic errors, never flaky.
- **A system references a member genome that does not validate.** Validate the
  member bundle first; a system is only as graftable as its members.
- **Replay raises non-determinism errors** (different model output across
  runs). Re-run once. If still flaky, mark the trace as `flaky` in the eval
  manifest rather than forcing a pass — Agenomic explicitly compares
  distributions, not absolute truths.

## What this skill does NOT do

- It does not **execute** workflows or systems — it grafts and evaluates their
  declarations. Running them is the orchestration engine's job.
- It does not sign attestations (`agm attest`) — signing is a separate
  human-authorised step.
- It does not push to the Agenomic cloud — graft-and-evaluate is local.
- It does not modify production prompts. Instrumentation is read-only on
  prompt content.

## Scripts

For convenience, two idempotent shell wrappers ship with this skill:

- [`scripts/self-graft.sh`](scripts/self-graft.sh) — phases 1.1 → 1.4
- [`scripts/self-evaluate.sh`](scripts/self-evaluate.sh) — phases 2.1 → 3

An agent can invoke them directly instead of running each `agm` sub-command.
They are spec-version aware (agent / workflow / system) and produce the same
JSON report.
