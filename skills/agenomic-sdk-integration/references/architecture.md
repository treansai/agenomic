# Architecture: where Agenomic attaches

## The layering

Agenomic is three layers with a one-way data flow. Confusing them is the most
common integration error.

```
┌─ your agent ──────────────────────────────────────────────┐
│  entrypoint → loop → model calls → tool calls → memory    │
└───────────────────────────┬───────────────────────────────┘
                            │  SDK: records a TraceEnvelope
                            ▼
┌─ trace layer (SDK) ───────────────────────────────────────┐
│  TraceRecorder → TraceEnvelope → Exporter                 │
│  JSONL · ATEP local (signed) · HTTP (cloud) · Multi       │
└───────────────────────────┬───────────────────────────────┘
                            │  files / uploads
                            ▼
┌─ proof + analysis layer (CLI) ────────────────────────────┐
│  validate · replay · diff · attest                        │
│  ledger (WAL → signed chain → Merkle blocks)              │
│  track (drift · loop · intent) · rmp · governance         │
│  evidence export / verify                                 │
└───────────────────────────────────────────────────────────┘
```

The SDK never reaches into the ledger or the proof engines. It writes traces;
the CLI reads traces and produces proofs. Any design where "the SDK appends
to the ledger" is not implementable today.

Both SDKs also have a `Client` that talks to Agenomic Cloud directly for
tracking, RMP sessions, Protect, tool execution and benchmarks. That is a
second, optional channel next to the trace layer, and it has different
failure behavior: see "Production topology" below.

## The four boundaries

Instrument these. Nothing else.

### 1. Run: one agent execution

The unit a human would call "one thing the agent did". Usually the HTTP
handler, the queue consumer, or the CLI `main`.

**Right:** the function that receives a user request and returns the answer.
**Wrong:** `main()` for a long-lived server (one run per *process* makes every
trace unreplayable), or a helper called 400 times per request.

Test: could you replay this run in isolation from its recorded input? If not,
the boundary is wrong.

### 2. Model call: every LLM invocation

At **client construction**, not at each call site. `instrument_openai(client)`
once, at the place the client is built, covers every call through it. Chasing
individual call sites guarantees you miss the retry path.

### 3. Tool call: every effect on the outside world

Anything that reads or changes state beyond the process: HTTP, database, file
system, MCP, another agent. Mark irreversible effects: they select the
`agm gate check` path.

Not a tool call: pure functions, formatting, parsing.

### 4. Memory access: persistent agent state

Reads and writes of state that survives the run: vector stores, conversation
stores, scratchpads. TypeScript records it in the trace (`addMemoryAccess`).
Python records it only in the tracking stream (`memory_write`, `memory.read`)
or on a canonical run (`log_memory`); the Python trace envelope has no memory
field, so an envelope-only Python integration reports it as a gap.

### The fifth, conditional: workflow step

When the runtime has an explicit graph (LangGraph, Temporal, a state machine),
each node execution is a step. `agm init` recovers the topology into
`workflows/<slug>.yaml`; at runtime, a tracking session emits the tracking
events `agent.step.started` / `agent.step.completed` (`session.step()`, or
the LangChain handler for LangGraph).
For free-form loops there are no steps: the loop iteration is the unit, and
loop detection is what catches it running away.

## What not to instrument

- individual helper functions: noise, not behavior;
- prompt template rendering: the prompt hash already covers it;
- retries *inside* a provider SDK: the adapter records the outer call;
- every graph edge: nodes are the behavior; edges are topology, and topology
  belongs in the manifest, not the trace.

The rule: **instrument what changes the agent's trajectory or touches the
world.** If removing the instrumentation would not lose an auditable fact,
it should not be there.

## Where the run boundary usually lives

| Shape | Boundary | Trap |
|---|---|---|
| HTTP API | route handler | Don't wrap the middleware: you'll trace health checks |
| Queue consumer | per-message handler | One run per message, not per poll batch |
| CLI tool | `main` after arg parsing | Fine here: the process *is* one run |
| Long-running server | request handler | Never `main` |
| Scheduled job | the job function | One run per execution |
| Streaming | the handler that produces the full response | Record on completion, not per chunk |

## Async and context propagation

The Python SDK propagates the recorder through a `contextvar`. That works
across `await` inside the same task. It does **not** cross:

- `threading.Thread`: the child gets a fresh context;
- `ProcessPoolExecutor`: separate process entirely;
- a job pushed to Celery/RQ/Arq: a different process later.

If the agent fans out to threads or workers, either wrap each worker as its
own run and correlate with a shared label, or pass the recorder explicitly.
Silently losing tool calls to a thread boundary is the most common cause of a
trace that looks complete and is not; `scripts/validate-integration.sh`
checks call-site count against trace content precisely to catch it.

`contextvars` *are* copied into `asyncio.create_task`, so async fan-out within
one event loop is safe. LangGraph copies the context into its own executors
as well, so its parallel branches and sync-nodes-under-`ainvoke` keep the
recorder; threads the project creates itself inside a node do not.

## Multi-agent systems

Each agent keeps its own identity and its own run. Link them:

- give every agent its own `agent_id` (`agent://<org>/<slug>`);
- record the parent run id as a label on child runs;
- declare the topology in `system.yaml` (members, roles, edges, entrypoint).
  `agm init` synthesizes one when it finds two or more LangGraph graphs, or a
  graph plus Temporal; it does not analyze handoffs, so review the edges;
- record handoffs as tool calls on the *sending* side.

Do not collapse a multi-agent system into one run. You lose per-agent
attribution, and a per-agent bundle (genome, contract, `agm diff`) no longer
maps to what ran.

## Production topology

```
agent process
  ├─ JsonlExporter        → local disk, always
  ├─ AtepLocalExporter    → signed ATEP store (Python)
  └─ HttpExporter         → Agenomic Cloud (Python: flushes on batch size or
                            aclose() only; TypeScript: one awaited POST per run)

sidecar / cron
  ├─ agm ledger init            once
  ├─ agm track start --ledger   binds the session
  ├─ agm ledger seal            periodic Merkle blocks
  └─ agm evidence export        on demand
```

The hot path writes locally and returns. Everything expensive (sealing,
verification, replay, governance) is out of band. That is the design
guarantee, and an integration that blocks the agent on a network call has
broken it.

The SDKs do not enforce this guarantee on their own. In cloud mode, Python
`Client` calls (tracking events, monitor events, RMP, tools, protect) are
synchronous HTTP requests that raise `CloudError`, and TypeScript
`trace.emit()`, tracking and RMP calls throw on HTTP errors, which
`traceAgentRun` turns into a rejected agent call. Keep the agent's client
local, or guard every cloud call (see the language recipes). The Python
LangChain `TrackingCallbackHandler` is the one surface that already offloads
to a background worker.
