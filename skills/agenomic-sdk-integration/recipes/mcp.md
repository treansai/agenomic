# MCP recipe

MCP has helpers in both SDKs, but no auto-instrumentation: there is no single
client object to wrap, so you call the recorder after each invocation.

## Python

```python
from agenomic.integrations import trace_mcp_call
from agenomic.types.trace import CallStatus

result = await mcp_session.call_tool("search", {"q": query})

trace_mcp_call(
    server="kb-server",
    tool="search",
    input_data={"q": query},
    output_data={"hits": len(result.content)},
    status=CallStatus.SUCCESS,
    latency_ms=elapsed_ms,
    requires_human_approval=False,
    approval_present=None,
)
```

It records a `ToolCall` with `protocol="mcp"`, hashing input and output
(BLAKE3 over canonical CBOR). Outside a `@trace_agent_run` context it is a
no-op: safe to leave in code paths that run untraced.

Both `input_data` and `output_data` must be dicts. Wrap other shapes:
`{"value": result}`.

A wrapper worth adding once, rather than repeating the call at each site:

```python
import time

async def traced_mcp(session, server: str, tool: str, args: dict):
    started = time.perf_counter()
    status = CallStatus.SUCCESS
    output: dict = {}
    try:
        result = await session.call_tool(tool, args)
        output = {"content": str(result.content)[:512]}
        return result
    except Exception as exc:
        status = CallStatus.ERROR
        output = {"error": type(exc).__name__}
        raise
    finally:
        trace_mcp_call(
            server=server, tool=tool, input_data=args, output_data=output,
            status=status, latency_ms=int((time.perf_counter() - started) * 1000),
        )
```

## TypeScript

```ts
import { recordMCPToolCall } from "@treansai/agenomic-typescript";

const startedAt = new Date().toISOString();
const result = await mcpClient.callTool({ name: "search", arguments: { q: query } });

recordMCPToolCall({
  server: "kb-server",
  tool: "search",
  transport: "stdio",          // "stdio" | "sse" | "http", default "stdio"
  arguments: { q: query },
  result,
  status: "ok",                // "ok" | "error"; derived from `error` when omitted
  startedAt,
  endedAt: new Date().toISOString(),
});
```

It records a `tool_call` event with `toolName: "<server>.<tool>"` and the
server, tool and transport under `metadata.mcp`. The signature is
`recordMCPToolCall(call, trace?)`; without a `trace` argument it uses the
active trace. **Unlike Python, it throws when no trace is active.** Guard it
(`getCurrentTrace()`) on code paths that may run untraced.
`createMCPToolCall(call)` only builds the draft, for `trace.addToolCall`.

## Declaring servers in the genome

`agm init` detects MCP servers and tools from the project's MCP configuration
and records them in `genome.yaml`. Verify the detected list against the
config, and record for each tool: server name, tool id, schema version, and
permission scope.

Declared MCP tools that also matter at build time are emitted by
`agm compile` as **typed stubs, not live bindings** (`BACKEND_GAPS.md`).
Compiling a runtime with MCP tools does not wire the transport; the operator
does. Do not tell the user a compiled bundle can call their MCP servers.

## Permissions and the gate

MCP tools are the most common irreversible-effect surface. For a tool that
writes, pays, deletes or sends, enforce at the boundary rather than merely
recording:

```sh
agm gate check tool-call.json --policy policies/ --approval approval.json
```

`gate check` is deterministic (never an LLM call) and checks the tool
allowlist and scopes, self-modification, path traversal, sensitive files, PII
exfiltration to unapproved recipients, and irreversible effects. Arguments
derived from model, tool or MCP content are treated as `untrusted` and held to
stricter rules.

Verdicts: `Allow` (0), `Block` (16), `RequireHumanApproval` (18). Each passage
seals signed ATEP events on the `policy` and `governance` streams.

The gate is a standalone surface. Wiring it to intercept *individual* tool
calls inside a running agent is an open gap: today you call it from the
agent's own effect path, before the effect.

## Coverage summary

| Aspect | Support |
|---|---|
| Tool invocation | ✅ `trace_mcp_call` / `recordMCPToolCall` |
| Tool Gateway routing | ⚠ cloud only: `adapter: mcp` bindings in a tool-execution run |
| Server + tool id | ✅ recorded |
| Schema version | ⚠ not a helper parameter: record `ToolCall(protocol="mcp", schema_version=...)` yourself (Python) or pass it in `metadata` (TypeScript) |
| Permission state | ⚠ via `requires_human_approval` / `approval_present` |
| Enforcement | ✅ `agm gate check`, called by you |
| Auto-instrumentation | ❌ by design |
| Live transport in compiled runtimes | ❌ stubs only |
