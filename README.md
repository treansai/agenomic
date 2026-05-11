# Agenomic

Umbrella workspace for the Agenomic platform. Each subdirectory is a git submodule pointing to its own repository.

## Components

| Path | Repo | Description |
|---|---|---|
| `agenomic-spec/` | [agenomic-spec](https://github.com/treansai/agenomic-spec) | Protocol specification, RFCs, schemas |
| `agenomic-cli/` | [agenomic-cli](https://github.com/treansai/agenomic-cli) | Reference CLI (Rust) |
| `agenomic-cloud/` | _private_ | Cloud services (private) |
| `agenomic-python/` | [agenomic-python](https://github.com/treansai/agenomic-python) | Python SDK |
| `agenomic-typescript/` | [agenomic-typescript](https://github.com/treansai/agenomic-typescript) | TypeScript SDK |
| `agenomic-web/` | [agenomic-web](https://github.com/treansai/agenomic-web) | Web frontend |
| `agenomic-examples/` | [agenomic-examples](https://github.com/treansai/agenomic-examples) | Example agents and demos |

## Clone

```sh
git clone --recurse-submodules https://github.com/treansai/agenomic.git
```

Or after a regular clone:

```sh
git submodule update --init --recursive
```

Access to `agenomic-cloud` requires permissions on the private repository.

## Update submodules

```sh
git submodule update --remote --merge
```
