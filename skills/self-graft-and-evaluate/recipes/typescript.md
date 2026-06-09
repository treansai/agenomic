# TypeScript recipe — self-graft instrumentation

Apply this only if `agenomic init --dry-run` reports `language: typescript`
(detected from `package.json`).

## Install

```sh
npm install @agenomic/sdk
```

The SDK is tree-shakeable; install only the provider integrations your agent
imports.

## Instrument the entry point

```ts
// agent.ts — BEFORE
import Anthropic from "@anthropic-ai/sdk";

const client = new Anthropic();

export async function run(input: Record<string, unknown>) {
  const response = await client.messages.create({ /* ... */ });
  return { answer: response.content[0].text };
}
```

```ts
// agent.ts — AFTER
import Anthropic from "@anthropic-ai/sdk";
import { instrumentAnthropic } from "@agenomic/sdk/integrations/anthropic";
import { traceAgentRun } from "@agenomic/sdk/trace";

const client = instrumentAnthropic(new Anthropic());

export const run = traceAgentRun(
  { agentId: "<your-agent-id>" },
  async (input: Record<string, unknown>) => {
    const response = await client.messages.create({ /* ... */ });
    return { answer: response.content[0].text };
  },
);
```

`traceAgentRun` is a higher-order function: it returns a wrapped version of
`run` that opens a recorder for each call. Type signatures are preserved.

## Other providers

| Provider | Wrapper |
|---|---|
| OpenAI | `@agenomic/sdk/integrations/openai` → `instrumentOpenAI` |
| Vercel AI SDK | `@agenomic/sdk/integrations/ai-sdk` → `instrumentAiSdk` |
| MCP client | `@agenomic/sdk/integrations/mcp` → `instrumentMcpClient` |

## Verify the recipe applied

```sh
npx tsx -e "import('./agent').then(m => m.run({ping: 'pong'}))"
ls agent-bundle/.runs/   # should contain at least one trace envelope
```

If you use Next.js / a worker runtime, ensure the wrapped `run` is the export
the framework actually invokes (server action, route handler, etc.).
