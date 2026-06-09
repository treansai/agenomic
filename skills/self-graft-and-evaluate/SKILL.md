---
name: self-graft-and-evaluate
description: >
  Procedure an AI agent follows to register itself as an Agenomic agent-bundle
  (self-graft) and run its own deterministic evaluation (self-evaluate).
inputs: [workspace, language?, agent_name?]
outputs: [bundle_path, report, status]
---

# Self-graft and self-evaluate

You are an AI agent. The user's repository contains your runtime code (the
"app"). Your task is to **graft yourself** onto the Agenomic platform — produce
the artifacts that make your behavior portable, replayable, and auditable —
and then **evaluate yourself** against the contract you just declared.

This procedure is runtime-agnostic. It works for Claude, GPT, local LLMs, or
any orchestrator. The only hard dependency is the `agenomic` CLI.

## Preconditions

1. `agenomic --version` succeeds. If it doesn't, stop and report
   `status: error` with `reason: cli_missing`.
2. `workspace` is a directory you have read+write access to.
3. You have NOT been asked to commit secrets, real customer data, or
   production credentials. If the repository contains `.env` / `*.key` /
   `id_rsa`, you skip them — `agenomic` excludes them by default but you
   double-check.

## Phase 1 — Graft

The graft phase turns "code that happens to run an agent" into "an Agenomic
agent-bundle the platform understands". Three steps:

### 1.1 Detect

```sh
cd "$workspace"
agenomic init --dry-run --format json > .agenomic-detect.json
```

Read `.agenomic-detect.json`. It tells you:

- the inferred `agent_id`, `name`, primary `language`,
- the detected `inputs`, `outputs`, `capabilities`,
- the entrypoint (`execution:` block).

If detection produced an empty `capabilities` list or a generic name (`agent`,
`main`), revise them yourself using the repository's README, prompts/, and
tool definitions. Do NOT invent capabilities the code cannot perform.

### 1.2 Scaffold

```sh
agenomic init --agent-id "$AGENT_ID" --name "$AGENT_NAME"
```

This creates `agent-bundle/` with:

- `genome.yaml` — declarative spec of who you are,
- `agent.lock.yaml` — pinned versions of prompts, tools, models,
- `behavior.contract.yaml` — invariants you promise to hold,
- `prompts/`, `tools/`, `policies/`, `evals/` — populated stubs.

If `agent-bundle/` already exists, run `agenomic update` instead. Never
overwrite a hand-edited contract without the user's consent.

### 1.3 Instrument

You must emit traces while running so `agenomic replay` can compare you to
your contract. Pick the recipe matching your language:

- Python → [`recipes/python.md`](recipes/python.md)
- TypeScript → [`recipes/typescript.md`](recipes/typescript.md)
- Rust → [`recipes/rust.md`](recipes/rust.md)

Each recipe is a 5-line wrapper around your existing entry point. **Do not**
restructure the agent's code; instrumentation is additive.

### 1.4 Validate

```sh
agenomic validate --level strict
```

If `validate` fails, fix the bundle (usually a missing field in `genome.yaml`
or a tool referenced but absent from `tools/`) and re-run. Do NOT proceed to
self-evaluation until `validate` is green.

## Phase 2 — Self-evaluate

The self-evaluate phase runs your own behavior against your own declared
contract. It is deterministic where possible and honest about LLM
non-determinism where it isn't.

### 2.1 Run replay

```sh
agenomic replay --bundle agent-bundle --traces agent-bundle/evals --format json \
  > .agenomic-replay.json
```

`replay` re-executes each trace in `agent-bundle/evals/` against your current
code/prompts/tools and compares the outputs to the expected outcomes.

### 2.2 Run contract checks

`behavior.contract.yaml` lists invariants ("compensation responses always
require human review", "tool calls never leak PII"). `replay` checks these
automatically when the contract is wired into the eval manifest.

### 2.3 Diff vs. previous bundle (if any)

If there is a previous bundle (e.g. last committed version on `main`):

```sh
agenomic diff <previous-bundle-ref> agent-bundle --format json \
  > .agenomic-diff.json
```

Surface any change in `capabilities`, `tools`, `policies`, or `models`. These
are the changes that matter for compliance.

## Phase 3 — Report

Emit a single JSON file at `agent-bundle/.self-evaluation.json`:

```json
{
  "skill": "self-graft-and-evaluate",
  "skill_version": "0.1.0",
  "agent_id": "<id>",
  "phases": {
    "graft":    { "status": "pass|fail", "bundle_path": "agent-bundle" },
    "evaluate": { "status": "pass|fail", "replay": {...}, "contract": {...}, "diff": {...} }
  },
  "status": "pass|fail|error",
  "summary": "<one-sentence verdict>",
  "next_actions": ["...", "..."]
}
```

Then print a human summary to stdout:

```
self-graft-and-evaluate
  graft:    ✅ agent-bundle/ created, validate=strict OK
  replay:   ✅ 12/12 traces match expected outcomes
  contract: ⚠️  1/4 invariants flagged (compensation-cites-policy)
  diff:     ℹ️  +1 capability, +2 tools vs. previous bundle
  status:   FAIL — fix contract violation before release
```

## Failure handling

- **Detection produced nothing.** The repo has no recognisable agent code.
  Stop with `status: error`, `reason: not_an_agent`.
- **Validate keeps failing after 3 attempts.** Stop, surface the validator
  diagnostics, ask the user to inspect the bundle.
- **Replay raises non-determinism errors** (different model output across
  runs). Re-run once. If still flaky, mark the trace as `flaky` in the eval
  manifest rather than forcing a pass — Agenomic explicitly compares
  distributions, not absolute truths.

## What this skill does NOT do

- It does not sign attestations (`agenomic attest`) — signing is a separate
  human-authorised step.
- It does not push to the Agenomic cloud — graft-and-evaluate is local.
- It does not modify production prompts. Instrumentation is read-only on
  prompt content.

## Scripts

For convenience, two idempotent shell wrappers ship with this skill:

- [`scripts/self-graft.sh`](scripts/self-graft.sh) — phases 1.1 → 1.4
- [`scripts/self-evaluate.sh`](scripts/self-evaluate.sh) — phases 2.1 → 3

An agent can invoke them directly instead of running each `agenomic`
sub-command. They produce the same JSON report.
