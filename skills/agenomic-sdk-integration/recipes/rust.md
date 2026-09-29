# Rust recipe

Applies when `agm init --dry-run` reports `runtime.runtime_kind: rust`.

## There is no Rust tracing SDK

Be direct with the user about this. The Rust crates in this repository
(`agenomic-atep`, `agenomic-track`, `agenomic-ledger-local`, `agenomic-hash`,
…) are **CLI-internal libraries**, not a published instrumentation SDK. There
is no `trace_agent_run` macro and no published `agenomic-trace` crate on
crates.io.

Do not add a dependency on a crate you have not verified exists. Check first:

```sh
cargo search agenomic
```

Two supported paths follow.

## Path A: emit the trace envelope yourself (recommended)

Write JSONL and let the CLI do everything downstream. The CLI reads
`trace_id`, `agent_id`, `input`, `output`, `tool_calls[].name` (plus optional
`arguments`, `result`, `human_approval_present`) and `metadata`, and ignores
the rest. The spec v0.1 envelope names the tool `tool` and the output
`final_output`, so the struct below writes both names: the file then
satisfies the spec and `agm trace validate`.

```rust
use serde::Serialize;

#[derive(Serialize)]
struct ModelCall<'a> {
    provider: &'a str,
    model: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_hash: Option<String>,
    latency_ms: u64,
    status: &'a str,            // success | error | aborted | timeout
}

#[derive(Serialize)]
struct ToolCall<'a> {
    tool: &'a str,
    name: &'a str,              // same value as `tool`; the CLI requires it
    protocol: &'a str,          // http | mcp | grpc | local
    #[serde(skip_serializing_if = "Option::is_none")]
    input_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_hash: Option<String>,
    latency_ms: u64,
    status: &'a str,
}

#[derive(Serialize)]
struct TraceEnvelope<'a> {
    schema_version: &'a str,    // "agenomic-trace/v0.1"
    trace_id: String,
    run_id: String,
    agent_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    release: Option<&'a str>,
    timestamp: String,          // RFC 3339
    input: serde_json::Value,   // { "type": "json", "payload_inline": … }
    model_calls: Vec<ModelCall<'a>>,
    tool_calls: Vec<ToolCall<'a>>,
    final_output: serde_json::Value,  // { "payload_inline": … }
    output: serde_json::Value,        // same value; the CLI reads `output`
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    duration_ms: u64,
}
```

Append one JSON object per line to `.agenomic/traces.jsonl`, then validate.
This is the contract, so run it in CI:

```sh
agm trace validate .agenomic/traces.jsonl
```

The schemas are embedded in the binary; the release archives ship only the
`agenomic` and `agm` executables. Test the field set against the installed
CLI with a one-line fixture before shipping.

Hashes are BLAKE3 over canonical CBOR in the Python SDK. If you only need the
CLI to accept the trace, any stable hash works; for cross-language
comparability, match the canonicalization (the Python SDK's `canonical_cbor`
and `blake3_hex`). `agm hash` hashes bundles, not trace payloads.

## Path B: shell out to the CLI

For low-volume or batch agents, skip in-process tracing entirely:

```rust
use std::process::Command;

Command::new("agm")
    .args(["ledger", "append", "--event", event_path])
    .status()?;
```

The event is JSON with `agent_id`, `run_id` and `event_type` required, and
optional `payload`, `event_id`, `session_id`, `genome_hash`, `release_id`,
`turn_id`, `turn_sequence_number`. Run `agm ledger init` once first; `append`
does not seal blocks (`agm ledger seal` does). This gives ledger durability
without any Rust integration work. It costs a process spawn per event:
acceptable for a batch job, not for a hot path.

## Instrumentation boundaries

Same four as everywhere (`references/architecture.md`). Rust has no
contextvar equivalent; thread the recorder explicitly:

```rust
pub struct RunRecorder { /* model_calls, tool_calls, started_at */ }

pub async fn run(input: Input, rec: &mut RunRecorder) -> Result<Output> { … }
```

Passing `&mut RunRecorder` through the call graph is verbose but honest: it
makes the trace boundary visible in the type system, and it cannot silently
lose events across a `tokio::spawn` the way an implicit context can.

## What still works without an SDK

Everything CLI-side, which is most of the platform: `validate`, `build`,
`compile`, `hash`, `diff`, `replay`, `attest`, `verify`, `atep *`,
`ledger *`, `track *`, `rmp` / `review` / `monitor` / `protect`,
`governance *`, `gate check`, `evidence *`, `cloud *`.

Record "no in-process SDK" as a coverage gap; do not record it as a failure.
A Rust agent emitting valid JSONL has full access to replay; the ledger and
evidence are fed by `agm ledger append` or by `agm track` / `agm rmp` sessions
started with `--ledger`, not by the JSONL file.
