# Rust recipe — self-graft instrumentation

Apply this only if `agenomic init --dry-run` reports `language: rust`
(detected from `Cargo.toml`).

## Add the crate

```toml
# Cargo.toml
[dependencies]
agenomic-trace = "0.1"
```

`agenomic-trace` is the runtime crate; it pulls in the canonical hasher and
the trace recorder. It does NOT pull in any LLM client by default.

## Instrument the entry point

```rust
// src/agent.rs — BEFORE
pub async fn run(input: Input) -> anyhow::Result<Output> {
    let resp = client.complete(prompt).await?;
    Ok(Output { answer: resp.text })
}
```

```rust
// src/agent.rs — AFTER
use agenomic_trace::{trace_agent_run, ModelClient};

pub async fn run(input: Input) -> anyhow::Result<Output> {
    trace_agent_run("your-agent-id", async {
        let resp = client.instrument().complete(prompt).await?;
        Ok(Output { answer: resp.text })
    })
    .await
}
```

`trace_agent_run` is an async wrapper that opens a recorder for the duration
of the future. `client.instrument()` is provided by the `ModelClient` trait
impls shipped for each supported provider.

## Provider impls

| Provider | Feature flag |
|---|---|
| `anthropic-sdk` | `agenomic-trace = { version = "0.1", features = ["anthropic"] }` |
| `async-openai` | `features = ["openai"]` |
| Custom HTTP client | implement `ModelClient` yourself — it is a 3-method trait |

## Verify the recipe applied

```sh
cargo run --release -- --input examples/ping.json
ls agent-bundle/.runs/   # should contain at least one trace envelope
```

If `.runs/` is empty, check that `trace_agent_run` is awaited on the actual
call path your binary exercises — a `#[tokio::main]` wrapper that calls
`run(...).await` is the simplest validation.
