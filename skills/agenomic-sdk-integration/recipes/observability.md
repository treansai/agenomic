# Observability recipe: coexistence

## The rule

**Never remove existing observability.** OpenTelemetry, LangSmith, Langfuse,
Datadog and the project's own logs answer "is the system healthy". Agenomic
answers "did the agent behave as declared, and can we prove it". Different
questions, different retention, different audience.

Ripping out a working tracer to install Agenomic is a regression, however
tidy the diff looks.

## There is no automatic bridge into Agenomic

No OTel→Agenomic exporter ships in either SDK. Anyone who tells the user
otherwise is guessing. The supported approach is **label-based correlation**:
carry the existing trace id into the Agenomic trace so an operator can pivot
between the two systems.

The other direction exists, narrowly. A Python canonical run started with
`start_run(agent_id, ..., tracer=otel_tracer)` (extra `agenomic[otel]`)
emits one OTel span per canonical event, with `gen_ai.*` attributes (system,
request model, token usage, tool name). It covers canonical runs only, not
`@trace_agent_run`, and it does not replace the correlation labels below.

## OpenTelemetry

```python
from opentelemetry import trace as otel_trace
from agenomic.trace.context import current_recorder

def link_otel() -> None:
    span = otel_trace.get_current_span()
    ctx = span.get_span_context()
    recorder = current_recorder()
    if recorder is None or not ctx.is_valid:
        return
    recorder.add_label("otel.trace_id", format(ctx.trace_id, "032x"))
    recorder.add_label("otel.span_id", format(ctx.span_id, "016x"))
```

Call it once at the top of the traced function. Now an Agenomic trace links to
its OTel trace and back.

```ts
import { trace as otelTrace } from "@opentelemetry/api";

const ctx = otelTrace.getActiveSpan()?.spanContext();
const trace = client.createTrace({
  agentId: "agent://acme/claims",
  input: payload,
  runMetadata: ctx ? { "otel.trace_id": ctx.traceId, "otel.span_id": ctx.spanId } : undefined,
});
```

The TypeScript builder has no custom event type and no label API once the
trace exists, so the link goes in at creation, through `runMetadata` (or
`runMetadata` on `traceAgentRun` when the span is already known there).

Keep the label keys identical across services (`otel.trace_id`), or the
pivot stops working the moment two teams choose different names.

## LangSmith / Langfuse

Both keep their own run ids. Record them as labels:

```python
recorder.add_label("langsmith.run_id", str(run_id))
```

They overlap with Agenomic on model-call capture. That is fine: keep both.
LangSmith is for prompt iteration; Agenomic traces feed replay, the ledger and
evidence. Do not disable one to avoid "duplicate" data; they are not the same
data.

## Datadog / New Relic / CloudWatch

Leave APM alone. Add the Agenomic trace id to the log context so a log line
can be traced back to a run:

```python
logger = logger.bind(agenomic_run_id=recorder.run_id)   # structlog
```

For metrics, export a small set from the Agenomic side (runs started,
completed, failed; export queue depth; ledger backlog) through the existing
metrics client. Health signals belong where the on-call already looks.

## Custom JSON / JSONL logs

If the project already writes structured per-run logs, they may be
convertible to the CLI trace shape (capability matrix §2) rather than
instrumented from scratch.

Feasible when the logs already contain: a stable run id, model calls with
provider and model, tool calls with names, and a start/end timestamp.

```sh
# after conversion
agm trace validate converted-traces.jsonl
agm replay . converted-traces.jsonl
```

`agm governance cluster` is a different input: FlaggedTrace records
(`trace_id`, `agent_id`, `skill`, `signal`, optional snippets), one per
flagged behavior, not trace envelopes.

Not feasible when runs are not correlatable or model calls are not
distinguishable: say so and instrument properly instead. A converter that
silently drops half the calls produces a coverage matrix full of confident
lies.

## Emitting health metrics

Telemetry that fails silently is worse than no telemetry. Expose:

| Metric | Source |
|---|---|
| runs traced | count of envelopes exported |
| export failures | exporter error log count |
| queue depth | `agm ledger queue status` |
| dead letters | `agm ledger queue dead-letter list` |
| ledger backlog | `agm ledger status` |

The ledger commands read `<cwd>/.agenomic/ledger` unless given `--store`.
`scripts/validate-integration.sh` runs `agm ledger status` once. In
production, poll all of them: that is how you find out the exporter has been
failing for a week.

## Sampling

Sampling is permitted only for high-volume, non-critical observability events.

**Never sample:** policy violations, approvals, security events, release
decisions, critical failures. These are evidence. A sampled audit trail is not
an audit trail, and the gap will surface exactly when someone needs the
missing record.
