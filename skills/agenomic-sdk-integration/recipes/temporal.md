# Temporal recipe

Temporal has **topology detection but no runtime adapter**. `agm init`
recovers `@workflow.defn` classes and `@workflow.signal` handlers (Python)
into `workflows/<slug>.yaml` (plus `system.yaml` when a LangGraph graph
coexists), provided a `pyproject.toml` or other manifest exists.
Instrumentation is manual.

## 1. Determinism comes first

Temporal replays workflow code. Anything non-deterministic inside a workflow
function corrupts replay, including tracing side effects.

**Rule: never instrument inside the workflow function. Instrument activities.**

```python
# ✗ WRONG: breaks Temporal determinism
@workflow.defn
class ClaimWorkflow:
    @workflow.run
    async def run(self, claim: dict) -> dict:
        recorder.record_tool_call(...)      # side effect during replay
```

```python
# ✓ RIGHT: activities are not replayed, they are recorded
@activity.defn
async def call_model(prompt: str) -> str:
    ...
```

File I/O, `time.perf_counter()`, ULID generation and exporter writes are all
non-deterministic. Keep every one of them in activities.

## 2. Where the run boundary goes

Two valid choices; pick deliberately and record which:

**A. One run per workflow execution** (preferred). Wrap the client-side
`start_workflow` / `execute_workflow` call:

```python
@trace_agent_run("agent://acme/claims", exporter=exporter)
async def submit_claim(claim: dict) -> dict:
    return await client.execute_workflow(
        ClaimWorkflow.run, claim,
        id=f"claim-{claim['id']}", task_queue="claims",
    )
```

Clean boundary, matches the business unit of work. Loses in-workflow detail
unless activities report back.

**B. One run per activity.** Wrap each activity as its own run and correlate
with the workflow id as a label:

```python
@activity.defn
async def call_model(prompt: str) -> str:
    info = activity.info()

    @trace_agent_run("agent://acme/claims-model", exporter=exporter)
    async def _traced() -> str:
        recorder = current_recorder()
        if recorder is not None:
            recorder.add_label("temporal.workflow_id", info.workflow_id)
            recorder.add_label("temporal.run_id", info.workflow_run_id)
            recorder.add_label("temporal.activity_id", info.activity_id)
        return await do_call(prompt)

    return await _traced()
```

Full detail, more traces to correlate. Choose B when activities are the
interesting behavior; A when the workflow is.

**Always label with `activity.info()`.** Without the workflow id, activity
traces cannot be reassembled into an execution.

## 3. Retries

Temporal retries activities automatically. Each attempt is a separate
execution and should be a separate trace. Record the attempt number:

```python
recorder.add_label("temporal.attempt", str(info.attempt))
```

Attempt counts are a first-class failure signal: an activity on attempt 5 is
what loop and failure detection should see.

## 4. Signals, queries, timers, child workflows

| Concept | Handling |
|---|---|
| Signal | Detected into `workflows/<slug>.yaml`. Record receipt from an activity, never from the handler. |
| Query | Read-only; do not instrument. |
| Timer | Not instrumented: `workflow.sleep` is deterministic and must stay untouched. |
| Child workflow | Its own run. Label with the parent workflow id. |
| Continue-as-new | New workflow run; link via a label. |

## 5. Human approval

Temporal's usual pattern (a signal that gates progress) maps to a tool call
with `requires_human_approval=True`. Record it from the activity that acts on
the approval, with `approval_present` reflecting whether the signal arrived.
For enforcement rather than recording, use `agm gate check` at the effect
site (exit 18 = `RequireHumanApproval`).

## 6. Exporter lifetime

Workers are long-lived. Construct the exporter once at worker startup and
close it on shutdown, not per activity:

```python
exporter = JsonlExporter(".agenomic/traces.jsonl")
try:
    await worker.run()
finally:
    exporter.close()
```

A per-activity `JsonlExporter` leaks a file handle on every invocation.

## 7. Verify

```sh
agm validate . --level strict
wc -l < .agenomic/traces.jsonl                 # one envelope per traced run
```

Cross-check the trace count against Temporal's own history for one workflow.
A mismatch means a code path is not on the instrumented route.

## Coverage summary

| Boundary | Support |
|---|---|
| Workflow topology | ✅ `@workflow.defn` / `@workflow.signal` detected |
| Workflow start/end | ⚠ manual, client side |
| Activity start/end | ⚠ manual, in-activity |
| Signals | ⚠ manual |
| Retries | ⚠ manual label |
| Timers / queries | ❌ by design |
| Child workflows | ⚠ manual correlation |
