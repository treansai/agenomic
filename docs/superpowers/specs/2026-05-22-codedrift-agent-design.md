# Design — `agenomic-codedrift`: an E2E test agent for the Agenomic platform

**Status:** approved 2026-05-22
**Owner:** gabin.mberikongo@traidano.com
**Target submodule:** `agenomic-codedrift`

## 1. Purpose

Build a Python LangGraph agent whose only job is to exercise the full
Agenomic platform end-to-end:

1. The agent is itself a versioned, signed Agenomic bundle pushed to the
   cloud registry.
2. Each scheduled run measures the code-quality drift of a single Claude
   model on a fixed micro-benchmark and emits a signed `TraceEnvelope`
   to the cloud.
3. The cloud links the trace to the bundle release so the release page
   shows both the manifest/attestation and a running trace history.

The agent's "mission" — drift detection — is real enough to be
non-trivial, but the primary value of the project is as an executable
smoke test of the Agenomic registry + trace + attestation surfaces.

### Out of scope

- Production-grade drift alerting (Slack, email).
- Multi-model comparison (a single Claude model per run; varying the
  model is a follow-up if useful).
- Self-looping LangGraph daemon. The continuous aspect is delegated to
  a GitHub Actions cron — the graph runs single-shot.
- Human-vs-AI commit auditing.

## 2. Architecture

```
┌────────────────── single run (LangGraph) ──────────────────┐
│ load_benchmark → prompt_claude → score_quality →          │
│        compare_baseline → emit_trace                       │
└────────────────────────────────────────────────────────────┘
                            │
                            ▼ (decorated with @trace_agent_run)
                  TraceEnvelope (signed, ATEP-compatible)
                            │
                            ▼
              JsonlExporter → AgenomicClient.upload_traces
```

External GitHub Actions cron fires every 6 hours; each fire executes
one `python -m agenomic_codedrift run`.

## 3. Repository layout

```
agenomic-codedrift/
├── README.md
├── pyproject.toml             # deps: agenomic[langgraph,anthropic], ruff, radon, bandit
├── agenomic.yaml              # bundle manifest
├── Makefile                   # build, test, e2e, bundle-push
├── benchmarks/                # the 5 fixed snippets the agent prompts about
│   ├── fizzbuzz.py
│   ├── two_sum.py
│   ├── parse_csv.py
│   ├── memo_fib.py
│   └── http_client.py
├── src/agenomic_codedrift/
│   ├── __init__.py
│   ├── __main__.py            # CLI entry: `python -m agenomic_codedrift run`
│   ├── graph.py               # LangGraph StateGraph wiring
│   ├── nodes.py               # node implementations
│   ├── metrics.py             # ruff/radon/bandit subprocess wrappers
│   ├── baseline.py            # JSONL persistence + drift math
│   └── claude.py              # Anthropic SDK thin wrapper
├── tests/
│   ├── test_metrics.py        # unit: each metric on known inputs
│   ├── test_baseline.py       # unit: drift math on synthetic series
│   └── test_graph_smoke.py    # integration: graph with mocked Claude
└── .github/workflows/
    ├── ci.yml                 # pytest + ruff + mypy on push/PR
    ├── release.yml            # on tag: build bundle, sign, push to Agenomic
    └── drift.yml              # cron */6h: run agent against prod cloud
```

## 4. Per-run data flow

| Node              | Input                                       | Output                                                                 |
|-------------------|---------------------------------------------|------------------------------------------------------------------------|
| `load_benchmark`  | `benchmarks/*.py` on disk                   | `[{name, source}]` — the 5 snippets                                    |
| `prompt_claude`   | the snippets                                | `[{name, source, response}]` — Claude's refactor of each               |
| `score_quality`   | the responses                               | `[{name, lint_issues, complexity, security_warnings, error?}]`         |
| `compare_baseline`| current scores + `baseline.jsonl` (last 30) | `{per_metric: {mean, current, delta, drift_flag}, snippet_flags: [...]}` |
| `emit_trace`      | scores + comparison + run metadata          | side-effect: append to baseline + upload via `AgenomicClient`          |

The prompt template (frozen):

> *"Refactor the following Python function for clarity and add complete
> type hints. Return only valid Python code, no commentary."*

The whole run is wrapped in `@trace_agent_run(agent_id="agent://traidano/codedrift", exporter=exporter)`,
so the LangGraph execution produces one `TraceEnvelope` per run.

## 5. Agenomic bundle integration

`agenomic.yaml`:

```yaml
name: codedrift-agent
version: 0.1.0
description: E2E test agent — measures Claude code-quality drift over a fixed benchmark
authors: [traidano]
runtime: python
entrypoint: agenomic_codedrift.__main__:main
inputs:
  schema: { snippets: list, model: string }
outputs:
  schema: { drift_flags: list, per_metric: object }
signing:
  algorithm: ed25519
  key: keys/codedrift.priv  # gitignored
```

**Release flow** (`release.yml`, on git tag `v*`):

1. `agm bundle build` → tarball.
2. `agm bundle sign --key keys/codedrift.priv` → ed25519 signature.
3. `agm bundle push --org $AGENOMIC_ORG_ID` → `POST /v1/bundles`. Cloud
   returns `release_id` and an attestation record.
4. `release_id` is exported as a workflow output for downstream jobs.

**Trace ↔ release linkage**: each run reads `release_id` from
`AGENOMIC_RELEASE_ID` env (set by `release.yml` or by the human running
locally) and embeds it in the `TraceEnvelope` metadata, so the cloud
can join traces back to the release on the registry page.

## 6. Continuous trigger

`drift.yml` (GitHub Actions):

```yaml
on:
  schedule:
    - cron: '0 */6 * * *'
  workflow_dispatch:
jobs:
  drift:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v5
        with: { python-version: '3.12' }
      - run: pip install -e .
      - run: python -m agenomic_codedrift run
        env:
          ANTHROPIC_API_KEY:    ${{ secrets.ANTHROPIC_API_KEY }}
          AGENOMIC_API_KEY:     ${{ secrets.AGENOMIC_API_KEY }}
          AGENOMIC_ENDPOINT:    ${{ secrets.AGENOMIC_ENDPOINT }}
          AGENOMIC_RELEASE_ID:  ${{ secrets.AGENOMIC_RELEASE_ID }}
```

## 7. Error handling

- **Claude 429 / timeout**: exponential backoff (1s, 4s, 16s), max 3
  retries per snippet. After 3 failures the snippet is marked
  `error: true` and the run continues with the remaining ones.
- **Missing tool on PATH** (ruff, radon, bandit): fail fast at process
  start with an explicit message — never mid-run.
- **First run, no baseline**: `compare_baseline` returns `drift: null`
  and no flags. The current scores become the first baseline entry.
- **Cloud upload fails**: log error, persist the `TraceEnvelope` to a
  local `traces/$RUN_ID.jsonl`, exit code 1 so the GHA job is flagged.
  Local scoring + baseline persistence still complete before exit.
- **Anthropic SDK auth failure**: fail fast — no point scoring nothing.

## 8. Testing

- **Unit `test_metrics.py`**: each of `ruff`, `radon`, `bandit` wrappers
  on a known-good and known-bad snippet, asserting the shape and
  numeric range of the returned scores.
- **Unit `test_baseline.py`**: drift math (mean, percentile-based
  threshold, no-baseline path) on synthetic numeric series.
- **Integration `test_graph_smoke.py`**: full graph end-to-end with
  Claude mocked via `anthropic.Anthropic` patched to return canned
  responses. Asserts a `TraceEnvelope` is produced, asserts the
  envelope contains the expected node spans and the expected scores.
- **No live LLM in CI**: Claude is always mocked in `pytest`.
- **Manual E2E**: `make e2e` runs against real Claude + real Agenomic
  cloud on **one** throwaway snippet to keep cost bounded.

## 9. Open questions

None blocking. Two minor follow-ups can be deferred to the
implementation phase:

- The exact percentile / threshold for "drift flag" (current draft:
  `|delta| > 1 stddev` of the rolling 30-run mean — to be confirmed
  during `test_baseline.py` authoring).
- Whether `bandit` warnings should weigh the same as `ruff` lint issues
  in a composite score, or stay reported separately. Default: separate.
