<!--
Template for docs/agenomic-integration.md in the TARGET repository.
If the file already exists, EXTEND it; do not duplicate sections.
Replace every <PLACEHOLDER>. Delete sections that do not apply; an empty
section reads as an unfinished integration.
-->

# Agenomic integration

How this project reports its agent behavior to Agenomic, and what to do when
that reporting misbehaves.

## What this gives us

- **Replay**: check recorded traces against the behavior contract, offline.
- **Signed history**: a tamper-evident record of what the agent did.
- **Drift and loop detection**: notice behavior changing before users do.
- **Evidence**: an offline-verifiable package for audit.

It does not change what the agent does. Instrumentation is additive, and a
telemetry failure never fails a request.

## Architecture

```text
<PASTE THE ARCHITECTURE MAP FROM THE INTEGRATION PLAN>
```

| Boundary | Where | How |
|---|---|---|
| Run | `<file:line>` | `<decorator/wrapper>` |
| Model calls | `<file:line>` | `<adapter>` |
| Tool calls | `<file:line>` | `<adapter or manual>` |
| Memory | `<file:line>` | `<or: tracking only / not instrumented, and why>` |

## Configuration

| Variable | Required | Purpose |
|---|:--:|---|
| `AGENOMIC_API_KEY` | cloud only | Cloud authentication |
| `AGENOMIC_ENDPOINT` | no | Defaults to `https://api.agenomic.io` |
| `<MYAPP>_AGENOMIC_API_URL` | no | SDK URL, read by our code; unset = local-only |

Everything works offline. Cloud is optional, and the agent must never depend
on it being reachable.

Secrets live in `<SECRET STORE>`. Ed25519 signing keys live at
`~/.config/agenomic/keys` (mode 0600) and are managed with `agm ledger keys`.
**Never commit a private key.**

## Local development

```sh
<INSTALL COMMAND>
<RUN COMMAND>

<COUNT CALLS: the jq lines from the language recipe>
<PYTHON ONLY: convert, then `agm trace validate .agenomic/traces.cli.jsonl`>
```

Traces land in `.agenomic/traces.jsonl`. Add `.agenomic/` to `.gitignore`
except for fixtures you deliberately commit under `evals/`.

## Replay

```sh
agm replay . evals/traces.cli.jsonl --contract behavior.contract.yaml
```

Local replay does not call the model: it runs the contract's deterministic
checks over exactly these recorded traces, a lower bound. Statistical replay
across model variation is **<used via Agenomic Cloud | not used>** because
<REASON>. A flaky behavior should be marked flaky, not forced to pass.

Add a fixture by capturing a real run, converting it to the CLI trace shape
and appending it to `evals/traces.cli.jsonl`. Redact before committing:
fixtures are source code.

## Monitoring

```sh
agm track start . --ledger --agent <AGENT_ID>          # prints the session id
agm track status --session <SID>
agm track report --session <SID> --include-ledger-proof --output report.json
agm track stop   --session <SID>
```

Detectors enabled: <drift | loops | intent | failures>.
Alerts route to <CHANNEL>; the owner is <TEAM>.

## Ledger

```sh
agm ledger init
agm ledger status                    # entries, chain head, WAL health, dead letters
agm ledger verify                    # full offline verification (exit 19 on failure)
agm ledger seal                      # seal a signed Merkle block
```

Mode: `durable_low_latency`, the only mode `agm` runs: the hot path returns
after an fsync'd WAL append. Payloads are hash-committed, never stored raw.

Two guarantees: no event loss (every event reaches a durable state or an
explicit dead-letter), and the agent is never blocked.

## Evidence

```sh
agm evidence export --include-ledger --run <RUN_ID> --output evidence/
agm evidence verify evidence/
```

Locally-signed bundles are technical integrity evidence with **non-probative
status**. Probative, org-attested packs are a hosted-service concern: do not
present a local bundle as a legal artifact.

## Existing observability

<OTel / LangSmith / Langfuse / Datadog> remains in place and unchanged.
Agenomic runs alongside it; the <SYSTEM> trace id is copied into the label
`<LABEL>` so you can pivot between the two.

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| Empty trace file | Decorator not on the executed path | Check the entrypoint |
| No model calls | A bound method or derived client escaped the wrapper, or (TypeScript) the original client is used instead of the instrumented copy | Instrument where the client is built and pass that object everywhere |
| No tool calls | `instrument_langgraph` relied on, or a thread the project starts dropped the context | Record tool calls explicitly; see `references/architecture.md` |
| Traces stop under load | Export queue saturated | `agm ledger queue status`, then `flush` |
| `agm validate` fails | Manifest drifted from code | `agm update --dry-run`, then `agm update --no-commit` |
| `agm trace validate` fails | SDK trace shape (Python `tool`, TypeScript envelope) | Convert Python traces; TypeScript is not readable by the CLI today |
| Ledger verify exit 19 | Integrity failure | `agm ledger verify` report; do not rewrite history |

## What is not covered

<LIST FROM THE INTEGRATION REPORT, with reasons. Say what a reader cannot
rely on. An honest gap list is what makes the rest of this document
trustworthy.>

## Runbook: telemetry is failing

1. `agm doctor` (machine health)
2. `agm ledger status` and `agm ledger queue status`
3. `agm ledger queue dead-letter list`
4. `agm ledger queue flush` / `retry`

The agent keeps serving throughout. Telemetry degradation is not an incident
for users, but a silent one is an incident for audit, so page on backlog
growth, not on individual failures.
