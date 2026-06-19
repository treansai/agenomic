# Agenomic Skills

Reusable, agent-readable instructions that any AI agent (or its runtime) can
load to perform a well-scoped, multi-step task against the Agenomic platform.

These skills are **runtime-agnostic**. They are plain Markdown + YAML files,
not tied to Claude Code, OpenAI Assistants, or any specific harness. An agent
loads `SKILL.md`, follows the procedure, and uses the templates/scripts
shipped alongside it.

## Skill format

```
skills/<skill-name>/
├── SKILL.md           # Agent-readable procedure (entry point)
├── manifest.yaml      # Machine-readable metadata (name, inputs, outputs)
├── recipes/           # Per-language or per-runtime variants (optional)
├── templates/         # File templates the skill writes into the target repo
└── scripts/           # Idempotent shell wrappers the skill invokes
```

Every skill MUST:

- declare its inputs/outputs in `manifest.yaml`,
- be idempotent (safe to run twice),
- avoid network I/O unless explicitly listed in `manifest.yaml`,
- emit a final structured summary the caller can parse.

## Available skills

| Skill | Purpose |
|---|---|
| [`self-graft-and-evaluate`](./self-graft-and-evaluate/) | Let an agent register itself as an Agenomic bundle and run its own replay/contract checks. Spec-version aware: single agents (genome), staged workflows, and multi-agent systems (spec v0.2, RFC 0009). |

## Adding a new skill

1. Create `skills/<name>/SKILL.md` with a clear procedure.
2. Add `manifest.yaml` (see existing skills for the schema).
3. Keep recipes small — one file per supported language/runtime.
4. Templates are copied verbatim; do not embed agent-specific data in them.
5. Scripts must work with `bash` and `set -euo pipefail`.
