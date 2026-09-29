# CrewAI recipe

**There is no CrewAI runtime adapter.** `agm compile --target crewai` emits a
deterministic runtime adapter *from* a genome; it does not instrument a
running crew. Instrumentation here is manual.

Say this to the user up front rather than discovering it mid-integration.

## 1. Manifest

`agm init` detects CrewAI as the framework from dependencies. Its "tools"
are dependency kinds (`requests` → http, `sqlalchemy` → sql, `redis` →
memory), not crew tools, and the entrypoint comes only from a Dockerfile
`ENTRYPOINT` / `CMD` or `agenomic.yaml`. It does **not** recover crew
topology the way it recovers LangGraph graphs. Write the topology by hand:

- one agent per CrewAI agent, each with its own `agent_id`;
- `system.yaml` with members, roles and delegation edges;
- `workflows/<slug>.yaml` when the crew runs a fixed task sequence.

Use the templates in
[`../../self-graft-and-evaluate/templates/`](../../self-graft-and-evaluate/templates/)
rather than duplicating them here. Fill every `<PLACEHOLDER>`; leave semantic
fields for `agm enrich` or a human.

## 2. Run boundary: crew kickoff

```python
from agenomic.trace.decorator import trace_agent_run
from agenomic.exporters.jsonl import JsonlExporter

exporter = JsonlExporter(".agenomic/traces.jsonl")

@trace_agent_run("agent://acme/research-crew", exporter=exporter)
def run_crew(topic: str) -> str:
    return crew.kickoff(inputs={"topic": topic})
```

One run per `kickoff`. A crew invoked in a loop produces one run per
iteration, which is correct and is what makes loop detection meaningful.

## 3. Tasks as steps

CrewAI exposes callbacks. Prefer them over monkey-patching:

```python
import time
from agenomic.crypto.canonical import canonical_cbor
from agenomic.crypto.hashing import blake3_hex
from agenomic.trace.context import current_recorder
from agenomic.types.trace import CallStatus, ToolCall

def on_task_complete(output):
    recorder = current_recorder()
    if recorder is None:
        return
    recorder.record_tool_call(ToolCall(
        tool=f"task:{getattr(output, 'name', 'unnamed')}",
        protocol="local",
        server="crewai",
        output_hash=blake3_hex(canonical_cbor({"raw": str(output)})),
        status=CallStatus.SUCCESS,
    ))

crew = Crew(agents=[...], tasks=[...], task_callback=on_task_complete)
```

Check the installed CrewAI version's callback names: `task_callback` and
`step_callback` have moved between releases. Verify against the installed
package, not from memory.

## 4. Tools

Wrap tool functions before registering them with the crew; use `traced_tool`
from [`python.md`](python.md#3-tool-calls). Set `irreversible=True` on
anything with an external effect.

## 5. Model calls

Instrument the underlying provider client, which CrewAI reaches through
LangChain:

```python
from agenomic.integrations import instrument_openai
llm = ChatOpenAI(client=instrument_openai(OpenAI()).chat.completions)
```

The wiring depends on the CrewAI/LangChain versions in the project. Verify
with a smoke run: if model-call count is zero, the client the crew actually
uses was not the one you wrapped. This is the usual failure here: CrewAI
often constructs its own client internally. When that happens, fall back to a
manual `ModelCall` recorded from a LangChain callback handler.

## 6. Delegation

Agent-to-agent delegation is a handoff. Record it on the delegating side:

```python
recorder.record_tool_call(ToolCall(
    tool="delegate:researcher→writer",
    protocol="local",
    server="crewai",
    status=CallStatus.SUCCESS,
))
```

Declare the same edges in `system.yaml`; `agm validate` checks them (an edge
naming an undeclared role fails). `agm diff` compares only `genome.yaml` and
`behavior.contract.yaml`, so topology changes are not diffed.

## 7. Memory

The Python trace envelope has no memory field. If tracking is enabled,
record memory writes with `session.memory_write(...)`; otherwise record them
as a `ToolCall` with `protocol="memory"` and flag the gap in the report.

## 8. Verify

```sh
agm validate . --level strict
jq -c '{model: ((.model_calls // []) | length), tool: ((.tool_calls // []) | length)}' .agenomic/traces.jsonl
```

Expect: one run per kickoff, one tool call per task, one model call per LLM
invocation. A zero in either count means the hook did not fire: do not report
coverage until it does.

## Coverage summary

| Boundary | Support |
|---|---|
| Run | ✅ decorator on `kickoff` |
| Task step | ⚠ manual via callback |
| Tool | ⚠ manual wrap |
| Model | ⚠ depends on client construction; verify |
| Delegation | ⚠ manual |
| Memory | ⚠ tracking `memory_write`, else `ToolCall` workaround |
| Topology detection | ❌ hand-written manifest |
