# TypeScript recipe

Applies when `agm init --dry-run` reports `runtime.runtime_kind: node`
(TypeScript or JavaScript).
Node 18+ required (CI gates 20, 22 and 24). The SDK uses `node:async_hooks`
and `node:fs`, so it runs on the Node runtime only, not on Edge or in the
browser.

```sh
pnpm add @treansai/agenomic-typescript
```

The scope matters: `agenomic-typescript` is the SDK's wire identity inside
trace metadata, not the npm name. Some SDK README snippets import from
`@agenomic/sdk`; that package does not exist.

The TypeScript trace model has memory, policy checks and human feedback; the
SDK has no ATEP signing. See `references/capability-matrix.md`.

## 1. Client

```ts
import { AgenomicClient } from "@treansai/agenomic-typescript";

const client = new AgenomicClient({
  apiKey: process.env.MYAPP_AGENOMIC_API_KEY,
  endpoint: process.env.MYAPP_AGENOMIC_TRACES_URL,  // full ingest URL, incl. /v1/traces
  baseUrl: process.env.MYAPP_AGENOMIC_API_URL,      // API root for tracking, rmp, tools...
});
```

Options: `apiKey`, `endpoint`, `baseUrl`, `headers`, all optional. `endpoint`
is POSTed to verbatim, so it carries the `/v1/traces` path. `baseUrl` is the
API root for the resources; when it is absent the resources derive it from
`endpoint` with a trailing `/v1/traces` stripped. Every call sends
`authorization: Bearer <apiKey>`.

With neither `endpoint` nor `baseUrl`, the client makes no HTTP request, but
"local-only" means three different things:

| Surface | Without a URL |
|---|---|
| Tracing (`emit`) | envelope is zod-validated, then not sent |
| `tracking`, `rmp`, `review`, `monitor`, protect reads | buffered in the instance; never throws |
| `tools`, protect enforcement, `benchmarks` | **throws** (`ToolExecutionError("cloud_required")` for tools) |

The SDK does not read `AGENOMIC_*` from the environment; those variables
belong to the CLI. Name the project's own variables after the project and add
them to its `.env.example` with empty values. The one exception:
`new HuggingFaceClient()` reads `HF_TOKEN` / `HUGGINGFACE_API_TOKEN`,
`HUGGINGFACE_ENDPOINT_URL`, `HUGGINGFACE_ORG`, `HUGGINGFACE_DEFAULT_MODEL` and
`HUGGINGFACE_TIMEOUT_SECONDS`.

## 2. Run boundary

```ts
import { traceAgentRun } from "@treansai/agenomic-typescript";

export const handleClaim = traceAgentRun(
  {
    client,
    agentId: "agent://acme/claims",
    release: "2026.7.1",
    redact: ["customer.email", { path: "customer.ssn", mode: "hash" }],
  },
  async (payload, trace) => {
    // `trace` is the TraceBuilder for this run
    return { approved: true, claimId: payload.claimId };
  },
);
```

`traceAgentRun` completes the trace on return and fails it on throw, then
re-throws. Options: `client`, `agentId`, `release`, `redact`, `redactionMode`,
`localOnly`, `metadata` (envelope), `runMetadata` (`run.metadata`), `tags`.
The envelope is not returned; call `trace.build()` if you need it.

Retrieve the builder from nested code with `getCurrentTrace()`, or establish
the context explicitly with `withTraceContext(trace, async () => ...)`.

### Export failures reach the agent

`traceAgentRun` awaits `trace.emit()` inside its own `try`. When `endpoint`
is set and ingestion fails (non-2xx, network error) or the envelope fails zod
at `build()`, the catch calls `trace.fail()` on an already-finalized trace,
and **the wrapped call rejects with `Trace has already been finalized.`**,
even though the handler succeeded. `withTracedRoute` has the same shape.

This breaks the "never crash the agent for telemetry" rule, so in any process
that must survive Agenomic being unreachable:

- leave `endpoint` unset on the client the agent uses (emit is then a no-op), or
- build the run manually and guard the export yourself:

```ts
import { withTraceContext } from "@treansai/agenomic-typescript";

export async function handleClaim(payload: Claim) {
  const trace = client.createTrace({
    agentId: "agent://acme/claims",
    input: payload,
    redact: ["customer.email", { path: "customer.ssn", mode: "hash" }],
  });
  try {
    const result = await withTraceContext(trace, () => runClaim(payload));
    trace.complete({ output: result });
    return result;
  } catch (error) {
    trace.fail(error);
    throw error;
  } finally {
    try {
      await trace.emit();
    } catch (err) {
      logger.warn({ err }, "agenomic trace export failed");
    }
  }
}
```

`createTrace` also accepts `release`, `sessionId`, `parentRunId`, `traceId`,
`runId`, `tags`, `metadata`, `runMetadata` and `redactionMode`.
`trace.complete()` defaults `status` to `"success"`.

## 3. Events

`TraceBuilder` methods, all chainable:

| Method | Records |
|---|---|
| `addModelCall(draft)` | one LLM invocation |
| `addToolCall(draft)` | one tool invocation |
| `addMemoryAccess(draft)` | a memory read / write / search / delete |
| `addPolicyCheck(draft)` | a policy evaluation and its outcome |
| `addHumanFeedback(draft)` | a human `approve` / `reject` / `escalate` |
| `addEvent(draft)` | one of the six event types, by `type` |

`addEvent` is not a free-form escape hatch: the only types are `model_call`,
`tool_call`, `memory_access`, `policy_check`, `human_feedback` and
`run_completed`.

```ts
trace.addModelCall({
  type: "model_call",
  provider: "anthropic",
  model: "claude-sonnet-5",
  input: { messages },
  output: { text: completion },
  usage: { inputTokens: 812, outputTokens: 240 },
});

trace.addToolCall({
  type: "tool_call",
  toolName: "payout_api",
  input: args,
  output: result,
  status: "ok",                 // "ok" | "error"
});

trace.addMemoryAccess({
  type: "memory_access",
  store: "postgres",            // required
  operation: "write",
  key: "claim:123:history",
});

trace.addPolicyCheck({
  type: "policy_check",
  policyName: "claims-input-check",
  outcome: "allow",             // "allow" | "deny" | "review"
});

trace.addHumanFeedback({
  type: "human_feedback",
  reviewerId: "ops-42",
  disposition: "approve",       // "approve" | "reject" | "escalate"
});
```

Drafts are checked by the TypeScript types and by `TraceEnvelopeSchema` at
`build()`. The exported event schemas (`ModelCallSchema`, `ToolCallSchema`,
`MemoryAccessSchema`, `PolicyCheckSchema`, `HumanFeedbackSchema`) validate
*materialized* events, which carry `id`, `traceId`, `runId` and `timestamp`;
a draft fails them.

## 4. Provider instrumentation

```ts
import { instrumentOpenAI, instrumentHuggingFace } from "@treansai/agenomic-typescript";

const openai = instrumentOpenAI(new OpenAI());
```

`instrumentOpenAI(client, { trace?, provider?, overlay? })` returns a shallow
copy that wraps only `responses.create` and `chat.completions.create`, and
records only while a trace is active. Other methods (`stream`, `parse`, ...)
are not carried over and streaming has no special handling. Verify it covers
the call path this project actually uses, and fall back to explicit
`addModelCall` if not.

Hugging Face has a fuller provider surface: `HuggingFaceClient`
(`validateCredentials`, `resolveModelMetadata`, `generateText`,
`embeddings`), `lockModel` and `redactToken(text, token?)`. `lockModel`
returns a credential-free lock record (`resolvedCommit`, `metadataHash`,
`parameterHash`) for the genome; it does **not** pin inference calls, since
`generateText` takes no revision. Record the resolved commit, and treat a
changed commit as drift.

**Anthropic has no TypeScript adapter.** Record manually with `addModelCall`.

## 5. Next.js

```ts
import { withTracedRoute } from "@treansai/agenomic-typescript";

export const POST = withTracedRoute(
  {
    client,
    agentId: "agent://acme/claims",
    mapRequest: async (req) => ({ body: await req.clone().json() }),
  },
  async (req, context, trace) => { /* ... */ },
);
```

Options add `mapRequest` and `mapResponse` to the `traceAgentRun` set. The
default request snapshot copies **every header, including `authorization`
and `cookie`**: always pass `mapRequest` or `redact`. Also exported:
`serializeRequestSnapshot`, `serializeResponseSnapshot`. The export-failure
caveat of section 2 applies.

## 6. Online tracking

```ts
const session = await client.tracking.start({
  agent: "agent://acme/claims",     // note: `agent`, not `agentId`
  environment: "production",        // defaults to "production"
  releaseId, bundleId, genomeHash,  // optional
  trackingConfig: { loops: { max_same_tool_calls: 3 } },  // cloud only
});

await session.step("classify_claim", async () => {
  await session.modelCall({ provider: "openai", model: "gpt-4o", inputHash });
  await session.toolCall({ toolName: "payout_api", protocol: "http", inputHash, outputHash });
  await session.intent("verify_claim_validity");
  await session.memoryWrite({ schemaVersion: "1.0.0" });
});

await session.stop();
```

`client.tracking` is a `TrackingResource`. `step(name, fn?)` emits
`agent.step.started` / `agent.step.completed` (or `agent.failed` on throw);
without `fn` it returns a handle whose `end()` closes the step. For anything
the helpers do not cover, `session.event({ type, ... })` takes the full
`TrackingEventInput`: `parentEventId`, `workflowStepId`, `toolName`,
`toolProtocol`, `toolPermissions`, `modelProvider`, `model`, `temperature`,
`inputHash`, `outputHash`, `intent`, `redactedPreview`, `policyResult`
(`{ policyId?, outcome: "allow"|"deny"|"review", denies? }`) and `metadata`.

Local mode buffers events: write `session.toJsonl()` to a file and analyze it
offline with `agm track`. Cloud mode streams them; `session.report()` is
cloud only. Cloud `event()` / `stop()` throw on HTTP errors, so guard them
like the trace export. `trackingConfig` is forwarded verbatim to the API and
ignored locally.

Event types follow the registry: `agent.started`, `agent.step.started`,
`agent.step.completed`, `model.call.started|completed`,
`tool.call.started|completed`, `memory.read`, `memory.write`,
`policy.evaluated`, `intent.detected`, `loop.detected`, `drift.detected`,
`harness.violation`, `alert.created`, `agent.completed`, `agent.failed`.

There is no `turn.*` event. Use `agent.step.*`.

## 7. Review / Monitor / Protect

Use the client namespaces; local state lives per instance, so do not build
second copies of the resources.

- `client.rmp`: sessions, `start({ agent, releaseId?, environment?, ledger?, genomeHash? })`,
  `stop(sessionId)`, `get`, `list`, `report`. Local mode reuses the active
  session for the same agent and environment until `stop()`; local `report()`
  returns zero counts. `ledger: true` asks the cloud for a tamper-evident
  session ledger.
- `client.review`: pre-release, `run({ agent, scenarios?, riskMatrix? })`,
  `listScenarios`, `addScenario`, `proposals`, `approveScenarioEnrichment`
- `client.monitor`: live, `start`, `event({ sessionId, event })`, `findings({ sessionId })`
- `client.protect`: alerts, action plans, recommendations, `notify`; cloud
  only: `overlay`, `catalog`, `killSwitch`, `simulate`, `coverage`,
  `metricsSummary`, and `.approvals`, `.decisions`, `.policies`,
  `.bindings`, `.restrictions`

Enable Review only when there is a release gate to attach it to, and Protect
only when a human will act on the alerts. An unwatched alert channel is worse
than none.

## 8. Tool execution and Protect enforcement (cloud only)

`client.tools` is the Tool Gateway and Mock Engine used by replays: each tool
call of a tool-execution run is routed to the real backend (secrets resolved
server-side) or to a mock, per tool, with no silent fallback to a real call.
It throws `ToolExecutionError("cloud_required")` without a URL and does not
record TraceBuilder events.

```ts
import { ToolApprovalPending, ToolCallDenied } from "@treansai/agenomic-typescript";

let run = await client.tools.createRun({ name: "hybrid", configText, repetitions: 3 });
if (run.status === "planned") run = await client.tools.approveRun(String(run.id), String(run.plan_hash));
await client.tools.startRun(String(run.id));

const router = client.tools.router(String(run.id), {
  localFunctions: { "crm.update_customer": updateCustomer },
  beforeAction: (intent) => audit.log(intent),   // may throw to abort; never approves
});

try {
  await router.call("crm.update_customer", { id: "c_42", fields: { credit_limit: 50000 } });
} catch (error) {
  if (error instanceof ToolApprovalPending) {
    await router.resume(error, { pollIntervalMs: 2000, timeoutMs: 900_000 });
  } else if (error instanceof ToolCallDenied) {
    // error.code, error.decision?.reason_codes, error.transformation
  } else {
    throw error;
  }
}
```

- **Pending** (202): `ToolApprovalPending` with `approvalId`. `resume()` polls
  `client.protect.approvals.get(id)` and, once approved, re-issues the
  identical call once (same arguments, logical call id and idempotency key).
  Rejected or expired approvals throw `ToolCallDenied`; the wait is in-process
  polling on the same router instance.
- **Denied** (403): `ToolCallDenied`, message = the safe explanation.
- Both throw regardless of `raiseOnError`. `localFunctions` run in-process
  only after the gateway answers `local`; a failed report leaves the call
  with `external_state: "indeterminate"` in `router.calls`.
- A different reviewer decides with
  `client.protect.approvals.decide(id, { decision: "approve" | "reject" })`;
  self-approval is refused.

`instrumentOpenAI(new OpenAI(), { overlay: await client.protect.overlay(runId) })`
prepends the run's Protect overlay to the system prompt. That edits the prompt,
so it is a deliberate enforcement choice, never part of a telemetry-only
integration.

## 9. Benchmarks (cloud only)

`client.benchmarks` plans and launches external suites (τ²-bench,
ToolSandbox, AppWorld, AgentDojo, MCP-Universe) against the customer's own
agent. `BridgeServer(client, bridge, { agent, releaseId? }).serve()` exposes
the agent through an `AgentTargetBridge` (`capabilities`, `handleTurn(turn)`)
so the benchmark environment can drive it turn by turn. See the SDK's
`docs/benchmarks.md`. This is a Review activity, not instrumentation: plan it
only in `full` mode and only when the user asks.

## 10. Redaction

```ts
import { applyRedaction, redactTraceEnvelope } from "@treansai/agenomic-typescript";
```

Modes: `remove`, `mask` (default, writes `"[REDACTED]"`), `hash` (sha256 hex).
The `redact` option on `traceAgentRun` / `createTrace` is applied to the
envelope inside `build()`, and only to `run.input` / `run.output` and event
`input` / `output` / `query`. Raw values stay in the builder's memory, and
`inputHash` / `outputHash` are computed on the unredacted values. Never put a
secret in `metadata` or `runMetadata`: redaction does not reach them.

## 11. Export and verify

```ts
import { exportTracesToJsonl, sendTraceToHttp } from "@treansai/agenomic-typescript";
```

Nothing writes `.agenomic/traces.jsonl` for you. `exportTracesToJsonl(path,
traces)` (and `client.exportJsonl`) **overwrites** the file; to accumulate
runs, append `JSON.stringify(await trace.emit()) + "\n"` yourself.
`sendTraceToHttp(endpoint, trace, { apiKey?, headers? })` sends one awaited
POST per trace, with no batching, and throws on a non-2xx answer.

```sh
node ./dist/smoke.js
jq -s '[.[].events[] | select(.type == "model_call")] | length' .agenomic/traces.jsonl
jq -s '[.[].events[] | select(.type == "tool_call")]  | length' .agenomic/traces.jsonl
```

**The CLI cannot read TypeScript traces today.** `agm trace validate`,
`agm trace summarize` and `agm replay` expect `trace_id` / `agent_id` /
`input` at the top level and fail on the TS envelope
(`{specVersion, run, events}`) with `missing field trace_id`. Count calls
from the JSONL as above, and report CLI validation and local replay as
blocked on the trace shape (capability matrix §2).

## Gaps to record for TypeScript

| Capability | Status |
|---|---|
| ATEP signed events | ❌ write JSONL, then `agm atep append` |
| Ledger | ❌ no SDK writes, use CLI `agm ledger`; ⚠ cloud RMP session ledger via `ledger: true` |
| Replay | ❌ `agm replay` rejects TS traces; ⚠ cloud Tool Gateway / Mock Engine only |
| Evidence / governance | ❌ CLI only |
| Anthropic adapter | ❌ manual `addModelCall` |
| LangChain / LangGraph adapter | ❌ Python only |
| Export failure isolation | ⚠ guard `emit()` yourself (section 2) |
