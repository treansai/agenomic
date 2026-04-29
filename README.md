# AgentLock

Umbrella workspace for the AgentLock platform. Each subdirectory is a git submodule pointing to its own repository.

## Components

| Path | Repo | Description |
|---|---|---|
| `agentlock-spec/` | [agentlock-spec](https://github.com/treansai/agentlock-spec) | Protocol specification, RFCs, schemas |
| `agentlock-cli/` | [agentlock-cli](https://github.com/treansai/agentlock-cli) | Reference CLI (Rust) |
| `agentlock-cloud/` | _private_ | Cloud services (private) |
| `agentlock-python/` | [agentlock-python](https://github.com/treansai/agentlock-python) | Python SDK |
| `agentlock-typescript/` | [agentlock-typescript](https://github.com/treansai/agentlock-typescript) | TypeScript SDK |
| `agentlock-web/` | [agentlock-web](https://github.com/treansai/agentlock-web) | Web frontend |
| `agentlock-examples/` | [agentlock-examples](https://github.com/treansai/agentlock-examples) | Example agents and demos |

## Clone

```sh
git clone --recurse-submodules https://github.com/treansai/agentlock.git
```

Or after a regular clone:

```sh
git submodule update --init --recursive
```

Access to `agentlock-cloud` requires permissions on the private repository.

## Update submodules

```sh
git submodule update --remote --merge
```
