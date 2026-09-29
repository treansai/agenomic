# Providers recipe

## What to capture

For every model call:

| Field | Why |
|---|---|
| `provider` | drift attribution |
| `model` | the primary drift signal |
| `fingerprint` | revision / system fingerprint: the *silent* drift signal |
| `temperature` | replay fidelity |
| `prompt_hash` | comparison without storing the prompt |
| `output_hash` | comparison without storing the output |
| `latency_ms` | performance regression |
| `cost_estimate` | budget tracking |
| `status` | `success` / `error` / `aborted` / `timeout` |

**Never** capture the API key. Never put raw prompts in the ledger.

`fingerprint` is the field integrations most often skip and most often need:
a provider silently reissuing `gpt-4o` or a local `llama3.1:8b` pull changing
underneath you is invisible without it.

## Adapters that exist

```python
from agenomic.integrations import (
    instrument_openai, instrument_openai_async,
    instrument_anthropic, instrument_anthropic_async,
    instrument_huggingface, trace_huggingface_call,
)
client = instrument_openai(OpenAI())
```

```ts
import { instrumentOpenAI, instrumentHuggingFace } from "@treansai/agenomic-typescript";
```

| Provider | Python | TypeScript |
|---|:--:|:--:|
| OpenAI | ✅ | ✅ |
| Anthropic | ✅ | ❌ manual |
| Hugging Face | ✅ `instrument_huggingface`, `trace_huggingface_call` | ✅ full provider surface |

## Everything else is manual

Azure OpenAI, Bedrock, Vertex, Mistral, Cohere, Ollama, vLLM, custom
OpenAI-compatible endpoints. Use the manual pattern in
[`python.md`](python.md#providers-without-an-adapter) or `addModelCall` in
TypeScript.

### Azure OpenAI

`AzureOpenAI` from the `openai` package exposes the same
`chat.completions.create`, so `instrument_openai` usually works. Verify with a
smoke run. The adapter records `provider="openai"` and the deployment name as
`model`; if the distinction matters, record manually with
`provider="azure-openai"` and both the deployment name and the model name.

### AWS Bedrock

```python
response = bedrock.invoke_model(modelId=model_id, body=body)
recorder = current_recorder()
if recorder is not None:
    recorder.record_model_call(ModelCall(
        provider="bedrock",
        model=model_id,                   # e.g. anthropic.claude-3-5-sonnet-20241022-v2:0
        prompt_hash=blake3_hex(canonical_cbor({"body": body})),
        latency_ms=elapsed_ms,
        region=region,                    # ModelCall accepts extra fields
    ))
```

The Bedrock model id embeds the version, which is good. Record the region
too; the same id can behave differently across regions.

### Local models (Ollama, vLLM, llama.cpp)

Pin the revision. Without it, a model pull is undetectable drift:

```python
ModelCall(
    provider="ollama",
    model="llama3.1:8b",
    fingerprint="sha256:<digest from /api/show>",
    temperature=0.0,
)
```

Record the endpoint as metadata, never as part of the agent identity: the
same agent runs against different endpoints per environment.

For Hugging Face, resolve the commit once and record it:
`HuggingFaceClient.resolve_model_metadata(model_id, revision)` returns
`resolved_commit` in Python, and `lockModel` returns a lock record with
`resolvedCommit` in TypeScript. Neither pins the inference call itself, and
neither adapter sets `fingerprint`. There is no equivalent for other local
runtimes, so do it by hand.

## Declaring providers in the genome

`agm init` infers providers and models from client construction and
dependency manifests. Verify the inferred list: dead code and test fixtures
produce false positives, and a provider reached through a factory produces
false negatives.

Record in the genome: provider, model, revision, parameters, tool-use
configuration. Reference credentials by variable name, never by value. If a
model id appears in the genome that the agent no longer calls, remove it;
`agm diff` will otherwise report phantom model changes forever.

## Provider-agnostic agents

When the provider is chosen at runtime, record what actually ran, not what
might have:

```python
ModelCall(provider=resolved_provider, model=resolved_model, ...)
```

Declare every reachable provider in the genome and add a behavior-contract
rule constraining the set. Drift detection then flags a call to an undeclared
provider, which is exactly the failure mode a configurable provider
introduces.

## Streaming

Record on completion, once, with the assembled output. Do not record per
chunk: it inflates the call count and makes latency meaningless. Capture
time-to-first-token as metadata if it matters.

## Tool-use / function-calling

When the model requests a tool, that is one model call (the request) followed
by one tool call (the execution) and usually a second model call (the
result). Record all three. Collapsing them loses the causal chain that makes
a trajectory auditable.
