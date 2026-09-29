# LangGraph recipe

LangGraph has topology detection in the CLI and one working runtime path in
the Python SDK: the LangChain `TrackingCallbackHandler`. Python only.

**Do not rely on `instrument_langgraph`.** On current LangGraph releases
(verified on 1.2.11) every node is a `RunnableCallable` or `PregelNode`,
which the adapter skips as not callable: the graph comes back unchanged and
records no tool call, before or after `.compile()`.
`instrument_langgraph_canonical` has the same gate. Its tests use fake nodes,
so nothing in the SDK catches this. Treat it as a known SDK defect and record
it in the report if the project already calls it.

## 1. Topology → manifest

```sh
agm init --dry-run --format json
```

`agenomic-detect` reads LangGraph builders directly and recovers `add_node`,
`add_edge`, `add_conditional_edges`, `set_entry_point` and `START` / `END`
into `workflows/<slug>.yaml`: step ids,
dependencies and conditional transitions. When several graphs hand off to
each other it also synthesizes `system.yaml` with members, roles and edges.

Review the recovered topology against the code. Static analysis follows
builder calls; a graph assembled dynamically in a loop or behind a factory
will be partially recovered. Fix the manifest rather than the detector, and
note in the report that the topology was hand-corrected.

## 2. Runtime instrumentation: tracking handler

```python
from agenomic import Client
from agenomic.integrations.langchain import TrackingCallbackHandler, dropped_events, flush

client = Client()                                  # or Client(api_key=..., base_url=...)
session = client.tracking.start(agent="agent://acme/claims")

result = await app.ainvoke(state, config={"callbacks": [TrackingCallbackHandler(session)]})
# app.invoke(...) works the same way

if not flush(session):
    log.warning("agenomic dropped %d tracking events", dropped_events(session))
session.stop()                                     # drains the worker, then closes
```

Build one handler per request and share the session. The handler maps the
LangChain run tree onto tracking events:

| LangChain run | Tracking events |
|---|---|
| root chain | `turn.started` / `.completed` / `.failed` (with `turn_id`) |
| node (`graph:step:N` tag) | `agent.step.*` |
| chat model / LLM | `model.call.*` with token usage |
| tool | `tool.call.*` |
| retriever | `retrieval.*` |

Other chains are silent and their children are re-parented, so the
hierarchy matches the graph. Only hashes leave the process (plain BLAKE3 hex,
no `blake3:` prefix); an error sends the exception class name, not its
message. `capture_turn_title=True` is the one opt-in exception: it sends the
first 120 characters of the root run's last message.

Events go through one background worker per session: a slow or failing
gateway never blocks the run and never raises into it, which also means
delivery is not guaranteed. `flush(session, timeout=5.0)` returns `False` on
timeout or on any drop; read `dropped_events(session)` before `stop()`.

This writes the **tracking stream**, not the trace envelope. For the envelope
(replay fixtures, ATEP), add section 3.

## 3. Run boundary and node tool calls (trace envelope)

The graph invocation is the run, not the node:

```python
from anthropic import Anthropic
from agenomic.trace.decorator import trace_agent_run
from agenomic.exporters.jsonl import JsonlExporter
from agenomic.integrations import instrument_anthropic

exporter = JsonlExporter(".agenomic/traces.jsonl")
llm = instrument_anthropic(Anthropic())
app = build_graph().compile()

@trace_agent_run("agent://acme/claims", exporter=exporter)
def handle_claim(payload: dict) -> dict:
    return app.invoke({"claim": payload})
```

The LLM client records model calls. Node and tool executions must be recorded
explicitly: decorate the functions nodes call to reach the outside world with
the `traced_tool` decorator from [`python.md`](python.md#3-tool-calls), or
record a `ToolCall` inside the node. `current_recorder()` is visible inside
nodes, including in LangGraph's own thread pool (verified on 1.2.11).

## 4. What the trace path does not cover

| Feature | Handling |
|---|---|
| Conditional edges | Topology only: in `workflows/<slug>.yaml`, not in the trace. Infer the taken path from the sequence of node tool calls. |
| State mutations | Captured as input/output hashes per node, not as a diff. |
| Interrupts / human-in-the-loop | Not recorded. Record an explicit `ToolCall` with `requires_human_approval=True` and `approval_present`. |
| Checkpoints | Not recorded. If the checkpointer is the agent's memory, record memory access explicitly. |
| Streaming (`.stream()`) | The decorator records on completion. Wrap the function that drains the stream. The tracking handler follows streamed runs natively. |
| Subgraphs | Nodes of the parent graph. To attribute them separately, instrument the subgraph and give it its own `agent_id` and run. |

## 5. Async

Use `instrument_anthropic_async` / `instrument_openai_async` with `ainvoke`,
and put `@trace_agent_run` on an `async def`. `contextvars` propagate across
`await` and into `asyncio.create_task`, so async node execution is safe.

LangGraph copies the context into its own executors: sync nodes under
`ainvoke` and parallel branches both see the recorder. A `threading.Thread`
or `ThreadPoolExecutor` that the project creates itself inside a node does
not; see `references/troubleshooting.md`.

## 6. Verify

```sh
agm validate . --level strict          # includes workflows/*.yaml
jq -s '[.[].tool_calls // [] | length] | add' .agenomic/traces.jsonl
```

Tool-call count should equal the number of recorded node and tool
executions. Zero usually means the project relied on `instrument_langgraph`.
For the tracking stream, check `session.events` (local mode) for one
`agent.step.started` per executed node.

## 7. Replay

LangGraph agents are replayable when tool results are deterministic or
recorded. Capture traces into `evals/`, converted to the CLI trace shape (capability
matrix §2), then:

```sh
agm replay . evals/traces.cli.jsonl --contract behavior.contract.yaml
```

`agm replay` takes one JSONL file, not a directory, and needs
`behavior.contract.yaml` in the bundle or `--contract`. It runs the
contract's deterministic checks over the recorded traces, offline, without
calling the model: a lower bound, not a statistical verdict.
`--runs-per-trace` > 1 is a warned no-op; statistical replay across model
variation is `agm cloud push-replay --mode statistical`.
