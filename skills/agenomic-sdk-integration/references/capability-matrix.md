# Capability matrix: what actually exists

Ground truth for planning. Every row was verified against the source, not
against marketing surface. When a plan promises a capability, it must cite a
`✅` here. When a user asks for a `❌`, say so plainly.

Verified on 2026-09-29 against:

| Component | Version |
|---|---|
| Python SDK `agenomic` | v0.1.3 (`agenomic-python` main, 5e6a476) |
| TypeScript SDK `@treansai/agenomic-typescript` | 0.1.1 (`agenomic-typescript` main, 06fc037) |
| CLI `agm` / `agenomic` | `agenomic-cli` main (decdbce); latest public release `v0.3.0-apha.0` |
| Spec | `agenomic-spec` main (483f2e2) |

Section 11 (managed prompts) has its own verification line: it describes
unreleased branches of both SDKs and the CLI, not the versions above.

Re-verify this file whenever the SDKs or CLI change; a stale matrix is worse
than none, because it launders assumptions as facts.

Legend: `✅` shipped · `⚠` partial / manual / cloud only · `❌` not available

---

## 1. Per-surface coverage

| Capability | Python SDK | TypeScript SDK | CLI (`agm`) |
|---|:--:|:--:|:--:|
| Run tracing | ✅ `@trace_agent_run` | ✅ `traceAgentRun`, `createTrace` | ⚠ `trace validate`, `trace summarize` (CLI trace shape only, §2) |
| Model calls | ✅ `ModelCall` + adapters | ✅ `addModelCall` + adapters | none |
| Tool calls | ✅ `ToolCall` + `trace_mcp_call` | ✅ `addToolCall` + `recordMCPToolCall` | ✅ `gate check` (standalone, per call) |
| **Turns** | ⚠ tracking `turn.*` only | ❌ | ❌ `track` rejects `turn.*`; ⚠ optional `turn_id` on `ledger append`, no chain |
| Workflow steps | ✅ tracking `session.step()` | ✅ tracking `session.step()` | ✅ topology in `workflows/<slug>.yaml` |
| Memory read/write | ⚠ tracking / canonical run; no envelope field | ✅ `addMemoryAccess`, tracking | ✅ `memory.read` / `memory.write` via `track event` |
| Policy checks | ⚠ recorded (tracking, canonical run) | ✅ recorded (`addPolicyCheck`) | ✅ `policy eval`, `gate check` |
| Human review | ⚠ `ToolCall.requires_human_approval`, canonical `request_human_review` | ✅ `addHumanFeedback` | ✅ `gate check` → exit 18 |
| Protect enforcement (pending / deny / resume) | ⚠ cloud only, tool router | ⚠ cloud only, tool router | ❌ (`gate check` is the offline analogue) |
| Agent handoffs | ⚠ labels | ⚠ `parentRunId` / `sessionId` on `createTrace` | ✅ `system.yaml` members and edges |
| Redaction | ✅ `RedactionEngine` | ✅ `redact`, `applyRedaction` (at `build()`) | none |
| ATEP signed events | ✅ `AtepLocalExporter`, ed25519 | ❌ | ✅ `atep init/append/verify/inspect/replay-state` |
| Cryptographic ledger | ❌ (`ledger=True` on a cloud RMP session only) | ❌ (`ledger: true` on a cloud RMP session only) | ✅ `ledger *` |
| Online tracking | ✅ `Client().tracking` | ✅ `client.tracking` | ✅ `track start/event/tail/status/report/stop` |
| Drift / loop / intent detection | ⚠ emits events; detection is server-side or CLI | ⚠ same | ✅ `agenomic-track` detectors |
| Review / Monitor / Protect | ✅ `client.rmp/review/monitor/protect` (local stubs, real in cloud) | ✅ same | ✅ `rmp`, `review`, `monitor`, `protect` |
| Tool execution / mock replay | ✅ local engine (static, rules, recorded mocks) + ⚠ cloud gateway | ⚠ cloud only | ❌ no `tools` command |
| Benchmarks | ⚠ cloud only; `agenomic-py benchmark serve` | ⚠ cloud only; `BridgeServer` | ❌ |
| Trace replay | ❌ | ❌ | ⚠ `replay`: deterministic offline only; statistical via `cloud push-replay --mode statistical` |
| Evidence packages | ❌ | ❌ | ✅ `evidence export`, `evidence verify` |
| Governance engines | ❌ | ❌ | ✅ `governance cluster/hypothesize/critique/audit` (FlaggedTrace input) |
| Genome / lockfile / hashing | ⚠ `client.agent.load(path).configure_model(...)` | ⚠ `client.models.configure` (rewrites the file) | ✅ `init`, `update`, `hash`, `validate`, `diff` |
| Bundles / attestation | ⚠ `ReleaseAttestation`, `AgenomicClient.upload_bundle` / `create_release` | ❌ | ✅ `build`, `attest`, `verify`, `compile` |
| Cloud | ✅ `Client` (local-first), `AgenomicClient` (async uploads) | ✅ `AgenomicClient` | ✅ `cloud *`, `bucket use` |
| Managed prompts (versions, render, bindings, offline bundles) | ⚠ unreleased branch only, §11 | ⚠ unreleased branch only: read, render, bind, §11 | ⚠ unreleased branch only: `prompts`, `channels`, §11 |
| LangGraph prompt pinning | ⚠ unreleased branch only: `bind_langgraph`, §11 | ❌ | ❌ |
| Prompt discovery / import | ⚠ unreleased branch only: `agenomic-py prompts scan`, `import`, §11 | ❌ | ❌ |
| Prompt experiments | ⚠ unreleased branch only, cloud only: `client.experiments`, runner, §11 | ❌ | ❌ |

### Consequences for planning

**Turns are not a trace unit.** The trace envelopes and the CLI tracking
vocabulary have no turn. The Python tracking stream does (`turn.*`, emitted
by the LangChain handler), but `agm track` rejects those events, and
`agm ledger append` accepts an optional `turn_id` without building a
per-turn chain (deferred, `agenomic-cli/docs/atep-ledger.md`). Map a
conversational turn onto an `agent.step` pair, or onto its own run. Never
invent a `turn_id` in a trace envelope.

**There are two event vocabularies.** The *tracking* vocabulary
(`agent.started`, `agent.step.*`, `model.call.*`, `tool.call.*`,
`memory.read|write`, `policy.evaluated`, `intent.detected`, `loop.detected`,
`drift.detected`, `harness.violation`, `alert.created`, `agent.completed`,
`agent.failed`) is what the SDK tracking sessions and `agm track` speak. The
v0.3 *event-type registry* (`run.started`, `llm.requested`,
`tool.call.executed`, `policy.check.performed`, `human.review.*`, ...) is a
different list used by canonical runs and ATEP. `agm track event` accepts
only the 17 tracking types above.

**The SDKs are close to parity.** Both have tracing, tracking, RMP, cloud
Protect, the tool router and benchmarks. What differs: ATEP signing,
Anthropic, Hugging Face call wrapping and LangChain / LangGraph handlers are
Python only; the Next.js adapter is TypeScript only; memory, policy and human
feedback *inside the trace envelope* are TypeScript only; the local tool
engine is Python only. Plan for the SDK you actually have.

**Proofs are CLI-side.** No SDK writes to the ledger, replays traces or
produces evidence. SDKs emit traces and events; the CLI turns them into
ledger entries, replay reports and evidence. Any plan promising "SDK writes
to the ledger" is wrong. The SDKs do talk to Agenomic Cloud directly
(tracking, RMP, Protect, tools, benchmarks), and that channel fails loudly:
see §8.

**Managed prompts are unreleased, and only Python has the full surface.**
Every managed prompt call lives on a branch of an SDK or of the CLI, not in
the versions above. Python has imports, `bind_langgraph` and experiments on
top of the registry calls; TypeScript reads, renders and binds; `agm` reads,
publishes, renders and exports. No SDK or CLI call approves, promotes or
rolls back a release, and `agm channels promote` exits 0 without moving
anything: see §11.

---

## 2. Trace format: SDK vs CLI

`agm trace validate`, `agm trace summarize` and `agm replay` read one shape:

```json
{"trace_id": "...", "agent_id": "...", "input": {},
 "output": {}, "tool_calls": [{"name": "...", "arguments": {}, "result": {},
 "human_approval_present": false}], "metadata": {}}
```

`trace_id`, `agent_id`, `input` and `tool_calls[].name` are required; every
other field is ignored. Neither SDK emits exactly this today:

| Emitter | Accepted by `agm trace validate`? | Path |
|---|:--:|---|
| Python envelope, no tool calls | ✅ | as is |
| Python envelope with tool calls | ❌ `missing field name` (SDK writes `tool`) | convert, below |
| TypeScript envelope (`{specVersion, run, events}`) | ❌ `missing field trace_id` | no conversion path today |

Python conversion, verified against `agm trace validate`:

```sh
jq -c '. + {output: (.output // .final_output)}
       | .tool_calls |= ((. // []) | map(. + {name: (.name // .tool),
           human_approval_present: (.human_approval_present // .approval_present)}))' \
  .agenomic/traces.jsonl > evals/traces.cli.jsonl
```

Python traces carry hashes, not arguments or results, so contract rules that
inspect tool `arguments` / `result` have nothing to check. Record this in the
replay-readiness section of the report. For TypeScript, report CLI trace
validation and local replay as `❌ blocked on the CLI trace shape`, and
measure coverage from the raw JSONL instead (`events[].type`).

`agm trace summarize` prints only trace and agent counts, not model or tool
calls; coverage has to be counted from the JSONL itself
(`scripts/validate-integration.sh` does this).

**Tracking JSONL into `agm track`.** Local SDK sessions export events with
`to_jsonl()` / `toJsonl()`. `agm track event` ingests one event per call and
rejects the Python-only types (`turn.*`, `retrieval.*`, and every `*.failed`
except `agent.failed`):

```sh
SID=$(agm track start . --agent agent://acme/claims --format json | jq -r .session_id)
jq -c 'select((.type | test("^(turn|retrieval)\\.")) | not)
       | select(.type == "agent.failed" or (.type | endswith(".failed") | not))' session.jsonl |
while IFS= read -r event; do
  printf '%s\n' "$event" | agm track event --session "$SID" --file -
done
agm track stop --session "$SID"
```

Start the CLI session before the run whose events it ingests: the loop
detector measures event timestamps against the session start
(`max_duration_seconds`).

---

## 3. Frameworks

Two independent axes. A framework can be understood by the CLI at the
*manifest* level while offering no *instrumentation* adapter; that is the
common case, and conflating the two produces plans that cannot be built.

| Framework | Manifest / topology detection | Runtime instrumentation |
|---|:--:|:--:|
| Plain / custom | ✅ `agm init` | ⚠ manual at the four boundaries |
| LangGraph | ✅ recovers `add_node`, `add_edge`, `add_conditional_edges`, `set_entry_point`, `START` / `END` | ✅ `TrackingCallbackHandler` (Python, tracking stream); ❌ `instrument_langgraph` is a no-op; ⚠ `bind_langgraph` pins managed prompts per thread (Python, unreleased, §11) |
| LangChain | ⚠ framework detected from dependencies | ✅ `TrackingCallbackHandler` (Python, tracking stream) |
| Temporal (Python) | ✅ recovers `@workflow.defn`, `@workflow.signal` | ⚠ manual at activity/workflow boundaries |
| CrewAI | ⚠ dependency-level; `compile --target crewai` | ⚠ manual at crew/task/tool boundaries |
| OpenAI Agents, LlamaIndex | ⚠ framework detected from dependencies | ❌ manual only |
| MCP | ❌ no detection; declare `tools[].protocol: mcp` and `server` in the genome by hand | ⚠ `trace_mcp_call` / `recordMCPToolCall` (manual helpers) |
| Google ADK | ⚠ detected from dependencies; `compile --target google-adk` | ❌ |
| AutoGen | ❌ | ❌ manual only |
| Next.js routes | not applicable | ✅ `withTracedRoute` (TypeScript) |

`agm init` detects one framework, first match of google-adk > langgraph >
langchain > openai-agents > crewai > llama-index, else `custom`. It writes
recovered topology to `workflows/<slug>.yaml`, and synthesizes `system.yaml`
when it finds two or more LangGraph graphs, or a graph plus Temporal.

`agm compile` emits deterministic runtime adapters for
`plain | langgraph | crewai | google-adk | docker | wasm`; MCP tools are
emitted as typed stubs, not live bindings (`BACKEND_GAPS.md`). Compiling a
runtime target is not the same as instrumenting a running agent.

---

## 4. Providers

| Provider | Auto-instrumentation | Path when absent |
|---|:--:|---|
| OpenAI | ✅ `instrument_openai` (chat completions only) / `instrumentOpenAI` (chat completions + responses) | manual for other endpoints |
| Anthropic | ✅ `instrument_anthropic` (Python only) | manual `addModelCall` in TS |
| Hugging Face | ✅ `instrument_huggingface`, `trace_huggingface_call` (Python); `instrumentHuggingFace` (TS) | none |
| Azure OpenAI | ⚠ the OpenAI wrapper records `provider="openai"`, deployment as `model` | manual if the distinction matters |
| AWS Bedrock | ❌ | manual `ModelCall` / `addModelCall` |
| Vertex AI | ❌ | manual |
| Mistral | ❌ | manual |
| Ollama / vLLM / local | ❌ | manual; record the endpoint, pin the revision |
| Custom OpenAI-compatible | ⚠ try the OpenAI wrapper on the client | manual if the SDK differs |

`agm init` detects a single provider (first match of anthropic > openai >
google > cohere > huggingface) and uses its default model unless one is
pinned in `agenomic.yaml`. Read the client construction sites yourself.

Record for every provider: `provider`, `model`, `fingerprint` (revision),
`temperature`, `prompt_hash`, `output_hash`, `latency_ms`, `status`. Never the
API key. For local models, the model *revision* is the drift signal: pin it.

---

## 5. Observability sources

Agenomic sits **alongside** existing observability, never in place of it.

| Source | Integration approach |
|---|---|
| OpenTelemetry | Keep the tracer. Copy the active trace id into a trace label (Python) or `runMetadata` (TS). No OTel→Agenomic bridge ships. Python canonical runs can emit OTel GenAI spans (`start_run(..., tracer=)`), the other direction. |
| LangSmith / Langfuse | Keep. Correlate by run id label. |
| Datadog / New Relic / CloudWatch | Keep. Emit Agenomic traces in parallel. |
| Custom JSON / JSONL logs | Can feed `agm trace validate` / `agm replay` if reshaped to the CLI trace shape (§2). `agm governance` takes FlaggedTrace records (`trace_id`, `agent_id`, `skill`, `signal`, ...), not trace envelopes. |

The honest path is label-based correlation, documented in
`recipes/observability.md`.

---

## 6. Notification channels

`agm protect notify --alert <ID> --session <SID>` resolves and records the
route for an alert; **the OSS CLI never sends network traffic**. Routes are
hard-coded in the default Protect engine (policy violations and high
anomalies → security on-call, drift → ML platform, loops and the rest →
agent owners), and no flag, file or environment variable changes them.
Delivery to a real channel is Agenomic Cloud's job or the project's own
alerting. Alert recipients remain a **human decision** (see the Phase 3
safety rule in `SKILL.md`); never hardcode a webhook URL or token into a
manifest.

---

## 7. Known gaps

From `agenomic-cli/docs/BACKEND_GAPS.md`. Do not plan around these; they fail
closed by design:

- **Ledger cloud sync / `strict_cloud`**: not implemented; refuses with
  `agenomic::ledger::cloud_unavailable`. No silent downgrade.
- **Remote `agent://` resolution**: registry, channels, digest retrieval and
  the publisher trust ledger are not implemented. Local references resolve.
- **Remote bundle signature verification**: not implemented.
- **Distributed replay**: single-machine only. Statistical replay is cloud
  only.
- **Live MCP tool transport in compiled runtimes**: stubs, not bindings.
  `agenomic run` does not launch compiled docker / wasm artifacts.
- **Gate interception**: `agm gate check` evaluates one call you hand it; it
  does not intercept tool calls inside a running agent.
- **Kernel sandbox**: none.
- **Spec 0.2 bump**: deferred; `agm init` still emits `spec_version '0.1'`.
- **`encrypted_full_payload`, KMS keystore, SDK ledger surfaces**: deferred.
- **Shadow / canary governance (Mode 5)**: open.

SDK and CLI defects found while verifying this matrix (2026-09-29). Plan
around them and cite them in the report when they apply:

- Python `instrument_langgraph` / `instrument_langgraph_canonical` record
  nothing on current LangGraph (nodes are not plain callables).
- Python `HttpExporter` ignores `batch_interval_ms`; it flushes on batch size
  with a running loop, or on `flush()` / `aclose()` / `close()`.
- TypeScript `traceAgentRun` / `withTracedRoute` with an `endpoint` reject the
  agent call (`Trace has already been finalized.`) when ingestion fails.
- `agm trace validate` / `agm replay` reject Python traces with tool calls and
  every TypeScript trace (§2).
- `agm replay --runs-per-trace` > 1 is a warned no-op, although
  `--from-ledger` help says replay "stays statistical".

## 8. Failure behavior on the hot path

| Surface | On Agenomic unreachable |
|---|---|
| Python exporters (`Jsonl`, `AtepLocal`, `Http`, `Multi`) | log and swallow |
| Python `Client` in cloud mode (tracking, monitor, RMP, tools, protect) | synchronous request, raises `CloudError` |
| Python `TrackingCallbackHandler` | background worker, counts drops, never raises |
| Python managed prompts (`client.prompts`, `bind_langgraph`; unreleased, §11) | reads and binding calls retry, then raise `registry_unavailable`; an existing thread continues on its cached binding (in memory, or on disk across restarts with `AGENOMIC_PROMPT_CACHE_DIR`); an online `bind_langgraph` made during the outage raises it unless the client knows its workspace (`AGENOMIC_WORKSPACE_ID`), and then serves cached bindings only |
| Python `ExperimentRunner` (unreleased, §11) | the first hello retries, then raises `registry_unavailable`; after it, a failed claim or an outage is logged and the runner keeps serving |
| TypeScript `client.prompts`, `client.bindings` (unreleased, §11) | throw `ApiError` (`transport_error`) at once: no retry, no cached binding |
| `agm prompts`, `agm channels` (unreleased, §11) | exit 6 |
| TypeScript `traceAgentRun` / `withTracedRoute` with `endpoint` | rejects the agent call |
| TypeScript tracking / RMP in cloud mode | throws |
| TypeScript tools / protect / benchmarks without a URL | throws `cloud_required` |
| `agm` ledger `append` | returns after a local fsync (§9) |

## 9. Ledger latency modes

| Mode | `append` returns after | Use |
|---|---|---|
| `best_effort_low_latency` | in-memory enqueue | dev only |
| `durable_low_latency` **(default)** | fsync'd WAL append | **production** |
| `strict_verified` | signed ledger append | high assurance, synchronous |
| `strict_cloud` | never | fails closed; not implemented |

`agm` always uses the default configuration: no flag, environment variable
or config key selects another mode, so `strict_verified` and `strict_cloud`
are unreachable from the CLI today. Two guarantees hold: no event loss (every
event reaches a durable state or an explicit dead-letter), and agents are
never blocked by default.

## 10. Environment variables (verified)

These exist and are read by the CLI:

```
AGENOMIC_API_KEY          cloud API key
AGENOMIC_ENDPOINT         cloud endpoint (default https://api.agenomic.io)
AGENOMIC_PROFILE          runtime profile
AGENOMIC_ENRICH_PROVIDER  direct | cloud | anthropic | openai
AGENOMIC_ENRICH_MODEL     enrichment model override
AGENOMIC_FORMAT           human | json | json-pretty | yaml
AGENOMIC_NO_COLOR         true | false  (plain NO_COLOR is honoured too)
```

**Set them or leave them unset; never set them empty.** An empty
`AGENOMIC_FORMAT` or `AGENOMIC_NO_COLOR` (or `AGENOMIC_NO_COLOR=1`) makes
every `agm` command exit 2, and an empty `AGENOMIC_ENDPOINT` replaces the
default URL with an empty string.

**These do not exist**: do not write them into `.env.example` or deployment
templates: `AGENOMIC_WORKSPACE_ID`, `AGENOMIC_ENVIRONMENT`,
`AGENOMIC_TRACKING_ENABLED`, `AGENOMIC_LEDGER_MODE`, `AGENOMIC_EDITION` (a
cloud server setting, not a CLI or SDK one).

Exceptions, on the unreleased managed prompts branches only (§11):

- Python `Client.from_env()` reads `AGENOMIC_ENDPOINT`, `AGENOMIC_API_KEY`,
  `AGENOMIC_WORKSPACE_ID`, `AGENOMIC_PROMPT_CACHE_DIR` and
  `AGENOMIC_TIMEOUT`, and `agenomic-py prompts import` uses it. Python
  v0.1.3 has no `from_env()`, and the CLI reads none of the last three. A
  project on this branch that binds managed prompts does set
  `AGENOMIC_WORKSPACE_ID`: a `bind_langgraph` made during a registry outage
  needs it (§8).
- The Python experiment runner (`agenomic-py experiment serve`) reads
  `AGENOMIC_RUNNER_TOKEN` and `AGENOMIC_ENDPOINT`.
- `agm channels promote` and `rollback` read `AGENOMIC_WEB_URL`, the web
  app origin, to print an absolute move address.

Other variables in play:

- `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`: read by `agm enrich` for direct
  enrichment.
- `HUGGINGFACE_API_TOKEN` / `HF_TOKEN`, `HUGGINGFACE_ENDPOINT_URL`,
  `HUGGINGFACE_ORG`, `HUGGINGFACE_DEFAULT_MODEL`,
  `HUGGINGFACE_TIMEOUT_SECONDS`: read by the CLI providers, by Python
  `HuggingFaceConfig.from_env()` and by TypeScript `new HuggingFaceClient()`.
- `AGENOMIC_BASE_URL`, `AGENOMIC_API_KEY`: read by `agenomic-py benchmark
  serve` only.
- `AGENOMIC_VERSION`, `AGENOMIC_INSTALL_DIR`: read by `install.sh` only.

Apart from those, both SDK libraries take configuration through constructor
arguments. Python `Client()` with no `base_url` and TypeScript
`new AgenomicClient()` with no `endpoint` / `baseUrl` never attempt HTTP for
tracing. Python `AgenomicClient(endpoint, api_key)` (the async upload client)
has no local mode. If the target project wants env-driven SDK config, it
supplies its own variables: name them after the project, and add them to the
project's `.env.example` with empty values.

## 11. Managed prompts

Verified on 2026-10-06 against the `feat/managed-prompts` branches of
`agenomic-python` (07c58c6), `agenomic-typescript` (e50d632) and
`agenomic-cli` (3d351e3). **Unreleased**: none of this is in `agenomic`
v0.1.3, `@treansai/agenomic-typescript` 0.1.1 or `agm` `v0.3.0-apha.0`.
Workflow: `recipes/langgraph.md` §8.

Python has the full surface. TypeScript reads, renders and binds, and `agm`
reads, publishes, renders and exports. The scanner, the importer, the
LangGraph adapter and the experiment runner are Python only.

In the tables, *local* means Python `Client()` without `base_url` (an
in-process registry), *offline* means no registry at all (a file or a
bundle), and *cloud* means a registry that serves the managed prompt API.

| Python surface (unreleased) | Status |
| --- | --- |
| `client.prompts`: get, render, publish, drafts, aliases | ✅ local, ⚠ cloud |
| `client.prompts.resolve_agent`, `client.bindings` | ✅ local, ⚠ cloud |
| `bindings.counterfactual`, `bindings.report_usage` | ⚠ cloud only |
| `client.channels.get`, `history` (read only) | ✅ local, ⚠ cloud |
| `client.channels.list`, `move_preview` | ⚠ cloud only |
| `client.prompts.export_bundle` | ⚠ cloud only |
| `PromptBundle.load`, `agenomic-py prompts bundle-verify` | ✅ offline |
| `bind_langgraph`, `prompts_for`, `scope_config` | ✅ local, ⚠ cloud |
| `managed_prompt`, `AgentFactory` | ✅ local |
| `agenomic-py prompts scan`, `render`, `digest` | ✅ offline |
| `to_langchain`, `from_langchain` (`langchain` extra) | ✅ offline |
| `agenomic-py prompts import`, `import_report`, `apply_import` | ⚠ cloud only |
| `plan_declarations`, `apply_declarations` (prompts files) | ⚠ cloud only |
| `register_runtime` (live LangChain templates) | ⚠ cloud only |
| Tracking `model.call.started` with `prompt_refs` | ✅ with `config_for` |
| `client.experiments`: create, preflight, launch, reads | ⚠ cloud only |
| `ExperimentRunner`, `agenomic-py experiment serve` | ⚠ cloud only |
| `local_assignment`, `ExperimentRunner.run_trial` | ✅ local |
| `agenomic-py experiment snapshot`, `snapshot_case` | ✅ offline |
| `client.rmp.start(candidate_release_id=...)` | ✅ local, ⚠ cloud |
| Slot or candidate calls; approve, promote, rollback | ❌ |
| Datasets, runner registration, evidence reads | ❌ web app or API |
| Rewriting code to read managed prompts | ❌ by hand |

| TypeScript surface (unreleased) | Status |
| --- | --- |
| `parseExecutionRef`, `parsePromptRef`, `formatPromptRef` | ✅ offline |
| `renderText`, `renderMessages`, `compose`, `renderContent` | ✅ offline |
| `contentDigest`, `manifestDigest`, `scanSecrets` | ✅ offline |
| `threadKey`, `executionKey` (the Python keys) | ✅ offline |
| `readPromptBundleFile` with `expectedBundleDigest` | ✅ offline |
| `client.prompts`: get, resolve, resolveAgent | ⚠ cloud only |
| `client.bindings`: create, get | ⚠ cloud only |
| Bundle signature verification | ❌ digest pin only |
| Publish, drafts, aliases, channels, export | ❌ |
| Scan, import, usage reporting, LangGraph, experiments | ❌ |

| CLI surface (unreleased) | Status |
| --- | --- |
| `agm prompts render <file>`, `render --bundle` | ✅ offline |
| `agm prompts push --dry-run` | ✅ offline |
| `agm prompts list`, `get`, `pull`, `push` | ⚠ cloud |
| `agm prompts render --server`, `export` | ⚠ cloud |
| `agm channels list`, `history` | ⚠ cloud |
| `agm channels promote`, `rollback` | ⚠ hand-off only |
| Drafts, alias moves, scan, import, experiments | ❌ |
| YAML prompt files | ❌ JSON only |

Evidence, all on macOS arm64:

- Offline and local rows, run on 2026-10-06: every Python snippet of
  `recipes/langgraph.md` §8, one offline trial per arm through
  `local_assignment`, and examples 12 to 17 of `agenomic-python`.
- One signed bundle, exported by the Python local registry, was checked by
  `agenomic-py prompts bundle-verify`. TypeScript loaded it with its digest
  pin, and `agm prompts render --bundle` rendered it, pinned or with
  `--trust-key`. All three gave the same messages, and an unpinned load
  was refused with `bundle_untrusted_key`.
- TypeScript `threadKey` and `executionKey` gave the Python keys.
- Cloud rows rest on each package's tests against a fake registry, with
  three exceptions run against a local build of Agenomic Cloud on
  2026-10-06: `client.experiments` with `agenomic-py experiment serve`, an
  `ExperimentRunner` serving a 300-trial experiment, and the
  `agm channels promote` hand-off.
- Never run against a live registry: the Python import calls and
  `agenomic-py prompts import`, the TypeScript resources, and the other
  `agm` cloud commands.

`bind_langgraph` was tested on these points only. The install range is
`langgraph>=1.0.10,<2`; another version runs with one
`AgenomicUntestedVersionWarning`.

| langgraph | langchain-core | Python, macOS arm64 |
| --- | --- | --- |
| 1.2.11 | 1.6.3 | 3.10, 3.11 |
| 1.0.10 (`langgraph-prebuilt` 1.0.8) | 1.6.3 | 3.10, 3.13 |

These points were run locally only: no CI cell has run, and Linux, Windows
and the other Python versions are unverified.

Consequences for planning:

- Production agents run with a `read` key. `bind_langgraph` refuses a key
  with all scopes, `write` or `admin` (`privileged_credential`) unless the
  project opts in.
- An API key binds, resolves or exports only a release that is approved, in
  production or the current target of one of the agent's channels. Another
  release gets `session_required` (`error.reason == "ungoverned_release"`),
  and a rejected or rolled back release gets `release_not_bindable`, whoever
  asks.
- Approving, promoting and rolling back a release, and moving an alias, need
  a signed-in person in Agenomic Cloud. Every API key gets `session_required`,
  through the remote MCP tools too, so never plan an SDK, CLI or CI step that
  promotes prompts.
- `agm channels promote` and `rollback` read a move preview, print where a
  signed-in person completes the move, and exit 0. Exit 0 means the preview
  was read, never that a channel moved.
- Threads keep their first release: a promotion reaches new threads only.
- TypeScript loads an offline bundle only with `expectedBundleDigest`.
  Verify the signed export once (`agm prompts export --trust-key` or
  `agenomic-py prompts bundle-verify`), then ship its
  `prompt_bundle_digest` with the file.
- Experiments need Agenomic Cloud with the `prompts.experiments` capability
  and a runner that a workspace owner registered. Trials run on the
  project's machines with its own model credentials, and the runner serves
  Python LangGraph agents only.
