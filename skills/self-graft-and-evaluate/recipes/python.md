# Python recipe — self-graft instrumentation

Apply this only if `agm init --dry-run` reports `language: python`.

## Install

```sh
pip install "agenomic[anthropic,openai,langgraph]"
```

Pick the extras matching the providers your agent already uses. The Agenomic
Python SDK lazy-imports each provider, so unused extras stay out of the
runtime.

## Instrument the entry point

Wrap your existing agent function with `trace_agent_run` and instrument any
LLM client at construction time. Do not refactor the agent's logic.

```python
# agent.py — BEFORE
from anthropic import Anthropic

client = Anthropic()

def run(input: dict) -> dict:
    response = client.messages.create(...)
    return {"answer": response.content[0].text}
```

```python
# agent.py — AFTER
from anthropic import Anthropic
from agenomic.integrations.anthropic import instrument_anthropic
from agenomic.trace.decorator import trace_agent_run

client = instrument_anthropic(Anthropic())

@trace_agent_run(agent_id="<your-agent-id>")
def run(input: dict) -> dict:
    response = client.messages.create(...)
    return {"answer": response.content[0].text}
```

That is the entire diff. `trace_agent_run` opens a recorder for the duration
of the call; `instrument_anthropic` makes every `messages.create` produce a
`ModelCall` event captured by that recorder.

## Other providers

| Provider | Wrapper |
|---|---|
| OpenAI | `agenomic.integrations.openai.instrument_openai` |
| LangGraph | `agenomic.integrations.langgraph.instrument_graph` |
| MCP tools | `agenomic.integrations.mcp.instrument_mcp_client` |

## Verify the recipe applied

```sh
python -c "from agent import run; run({'ping': 'pong'})"
ls .runs/   # should contain at least one trace envelope (at the bundle root)
```

If `.runs/` is empty, the decorator is not on the entry point actually being
called — re-check that `run` is the function your CLI / server invokes.
