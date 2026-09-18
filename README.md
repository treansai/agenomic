# Agenomic

Umbrella workspace for the Agenomic platform. Each subdirectory is a git submodule pointing to its own repository.

## Components

| Path | Repo | Description |
|---|---|---|
| `agenomic-spec/` | [agenomic-spec](https://github.com/treansai/agenomic-spec) | Protocol specification, RFCs, schemas |
| `agenomic-cli/` | [agenomic-cli](https://github.com/treansai/agenomic-cli) | Reference CLI (Rust) |
| `agenomic-python/` | [agenomic-python](https://github.com/treansai/agenomic-python) | Python SDK |
| `agenomic-typescript/` | [agenomic-typescript](https://github.com/treansai/agenomic-typescript) | TypeScript SDK |
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

## License

Copyright (C) 2026 Agenomic Contributors.
The Agenomic Community edition is distributed under the GNU Affero General
Public License v3.0 (`AGPL-3.0-only`). The full text is in
[LICENSE](LICENSE); the operational rules are in
[docs/legal/open-source-license.md](docs/legal/open-source-license.md).

[![AGPL v3](https://img.shields.io/badge/license-AGPL--3.0--only-blue.svg)](LICENSE)

Component licenses differ on purpose. The SDKs and the specification stay
permissive so they can be embedded in any application:

| Component | License |
|---|---|
| This repository, `crates/agenomic-fingerprint`, `crates/agenomic-metrics` | `AGPL-3.0-only` |
| `agenomic-cli/` | `AGPL-3.0-only` |
| `agenomic-codedrift/` | `AGPL-3.0-only` |
| `agenomic-examples/` | `AGPL-3.0-only` |
| `agenomic-python/` | `Apache-2.0` |
| `agenomic-typescript/` | `MIT` |
| `agenomic-spec/` | `Apache-2.0` |

Agenomic Cloud and Enterprise components are proprietary, live in private
repositories and are not covered by any of these licenses. Third-party
dependencies keep their own licenses; each Core release ships a
`THIRD_PARTY_LICENSES.md` report.
