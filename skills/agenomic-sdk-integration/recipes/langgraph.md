# LangGraph recipe

LangGraph has topology detection in the CLI and one working tracing path in
the Python SDK: the LangChain `TrackingCallbackHandler`. Python only.

Managed prompts (section 8) pin the prompts of each thread of a graph to one
release with `bind_langgraph`. That adapter pins prompts and records nothing by
itself, and it is not in a released SDK yet.

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

## 8. Managed prompts

**Unreleased.** Verified on 2026-10-05 against the `agenomic-python` branch
`feat/managed-prompts` (c3616ef); none of it is in `agenomic` v0.1.3. Plan it
only when the project can pin that branch, and record it as unreleased in the
report. Python only: the TypeScript SDK and `agm` have no managed prompt
surface (capability matrix §11).

The path from prompts in code to pinned prompts:

1. `agenomic-py prompts scan`: static discovery, on the developer machine.
2. `agenomic-py prompts import`: prompts and slot declarations in Agenomic
   Cloud, with a `write` key.
3. A candidate release pins each slot to one prompt version; a signed-in
   person approves and promotes it in Agenomic Cloud.
4. `bind_langgraph` at runtime, with a `read` key: every thread runs on the
   release it was first bound to.

### 8.1 Scan

```sh
agenomic-py prompts scan app/ --out prompt-report.json
```

- The scanner parses the files with `ast`. It never imports, runs or rewrites
  the scanned code. It writes an `agenomic.prompt_discovery_report/v1`
  document and prints a count per status on stderr.
- It recognizes module constants whose name ends in `prompt`, `template`,
  `instruction(s)` or `system_message`, LangChain `PromptTemplate` and
  `ChatPromptTemplate` (with `MessagesPlaceholder`),
  `create_react_agent(prompt=...)`, and the runtime forms that feed them
  (f-strings, `.format`, `hub.pull`, call results).
- Each candidate has a `status`:
  - `supported`: the content converts to a managed version;
  - `unsupported`: a recognized construct with a refused feature (mustache,
    jinja2, format specs, callable partials);
  - `unresolved`: built at runtime, so no content; map it by hand or keep it
    in code;
  - `blocked_secret`: it matched a secret pattern, and the report carries the
    location only, never the text.
- Each candidate proposes a slot path `<node>.<usage>` (`plan.instructions`
  for a constant used by the `plan` node; the symbol name replaces the node
  when no node uses it) and a prompt id made from the slot path with its
  dots as underscores (`prm_plan_instructions`).

**The report is not a coverage proof.** A string built inline in a node under
another name (`system = f"Reply in {locale}"`) is not reported at all, and a
template that no node references keeps `node_path: null`. Read the nodes
yourself and list in the report what stays in code.

### 8.2 Import

```sh
export AGENOMIC_ENDPOINT=https://<registry> AGENOMIC_API_KEY=<write key>
agenomic-py prompts import prompt-report.json --agent-id <agent uuid>
agenomic-py prompts import prompt-report.json --agent-id <agent uuid> \
  --apply --declare-slots --slots-revision 0
```

- Without `--apply`, the command uploads the report, prints the registry's
  plan (JSON on stdout, `plan <plan_id> <plan_digest>` on stderr) and changes
  nothing. Review every item's `action` (`create_prompt`, `create_version`,
  `reuse_version`, `map_slot_only`, `skip`, `blocked`); `summary.unresolved`
  counts what could not be ported. The upload already needs a `write` or
  `admin` key.
- `--apply` uploads the same report again (the registry returns the same
  plan) and applies exactly that plan, citing its `plan_digest`.
  `--mode draft` writes drafts instead of versions.
- `--declare-slots --slots-revision N` also records the slot paths in the
  agent's slot inventory. `N` is the current inventory revision, `0` for an
  agent that has none; a stale value is refused with
  `agent_prompt_slots_conflict`.
- Exit codes: 0, 1 (refused or unreachable; the error code is printed) and 2
  (usage, no `AGENOMIC_ENDPOINT`, unreadable file). A missing
  `AGENOMIC_API_KEY` is not caught locally: the request goes out without a
  key and the command exits 1.
- The CLI is the only import path: `client.prompts` has no import,
  declaration file or runtime registration call yet. This path was not run
  against a live registry (capability matrix §11).
- Nothing rewrites the code. After the import, change each node by hand to
  read its slot (8.4).

### 8.3 Slots and releases

- A slot is a prompt position in the agent, such as `planner.instructions`:
  at least two lowercase segments joined by dots, at most 128 characters
  (`^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+$`).
- The slot inventory (declared by the import or in Agenomic Cloud) is
  descriptive and never changes what runs.
- The release manifest decides what runs: each slot is pinned to one
  immutable `prm_x:N` with its content digest, and child agents are pinned by
  release.
- A new manifest is a candidate release created in Agenomic Cloud. A
  signed-in person other than its author approves it, and a signed-in person
  promotes it on a channel. The SDK and `agm` have no call for slots,
  candidates, approval, promotion or rollback, and an API key can never
  approve or promote.
- Local mode simulates releases for tests: `Client()` without `base_url`,
  then `client.prompts.local.create_release(agent_id, {slot: "prm_x:N"})`
  and `client.prompts.local.move_channel(...)`. Examples 12 to 16 in
  `agenomic-python/examples` run that way, offline. It is a simulation, never
  the governed path.

### 8.4 Bind the graph

```python
from langchain_core.messages import SystemMessage
from langchain_core.runnables import RunnableConfig

from agenomic import Client
from agenomic.integrations import bind_langgraph, prompts_for


def plan(state: State, config: RunnableConfig) -> dict:
    prompts = prompts_for(config)
    system = prompts.render_text(
        "plan.instructions", {"customer": state["customer"]}
    )
    reply = model.invoke(
        [SystemMessage(system), *state["messages"]],
        prompts.config_for("plan.instructions"),
    )
    return {"messages": [reply]}


app = build_graph().compile(checkpointer=saver)
managed = bind_langgraph(
    app, client=Client.from_env(), agent_id=AGENT_ID, channel="production"
)
result = managed.invoke(state, {"configurable": {"thread_id": "ticket-1001"}})
```

- Call the proxy, never the inner graph. Every entry point (`invoke`,
  `ainvoke`, `stream`, `astream`, `astream_events`, `update_state`, `batch`)
  creates or reads the thread's binding before the first node runs. A node
  run without the proxy fails with `binding_missing`.
- Pass exactly one of `channel` and `release_id`: there is no default and no
  latest. `agent_id` is the agent's uuid in Agenomic Cloud.
- `configurable.thread_id` is required (`thread_id_required`). It is hashed
  before it leaves the process.
- Run with a `read` key. A key with all scopes, `write` or `admin` raises
  `privileged_credential` unless `allow_privileged_credential=True`.
  `Client.from_env()` reads `AGENOMIC_ENDPOINT`, `AGENOMIC_API_KEY` and,
  optionally, `AGENOMIC_WORKSPACE_ID`; `Client()` without `base_url` is the
  local registry.
- In a node, read prompts only through `prompts_for(config)`, with the
  `config` that LangGraph passes to the node, never `get_config()` (it fails
  in async nodes on Python 3.10). `render_text`, `render_messages`,
  `compose(slot, variables, history=...)` and `version(slot)` read the pinned
  release, and no alias is resolved inside a managed run.
- Pass `prompts.config_for(slot)` to the model call and return only the
  reply, so the rendered system prompt never enters the checkpointed history.
- The caller cannot choose the binding: the `agenomic_*` keys the adapter
  injects are reserved, and a caller that sets one gets
  `agenomic_reserved_key`.

### 8.5 Pinning

- Thread scope, the default: the first call on a thread creates its binding
  and every later call gets the same one. A promotion reaches new threads
  only. An existing thread keeps its release on a later turn, on a resume
  after `interrupt()`, after a process restart on a durable checkpointer and
  on a time-travel fork.
- `pin_scope="execution"` pins one execution instead. A new input then needs
  `configurable["agenomic_execution_key"]`, an unguessable id that the retries
  of one request share (`execution_key_required` otherwise);
  `managed.with_retry(...)` generates one per input.
- Each invocation makes one small idempotent binding request, and none per
  node or token.
- During a registry outage an existing thread continues on its cached,
  verified binding, and a new thread raises `registry_unavailable`. The
  default cache is in memory, so after a restart an existing thread
  continues only with a disk cache (`AGENOMIC_PROMPT_CACHE_DIR` or
  `PromptCache(directory)`). An online `bind_langgraph` reads
  `GET /v1/whoami`, so it raises `registry_unavailable` in a process that
  starts during the outage; an offline bundle (8.8) needs no registry.
  Nothing falls back to a bundle, a cached latest version or an inline
  string.
- Checkpoint metadata carries `agenomic_binding_id`,
  `agenomic_prompt_manifest_digest` and `agenomic_release_id`, never prompt
  text or credentials.

### 8.6 Subagents

```python
managed = bind_langgraph(
    app,
    client=client,
    agent_id=SUPERVISOR_ID,
    channel="production",
    children={"research": RESEARCHER_ID},
)


def review(state: State, config: RunnableConfig) -> dict:
    return reviewer_graph.invoke(state, scope_config(config, REVIEWER_ID))
```

- One binding pins the root release and the child releases of its manifest;
  a child never resolves its own channel.
- A compiled child graph added as a node: map the node name in `children=`.
  A child invoked in a wrapper node or a tool: pass
  `scope_config(config, CHILD_ID)`. A child with its own thread and
  checkpointer: `scope_config(config, CHILD_ID, thread_id=...)`.
- Always pass `config` to the child. Bind only the root: a child invoked with
  a fresh config raises `binding_missing` or `nested_bind_unsupported`, and a
  child missing from the release raises `child_agent_not_pinned`.

### 8.7 Prompts captured at construction

```python
agent = create_react_agent(
    model, tools, prompt=managed_prompt("researcher.system"), checkpointer=saver
)
managed = bind_langgraph(
    agent, client=client, agent_id=AGENT_ID, channel="production"
)
```

`managed_prompt(slot)` is a Runnable that renders the pinned slot on every
call, for constructors that accept a Runnable prompt. For a string captured
at construction, bind an `AgentFactory` instead:

```python
factory = AgentFactory(
    lambda p: create_react_agent(
        model,
        tools,
        prompt=p.version("researcher.system").render_text({}),
        checkpointer=saver,
    )
)
managed = bind_langgraph(
    factory, client=client, agent_id=AGENT_ID, channel="production"
)
```

The factory builds one graph per release, and every graph must have the same
nodes and checkpointer (`factory_topology_mismatch`). `create_react_agent` is
deprecated in LangGraph 1.x; `langchain.agents.create_agent` is not
supported.

### 8.8 Offline bundles

```python
from agenomic.integrations import LocalBindingStore, bind_langgraph
from agenomic.prompts import BundleTrust

managed = bind_langgraph(
    app,
    agent_id=AGENT_ID,
    channel="production",
    offline=True,
    workspace_id=WORKSPACE_ID,
    bundle="prompt-bundle.json",
    trust=BundleTrust.from_pem_files("keys/orgkey_01.pem"),
    binding_store=LocalBindingStore(".agenomic/prompt-pins"),
)
```

- The bundle is a signed export of one release
  (`client.prompts.export_bundle`). Loading it checks the signature against
  `trust`, every digest, the workspace and agent, the expiry and the approval.
  `agenomic-py prompts bundle-verify` runs the same checks from a shell.
- A graph with a checkpointer needs `LocalBindingStore(directory)`, so pins
  survive a restart. Ship a new release as a new bundle and pass the earlier
  ones in `retained_bundles`, so paused threads resume on their original
  prompts (example 15).
- A disconnected process cannot learn about a revocation: a bundle stays
  usable until its `expires_at`.

### 8.9 Tracking

Add the handler of section 2 to the same config:

```python
managed.invoke(
    state,
    {
        "configurable": {"thread_id": "ticket-1001"},
        "callbacks": [TrackingCallbackHandler(session)],
    },
)
```

The `model.call.started` event of a node that passed `config_for(slot)`
gains `prompt_binding_id`, `prompt_manifest_digest`, `prompt_refs` (`slot`,
`ref`, `content_digest`), `prompt_rendered_hash`, and `agent_version` when
the release has one. Refs and hashes only, never prompt text; the
`completed` and `failed` events carry none of them.

### 8.10 Versions and limits

- Install range `langgraph>=1.0.10,<2` with `langchain-core>=1.6.3,<2`.
  Tested points: langgraph 1.2.11, and 1.0.10 with `langgraph-prebuilt` 1.0.8,
  both with langchain-core 1.6.3. They were run locally on macOS arm64 only;
  no CI cell has run yet. Any other version runs with one
  `AgenomicUntestedVersionWarning`.
- `interrupt()` inside async nodes needs Python 3.11. On Python 3.10 async,
  `nested_bind_unsupported` cannot be detected.
- `astream_events(version="v3")` passes through as experimental on 1.2.11.
  On 1.0.10 LangGraph raises `NotImplementedError`, and the proxy passes it
  on.
- Wrap a stream consumed partially in `contextlib.closing` (`aclosing` for
  async) so the proxy releases it at once.

### 8.11 Verify

```python
config = {"configurable": {"thread_id": "ticket-1001"}}
managed.invoke(state, config)
metadata = managed.get_state(config).metadata
assert metadata["agenomic_prompt_manifest_digest"].startswith("sha256:")
assert metadata["agenomic_release_id"] == EXPECTED_RELEASE_ID
```

Then confirm that no node calls `get_config()`, that every model call in a
managed node passes `config_for`, and that nothing left in code is reported
as managed.
