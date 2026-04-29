# AGENTS.md — AgentLock Project Instructions

You are working on AgentLock, an open-core platform for agent-native release, replay, diff, rollback and compliance.

## Product vision

AgentLock turns every AI agent release into a portable, signed, replayable and auditable artifact.

The core idea is not "Git for agents".
The core idea is: version the proof of agent behavior.

AgentLock has three layers:

1. Open standard:
   - agent-bundle specification
   - genome.yaml
   - agent.lock
   - behavior contracts
   - trace event schema
   - release attestation schema

2. Open source developer tools:
   - Rust CLI
   - Python SDK
   - TypeScript SDK
   - local replay
   - local diff
   - example agents

3. Paid enterprise platform:
   - cloud registry
   - distributed replay
   - signed attestations
   - AI Act evidence packs
   - RBAC, SSO, audit logs
   - approval workflows
   - enterprise connectors

## Engineering principles

- Prefer small, well-tested modules.
- Do not over-engineer the first version.
- Keep public repositories clean, documented and usable.
- Avoid vendor lock-in where possible.
- All schemas must be explicit and versioned.
- All examples must avoid real personal data.
- Do not hardcode secrets.
- Add useful README files.
- Add tests for critical behavior.
- Use deterministic checks where possible.
- Be honest about LLM non-determinism: replay compares distributions, not absolute truth.

## Naming conventions

- Product name: AgentLock
- Bundle format: agent-bundle
- Main config: genome.yaml
- Lockfile: agent.lock
- CLI binary: agentlock
- Python package: agentlock
- TypeScript package: @agentlock/sdk

## Security rules

- Never log API keys.
- Never include real customer data in examples.
- Support redaction hooks for traces.
- Treat prompts, tool outputs and retrieved documents as sensitive.
- Prefer explicit allowlists over denylists.
- Include audit-friendly metadata in all generated artifacts.

## Development expectations

When implementing a task:

1. Inspect the existing repo.
2. Create or update the minimal required files.
3. Add tests.
4. Run formatting and tests when possible.
5. Update README.
6. Summarize what changed and what remains.
