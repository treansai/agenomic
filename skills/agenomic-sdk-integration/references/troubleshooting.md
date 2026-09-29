# Troubleshooting

Ordered by how often each occurs in real integrations.

## The trace file is empty

The decorator is not on the code path the run actually took.

```sh
grep -rn "trace_agent_run\|traceAgentRun" --include='*.py' --include='*.ts' .
```

Check, in order:

1. **Is the decorated function called?** A decorator on `handle_claim` does
   nothing if the router dispatches to `handle_claim_v2`.
2. **Is an exporter attached?** `@trace_agent_run("id")` with no `exporter`
   records everything and writes nowhere.
3. **Was the exporter closed?** `JsonlExporter` flushes per write by default,
   but `flush_each=False` plus a hard exit loses the tail.
4. **Did the process exit before an async export?** Python `HttpExporter`
   buffers in memory and does not flush on its interval; without
   `await aclose()` (or `close()`) the buffer dies with the process.
5. **TypeScript: did anything write the file?** The TS SDK never writes
   `.agenomic/traces.jsonl` on its own, and `exportTracesToJsonl` overwrites.
   Append each emitted envelope yourself.

## Traces exist but contain no model calls

Python adapters patch the client object in place, so a reference held
before instrumenting is still covered. What escapes them:

```python
create = client.chat.completions.create      # bound before instrumenting: unwrapped
client = instrument_openai(client)
scoped = client.with_options(timeout=5)     # a new client object: not patched
client.responses.create(...)                # OpenAI Responses API: not wrapped
```

Instrument at construction, and wrap any client derived later:

```python
client = instrument_openai(OpenAI())
agent = Agent(client=client)
```

In TypeScript, `instrumentOpenAI` returns a **copy**: an agent holding the
original object records nothing. Pass the returned client everywhere.

Other causes:

- The call happens outside `@trace_agent_run`. Adapters record nothing
  without an active recorder, by design, so imports stay safe.
- A framework constructs its own client (common in CrewAI, some LangChain
  paths). Wrapping yours changes nothing. Fall back to a manual `ModelCall`
  from a framework callback.
- The provider has no adapter (Bedrock, Vertex, Mistral, Ollama). Expected:
  record manually.

## Traces exist but contain no tool calls

**LangGraph: first suspect `instrument_langgraph`.** It records nothing on
current LangGraph releases (see the next section).

**Otherwise, first suspect a thread or worker boundary.**

The Python recorder lives in a `contextvar`. It propagates across `await` and
into `asyncio.create_task`. It does **not** propagate into:

- `threading.Thread`
- `ThreadPoolExecutor` / `ProcessPoolExecutor`
- Celery / RQ / Arq tasks

```sh
grep -rn "ThreadPoolExecutor\|threading.Thread\|\.delay(\|\.apply_async(" --include='*.py' .
```

If tool execution is offloaded to a thread pool, `current_recorder()` returns
`None` inside the worker and every record silently vanishes. This is the
single most common cause of a trace that looks complete and is not.

Fixes: trace the worker as its own run and correlate with a shared label, or
pass the recorder explicitly into the worker.

LangGraph's own executors copy the context, so its parallel branches are not
the cause; a thread the project starts inside a node is.

Other cause: tools genuinely not called on that path.

## `instrument_langgraph` seems to do nothing

It does nothing. The adapter skips every node that is not a plain callable,
and on current LangGraph (verified on 1.2.11) nodes are `RunnableCallable`
before compile and `PregelNode` after, so the graph comes back unchanged
either way. `instrument_langgraph_canonical` has the same gate.

Use `TrackingCallbackHandler` for the tracking stream and record tool calls
explicitly for the envelope: see `recipes/langgraph.md`.

## `agm validate` fails after the integration

Instrumentation changed detectable structure: a new import, a new tool.

```sh
agm update --dry-run    # see what re-detection would change
agm update --no-commit  # re-detect and merge, without committing
agm validate . --level strict
```

Plain `agm update` commits inside a git repo, refuses on `main` / `master` /
`release/*` and on unrelated dirty files, and exits 1 when nothing changed.
Never `agm init --force` over a hand-edited manifest. If validation fails on
a semantic rule (dangling `depends_on`, an edge naming an undeclared role,
a duplicate step id), fix the manifest: these are deterministic, never flaky.

## `agm trace validate` rejects the envelopes

Expected with SDK output today. The CLI checks only `trace_id`, `agent_id`,
`input` and `tool_calls[].name`, and ignores every other field:

| Error | Cause | Fix |
|---|---|---|
| `missing field name` | Python envelope with tool calls (the SDK writes `tool`) | convert with the `jq` line in capability matrix §2 |
| `missing field trace_id` | TypeScript envelope (`{specVersion, run, events}`) | none today; report CLI validation as blocked |
| any other missing field | hand-rolled emitter (Rust, custom) | match the CLI shape exactly |

```sh
head -1 .agenomic/traces.jsonl | jq 'keys, (.tool_calls[0] // {} | keys)'
```

## Ledger: exit 19

`LedgerIntegrityFailed`. Run the report before doing anything else:

```sh
agm ledger verify
```

| Cause | Meaning |
|---|---|
| Signature invalid | Wrong key, or the entry was modified |
| Chain broken | An entry is missing between two links |
| Sequence gap | Events were lost before reaching the ledger |
| Conflicting event id | Same id, different payload, dead-lettered as tampering |
| Key revoked | Flagged, not failed; history stays verifiable |

**The ledger never rewrites history.** Gaps are reported, not repaired. If the
cause is a fresh signing key per process, persist the key: that is the fix,
and re-signing the old entries is not an option.

## Queue backlog / dead letters

```sh
agm ledger queue status
agm ledger queue dead-letter list
agm ledger queue flush
agm ledger queue retry
agm ledger queue dead-letter replay --id <ID>
```

Draining replays pending WAL records idempotently; dead letters are removed
only on successful re-submission.

A growing backlog means the sealer cannot keep up, or the disk budget is
exhausted (`agenomic::ledger::busy`, an explicit refusal, never data loss).
Corrupt segments are quarantined as `.corrupt` and preserved.

## Someone asks for `strict_cloud` or `strict_verified`

`agm` always runs the ledger in the default `durable_low_latency` mode; no
flag, environment variable or config key selects another. Cloud sync is not
implemented, and `strict_cloud` fails closed with
`agenomic::ledger::cloud_unavailable` wherever it can be selected; there is
**no silent downgrade**. Record the requirement as a gap.

## Replay results do not match expectations

Local `agm replay` does not call the model. It runs the contract's
deterministic checks over the recorded traces:

```sh
agm replay . evals/traces.cli.jsonl --contract behavior.contract.yaml
```

It takes one JSONL file in the CLI trace shape (not a directory), and
`--runs-per-trace` > 1 is a warned no-op. A local result is a lower bound over
exactly those traces. Variation across model runs is statistical replay, which
is Agenomic Cloud (`agm cloud push-replay --mode statistical`). If a behavior
stays flaky there, mark it flaky in the eval manifest rather than forcing a
pass. A forced pass is a false negative in every future run.

Genuine non-replayability comes from unrecorded dependencies: live tool
results, wall-clock time, random seeds, external state. Record tool outputs in
the trace, or supply fixtures.

## The agent got slower

Measure before blaming Agenomic. Expected overhead is sub-millisecond per
event: BLAKE3 over canonical CBOR, plus an append.

Real causes, in order of likelihood:

1. `JsonlExporter(flush_each=True)` on a high-volume path: a file flush per
   run (not an fsync).
2. A synchronous HTTP call on the hot path: a hand-rolled POST, a cloud-mode
   Python `Client` call (tracking and monitor events are one request each),
   or TypeScript `traceAgentRun` with an `endpoint` (one awaited POST per run).
3. Large payloads captured inline. Use `capture_input=False` /
   `capture_output=False`, or redact.

## The agent crashes when Agenomic is unreachable

**This is a bug in the integration**, not expected behavior, but the SDKs
make it easy:

- Python exporters log and swallow; Python cloud-mode `Client` calls raise
  `CloudError`.
- TypeScript `traceAgentRun` / `withTracedRoute` with an `endpoint` reject the
  agent call with `Trace has already been finalized.` when ingestion fails;
  tracking and RMP cloud calls throw.

```sh
grep -rn "upload_traces\|\.export(\|await .*emit()\|tracking\.\|\.rmp\.\|\.monitor\.\|endpoint:" --include='*.py' --include='*.ts' .
```

Every call into Agenomic on the hot path must be inside a `try`, or delegated
to an exporter that already handles it. `scripts/smoke-run.sh` tests exactly
this when given the project's Agenomic URL variables
(`AGENOMIC_SMOKE_ENDPOINT_VARS`): it
points that variable at `127.0.0.1:1` and requires the agent to complete.
`AGENOMIC_ENDPOINT` alone proves nothing, since only `agm` reads it.

## `agm doctor` reports problems

Run it first for any environment issue:

```sh
agm doctor
```

It checks the machine, not the workspace: CLI version, platform, embedded
schemas, config file path, credentials mode, ATEP support, BLAKE3 sanity, the
temp dir, and cloud health for cloud profiles. It has no key or ledger store
check (use `agm ledger status` and `agm ledger verify`). Output is always
JSON; it exits 3 when any check fails.

## Nothing here matches

1. Re-run `scripts/inspect-target.sh`: the architecture map may be wrong.
2. Re-read `references/capability-matrix.md`: the capability may not exist.
3. Check `agenomic-cli/docs/BACKEND_GAPS.md` for a known gap.
4. Report the gap rather than working around it silently. A documented `❌` is
   more useful than an undocumented workaround.
