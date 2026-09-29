# Python recipe

Applies when `agm init --dry-run` reports `runtime.runtime_kind: python`.
Python 3.10+.

## Install

```sh
pip install agenomic                   # core: tracing, ATEP, redaction, exporters,
                                       # Client (tracking, RMP, tools, protect), canonical v0.3
pip install "agenomic[openai]"         # + OpenAI adapter
pip install "agenomic[anthropic]"      # + Anthropic adapter
pip install "agenomic[huggingface]"    # + huggingface_hub, for trace_huggingface_call
pip install "agenomic[langgraph]"      # + LangGraph adapter
pip install "agenomic[mcp]"            # + mcp client library
pip install "agenomic[otel]"           # + OpenTelemetry, for canonical-run GenAI spans
pip install "agenomic[all]"
```

Integrations are lazy-imported: importing `agenomic.integrations` does not
import `openai`, `anthropic`, `langgraph` or `mcp`. Adding the dependency is
safe even where the extra is absent. The exception is
`agenomic.integrations.langchain` (the LangChain / LangGraph tracking
handler): it imports `langchain_core` eagerly and is not re-exported from
`agenomic.integrations`. There is no `langchain` extra; `langchain_core`
comes with `agenomic[langgraph]`.

The package also installs one CLI, `agenomic-py` (`atep verify|inspect`,
`traces summarize`, `keys generate`, `benchmark serve`). It is not `agm`.

## Three recording paths

The Python SDK records behavior three ways. Pick deliberately; they do not
feed the same consumers.

| Path | Entry point | Records | Consumed by |
|---|---|---|---|
| Trace envelope (v0.1) | `@trace_agent_run` + exporters | run, model calls, tool calls | `agm trace`, replay fixtures, ATEP store |
| Online tracking | `Client().tracking.start(...)` | event stream: steps, turns, model, tool, memory, policy, intent | `agm track` (local JSONL) or Agenomic Cloud |
| Canonical run (v0.3) | `agenomic.canonical.start_run(...)` | llm, tool, memory, policy check, human review, error | your own persistence; optional OTel GenAI spans |

The envelope path is the default integration. Add tracking when the plan
includes live monitoring. The canonical run returns a dict and wires no
exporter; it stores each event's payload inline, **in clear unless you pass
`redaction=`**, so do not adopt it without a redaction plan.

## 1. Run boundary

```python
from agenomic.trace.decorator import trace_agent_run
from agenomic.exporters.jsonl import JsonlExporter

exporter = JsonlExporter(".agenomic/traces.jsonl")

@trace_agent_run("agent://acme/claims", release="2026.7.1", exporter=exporter)
def handle_claim(payload: dict) -> dict:
    ...
```

Works on `async def` too: the decorator detects it and awaits the export.

Signature:

```python
trace_agent_run(
    agent_id: str,
    *,
    release: str | None = None,
    exporter: Exporter | None = None,
    redaction: RedactionEngine | None = None,
    capture_input: bool = True,
    capture_output: bool = True,
)
```

It records input arguments, everything the recorder collected, the output or
the error, and duration. On exception it records the error and **re-raises**:
the agent's own error handling is unchanged.

`run_id` and `trace_id` are generated per invocation (ULID). The envelope has
no `turn_id`; turns exist only in the tracking stream (section 8). See
`references/capability-matrix.md`.

## 2. Model calls

Instrument at client construction:

```python
from openai import OpenAI
from agenomic.integrations import instrument_openai

client = instrument_openai(OpenAI())     # wraps chat.completions.create
```

```python
from anthropic import Anthropic
from agenomic.integrations import instrument_anthropic

client = instrument_anthropic(Anthropic())   # wraps messages.create
```

Async variants: `instrument_openai_async`, `instrument_anthropic_async`.

Both patch the client in place and return it, and record a `ModelCall` with
`provider`, `model`, `temperature`, `prompt_hash`, `output_hash`,
`latency_ms` and `status` (OpenAI also records `fingerprint` from
`system_fingerprint`), and record an `ERROR` call before re-raising when the
provider fails. Prompt and response are hashed (BLAKE3 over canonical CBOR),
never stored raw.

Coverage limits to check against the project's real call path:

- OpenAI: only `chat.completions.create`. The Responses API and other
  endpoints are not recorded.
- A bound method captured before instrumenting (`create = client.chat.completions.create`)
  keeps the unwrapped function.
- Clients derived with `client.with_options(...)` are separate objects and
  are not patched.
- Azure OpenAI goes through the same wrapper but is recorded as
  `provider="openai"` with the deployment name as `model`.

Calls made outside a `@trace_agent_run` context are not recorded. That is
deliberate: importing an instrumented client at module scope is safe. The
provider call itself still happens.

The `overlay=` keyword on both adapters injects a Protect overlay into the
system prompt. It changes the prompt, so it is an enforcement decision
(section 10), never part of a telemetry-only integration.

### Hugging Face

```python
from agenomic.integrations import instrument_huggingface, trace_huggingface_call
from agenomic.providers.huggingface import HuggingFaceClient, HuggingFaceConfig

hf = instrument_huggingface(HuggingFaceClient(HuggingFaceConfig.from_env()))
hf.generate_text("gpt2", "hello")        # generate_text and embeddings are recorded

reply = trace_huggingface_call(hub_client.text_generation, model=model_id, prompt=prompt)
```

`HuggingFaceConfig.from_env()` reads `HUGGINGFACE_API_TOKEN` (then `HF_TOKEN`),
`HUGGINGFACE_ENDPOINT_URL`, `HUGGINGFACE_ORG`, `HUGGINGFACE_DEFAULT_MODEL` and
`HUGGINGFACE_TIMEOUT_SECONDS`. The token is never recorded. Neither helper
sets `fingerprint`: resolve the revision with
`HuggingFaceClient.resolve_model_metadata(model_id, revision="main")` and
record its `resolved_commit`.

### Providers without an adapter

Bedrock, Vertex, Mistral, Ollama, vLLM: record manually:

```python
import time
from agenomic.crypto.canonical import canonical_cbor
from agenomic.crypto.hashing import blake3_hex
from agenomic.trace.context import current_recorder
from agenomic.types.trace import CallStatus, ModelCall

def call_local_llm(prompt: str) -> str:
    started = time.perf_counter()
    status = CallStatus.SUCCESS
    try:
        response = my_client.generate(prompt)
    except Exception:
        status = CallStatus.ERROR
        raise
    finally:
        recorder = current_recorder()
        if recorder is not None:
            recorder.record_model_call(ModelCall(
                provider="ollama",
                model="llama3.1:8b",
                fingerprint="sha256:...",          # pin the revision
                prompt_hash=blake3_hex(canonical_cbor({"prompt": prompt})),
                latency_ms=int((time.perf_counter() - started) * 1000),
                status=status,
            ))
    return response.text
```

Always guard on `recorder is not None`.

## 3. Tool calls

```python
from agenomic.crypto.canonical import canonical_cbor
from agenomic.crypto.hashing import blake3_hex
from agenomic.types.trace import CallStatus, ToolCall
from agenomic.trace.context import current_recorder

recorder = current_recorder()
if recorder is not None:
    recorder.record_tool_call(ToolCall(
        tool="payout_api",
        protocol="http",
        input_hash=blake3_hex(canonical_cbor(args)),
        output_hash=blake3_hex(canonical_cbor(result)),
        latency_ms=elapsed_ms,
        status=CallStatus.SUCCESS,
        requires_human_approval=True,     # irreversible effect
        approval_present=approval is not None,
    ))
```

`protocol` is free-form; use `http`, `mcp`, `grpc`, `local`, `db`.
`ToolCall` and `ModelCall` accept extra fields (`schema_version=`,
`region=`), and `recorder.add_metadata(key, value)` attaches run-level
metadata.

For MCP use `trace_mcp_call`: see [`mcp.md`](mcp.md).
For LangGraph see [`langgraph.md`](langgraph.md): `instrument_langgraph`
records nothing on current LangGraph releases.

A decorator for repetitive tools. `canonical_cbor` raises on values it cannot
encode, so hashing is isolated: telemetry must never replace the tool's
return value or exception.

```python
import functools, time

def _safe_hash(value) -> str:
    try:
        return blake3_hex(canonical_cbor(value))
    except Exception:
        return blake3_hex(canonical_cbor({"repr": repr(value)}))

def traced_tool(name: str, protocol: str = "local", *, irreversible: bool = False):
    def deco(fn):
        @functools.wraps(fn)
        def wrapper(*args, **kwargs):
            recorder = current_recorder()
            started = time.perf_counter()
            status = CallStatus.SUCCESS
            result = None
            try:
                result = fn(*args, **kwargs)
                return result
            except Exception:
                status = CallStatus.ERROR
                raise
            finally:
                if recorder is not None:
                    recorder.record_tool_call(ToolCall(
                        tool=name,
                        protocol=protocol,
                        input_hash=_safe_hash({"args": list(args), "kwargs": dict(kwargs)}),
                        output_hash=_safe_hash(result),
                        latency_ms=int((time.perf_counter() - started) * 1000),
                        status=status,
                        requires_human_approval=irreversible,
                    ))
        return wrapper
    return deco
```

## 4. Memory

The trace envelope has no memory field. Record memory access where the plan
already has a surface for it:

- **tracking on:** `session.memory_write(schema_version=..., output_hash=...)`
  and `session.event("memory.read", ...)`;
- **canonical run:** `run.log_memory(store=..., operation="read"|"write", key=...)`;
- **envelope only:** a `ToolCall` with `protocol="memory"` and
  `tool="memory.read"` / `"memory.write"`. This is a workaround: say so in
  the coverage report.

Policy checks and human review follow the same split: `policy.evaluated` in
tracking, `log_policy_check` / `request_human_review` on a canonical run, and
`ToolCall.requires_human_approval` / `approval_present` in the envelope.
There is no local policy evaluator; evaluation is `agm policy eval` /
`agm gate check`, or Protect in the cloud.

## 5. Redaction

Apply before the envelope is built:

```python
from agenomic.redaction.engine import RedactionEngine
from agenomic.redaction.rules import RedactionMode, RedactionRule

redaction = RedactionEngine([
    RedactionRule(path="**.email",       mode=RedactionMode.MASK),
    RedactionRule(path="**.ssn",         mode=RedactionMode.HASH),
    RedactionRule(path="**.api_key",     mode=RedactionMode.REMOVE),
    RedactionRule(path="notes", mode=RedactionMode.TRUNCATE, truncate_length=200),
])

@trace_agent_run("agent://acme/claims", exporter=exporter, redaction=redaction)
def handle_claim(payload: dict) -> dict:
    ...
```

`RedactionEngine(rules)` takes the list directly; there is no `from_rules`.
Modes: `REMOVE`, `MASK` (writes `"***"`), `HASH` (`"hash:"` plus 16 hex chars
of BLAKE3), `TRUNCATE` (`truncate_length` required, strings only). Path
syntax: `a.b.c` exact, `a.*.c` one segment, `a.**.c` any depth. Unknown paths
are skipped silently, and the engine always works on a deep copy.

**The rules run twice, on two different roots.** Once on the captured input,
`{"args": [...], "kwargs": {...}}`, and once on the raw return value. There is
no `input.` or `output.` prefix. So a keyword argument `payload` is at
`kwargs.payload.…`, the same value passed positionally is at `args.0.…`, and
`path="notes"` above only matches a top-level `notes` key of the return
value. Prefer `**.field` rules, which match wherever the value lands. Verify
with one run:

```python
print(redaction.apply({"kwargs": {"payload": {"customer": {"ssn": "1"}}}}))
```

A rule that silently matches nothing is indistinguishable from no redaction.

To drop payloads entirely: `capture_input=False`, `capture_output=False`.

## 6. Exporters

| Exporter | Use |
|---|---|
| `JsonlExporter(path)` | always: local, append-only, replay fixtures |
| `AtepLocalExporter(store, signing_key)` | signed local history |
| `HttpExporter(client, batch_size=100, batch_interval_ms=5000)` | cloud, async; see the caveat below |
| `MultiExporter(*exporters)` | fan-out; one failing exporter does not stop the others |

Production:

```python
import os
from pathlib import Path
from agenomic.atep.store import AtepStore
from agenomic.crypto.signing import SigningKey
from agenomic.exporters.atep_local import AtepLocalExporter
from agenomic.exporters.jsonl import JsonlExporter
from agenomic.exporters.multi import MultiExporter

store = AtepStore.open_or_init(Path(".agenomic/atep"), "agent://acme/claims")
signing_key = SigningKey.from_pem_file(Path(os.environ["MYAPP_ATEP_KEY_PATH"]))

exporter = MultiExporter(
    JsonlExporter(".agenomic/traces.jsonl"),
    AtepLocalExporter(store, signing_key),
)
```

Generate the key once, outside the repo: `agenomic-py keys generate
<out.pem>` writes a PKCS#8 PEM with mode 0600 plus `<out>.pem.pub`. Never
commit a private key, and never `SigningKey.generate()` on each boot: a fresh
key per process breaks chain verification. `AtepStore.open_or_init` raises if
the store belongs to another `agent_id`.

`HttpExporter` takes the async cloud client,
`agenomic.client.AgenomicClient(endpoint, api_key)`, not `agenomic.Client`.
**`batch_interval_ms` is not implemented**: the exporter flushes only when
`batch_size` envelopes are queued while an event loop is running, or on
`await flush()`, `await aclose()` or `close()`. From sync code nothing is
sent until close. It is also not a WAL: envelopes queued at a crash are lost.
Pair it with `JsonlExporter` so the local file remains the durable record,
call `aclose()` on shutdown, and use `agm ledger` for the durability
guarantee.

Export failures are logged and swallowed. Do not change that.

## 7. Client (tracking, RMP, tools)

```python
from agenomic import Client

client = Client()                                            # local-only
client = Client(api_key=key, base_url=os.environ["MYAPP_AGENOMIC_URL"])  # cloud
```

`Client(api_key=None, base_url=None, *, timeout=30.0)` is local-first:
`client.is_cloud` is `base_url is not None`. It reads no environment
variable. Namespaces: `tracking`, `agent`, `rmp`, `review`, `monitor`,
`protect`, `benchmarks`, `tools`.

Without `base_url`, tracking buffers in memory, RMP runs in-memory stubs and
tools use an in-process engine; Protect policy and approval methods, all of
`benchmarks` and `tools.test_connection` raise immediately. **In cloud mode
every call is a synchronous HTTP request on the caller's thread and raises
`CloudError` on failure**, with no retry. Wrap each call on the hot path in
`try/except` and log, or the agent crashes when Agenomic is unreachable. The
LangChain handler (section 8) is the one surface that offloads to a worker.

## 8. Online tracking

```python
from agenomic.canonical import content_hash

session = client.tracking.start(
    agent="agent://acme/claims",
    release_id="release_123",          # optional; also bundle_id, genome_hash
    environment="production",          # default
    tracking_config=None,              # cloud only; forwarded verbatim
)

with session.step("classify_claim"):
    session.model_call(provider="openai", model="gpt-4o", input_hash=content_hash(prompt))
    session.tool_call(tool="claims_db.lookup", protocol="db",
                      input_hash=content_hash(args), output_hash=content_hash(row))
    session.intent("verify_claim_validity")
    session.memory_write(schema_version="1.0.0")

session.stop()
events = session.to_jsonl()            # local mode: write to a file for `agm track`
```

`content_hash(value)` returns `"blake3:<hex>"`. `step(name)` emits
`agent.step.started` / `agent.step.completed`, or `agent.failed` if the block
raises. `session.event(event_type, **snake_case_fields)` emits anything else;
an unknown type raises `ValueError`, and emitting after `stop()` raises
`RuntimeError`. `report()` is cloud only.

Event types: `agent.started|completed|failed`,
`agent.step.started|completed|failed`, `model.call.started|completed|failed`,
`tool.call.started|completed|failed`, `turn.started|completed|failed`,
`retrieval.started|completed|failed`, `memory.read`, `memory.write`,
`policy.evaluated`, `intent.detected`, `loop.detected`, `drift.detected`,
`harness.violation`, `alert.created`.

Detection (drift, loops, intent, harness) runs server-side or in
`agm track`, not in the SDK.

For LangChain and LangGraph, attach `TrackingCallbackHandler` instead of
hand-emitting events: see [`langgraph.md`](langgraph.md).

## 9. Review / Monitor / Protect

- `client.rmp`: `start(*, agent, release_id=None, environment="production", ledger=False, genome_hash=None)`,
  `stop(session_id)`, `get`, `list`, `report`. Local mode reuses the active
  session for the same agent and environment until `stop()`. `ledger=True`
  only asks the cloud for a session ledger.
- `client.review`: `run(*, agent, scenarios=None, risk_matrix=None)`,
  `list_scenarios`, `add_scenario`, `proposals`, `approve_scenario_enrichment`
- `client.monitor`: `start(...) -> MonitorSession` (`event(dict)`, `stop()`),
  `event(session_id, event)`, `findings(session_id)`
- `client.protect`: `alerts`, `action_plan`, `recommendations`, `notify`

Local mode returns stubs (`review.run` and `rmp.report` report "pass",
`notify` routes to stdout). Useful for wiring and tests, not evidence. Enable
Review only when there is a release gate to attach it to, and Protect only
when a human will act on the alerts.

## 10. Tool execution and Protect enforcement

`client.tools` routes tool calls of a tool-execution run to the real backend
or to a mock, per tool: the Tool Gateway and Mock Engine behind replays. The
local engine supports validation, preflight, the run lifecycle, `static` /
`rules` / `recorded` mocks and runtime-local functions; `mcp` / `http`
adapters, connection tests and `protect` need the cloud.

```python
from agenomic.tools import ToolApprovalPending, ToolCallDenied

router = client.tools.router(run_id, local_functions={"crm.update": crm_update})
try:
    router.call("payments.refund", {"amount_minor": 90000, "currency": "EUR"})
except ToolApprovalPending as pending:
    result = router.resume(pending, poll_interval=2.0, timeout=900.0)
except ToolCallDenied as denied:
    log.warning("refund denied: %s", denied.decision.reason_codes if denied.decision else [])
```

- **Pending:** `ToolApprovalPending` (202). `resume()` polls the approval and,
  once approved, re-issues the identical call once with the same idempotency
  key. Rejected or expired approvals raise `ToolCallDenied`.
- **Denied:** `ToolCallDenied` (403); nothing executed.
- Async: `client.tools.arouter(...)`.

Routing tools through the gateway replaces the agent's own tool execution.
That is a behavior change, not instrumentation: plan it only when the user
asks for replay or enforcement, never in `minimal` or `integrate` mode.

## 11. Benchmarks (cloud only)

`client.benchmarks` plans and launches external suites against the agent;
`serve_bridge(client, bridge, agent=...)` or `agenomic-py benchmark serve
--agent ... --bridge module:attr` exposes the agent through an
`AgentTargetBridge`. A Review activity, not instrumentation: `full` mode only,
and only when asked.

## 12. Verify

```sh
python -m your_app.smoke          # or whatever exercises one run
jq -c '. + {output: (.output // .final_output)}
       | .tool_calls |= ((. // []) | map(. + {name: (.name // .tool),
           human_approval_present: (.human_approval_present // .approval_present)}))' \
  .agenomic/traces.jsonl > .agenomic/traces.cli.jsonl
agm trace validate .agenomic/traces.cli.jsonl
jq -s '[.[].model_calls // [] | length] | add' .agenomic/traces.jsonl   # model calls
jq -s '[.[].tool_calls  // [] | length] | add' .agenomic/traces.jsonl   # tool calls
```

`agm trace validate` rejects the raw SDK file as soon as it contains a tool
call (`missing field name`), hence the conversion; `agm trace summarize`
prints only trace and agent counts. Count calls from the JSONL: both counts
must be non-zero. If tool calls are zero, the tool boundary was missed, the LangGraph adapter was
relied on, or a thread boundary dropped the contextvar: see
`references/troubleshooting.md`. `agenomic-py traces summarize` counts
envelopes and errors only, not calls.

## Gaps to record for Python

| Capability | Status |
|---|---|
| Memory | ⚠ tracking / canonical run only; no envelope field |
| Policy checks | ⚠ recorded (tracking, canonical); evaluation is CLI or cloud Protect |
| Human review | ⚠ recorded; approvals are cloud Protect |
| Online tracking | ✅ `Client().tracking` |
| RMP | ✅ local stubs; real analysis needs the cloud or `agm rmp` |
| LangGraph node tracing | ❌ `instrument_langgraph` is a no-op; use `TrackingCallbackHandler` or manual `ToolCall` |
| Ledger | ❌ CLI `agm ledger` |
| Cloud calls on the hot path | ⚠ raise `CloudError`; guard them |
