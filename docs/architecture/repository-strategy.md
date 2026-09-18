# Repository strategy

Agenomic is open core. This document says what is public, what is private,
which repository owns what, and how the commercial product consumes the
open-source Core without either side leaking into the other.

## What is public

`github.com/treansai/agenomic` (this repository) and its component
repositories:

| Component | Repository | Licence | Contents |
|---|---|---|---|
| Specification | `agenomic-spec` | Apache-2.0 | agent-bundle spec, genome.yaml, agent.lock, trace and attestation schemas, RFCs |
| CLI | `agenomic-cli` | AGPL-3.0-only | `agenomic` / `agm` binaries, local validate, replay, diff, RMP client |
| Python SDK | `agenomic-python` | Apache-2.0 | tracing and ATEP instrumentation |
| TypeScript SDK | `agenomic-typescript` | MIT | tracing instrumentation |
| Examples | `agenomic-examples` | AGPL-3.0-only | example agents |
| Code drift | `agenomic-codedrift` | AGPL-3.0-only | drift detection job |
| Shared crates | `crates/agenomic-fingerprint`, `crates/agenomic-metrics` | AGPL-3.0-only | behavioural fingerprints, metrics |

The Community edition is AGPL-3.0-only. The SDKs and the specification stay
permissive (`Apache-2.0`, `MIT`) because they are embedded in user
applications and consumed by third-party generators; relicensing them would
push a copyleft obligation onto every instrumented application.
`scripts/license/` is the machine-readable source of truth and the CI gate.

Everything a contributor needs is here: clone, `cargo test`, `cargo build`,
the CLI and SDK test suites. No account, private registry, Cloud secret or
Agenomic service is required.

## What is private

The managed control plane (Agenomic Cloud), the product web application,
the governance agents and the deployment infrastructure live in private
repositories of the same organisation. They **consume** this repository;
they never push to it.

They are proprietary: no public package, manifest or artifact of this
repository may declare them, and the OSS licence checks never reclassify a
private component as AGPL.

## Who owns what

- This repository owns the Core: the specification and the shared crates,
  their versions and their releases (`core-vX.Y.Z` tags).
- The private repositories own the commercial extensions and every
  deployment concern.
- A change to a shared crate lands here first, through a public pull
  request, and reaches Cloud only through a Core release.

## How Cloud consumes Core

Cloud pins an explicit, immutable Core: a `core.lock` file declaring the
release version and the commit SHA, mirrored by git dependencies pinned to
that commit (`rev = ...`, never a branch or tag). A bot in the Cloud
repository polls the releases of this repository and opens an upgrade pull
request per release; that pull request is reviewed and tested on the Cloud
side. Nothing in this repository is triggered by, or has credentials for,
any private repository.

## How Core is upgraded

1. Public pull request → CI (`ci-core`) → merge to `main`.
2. Maintainer bumps the crate versions and tags `core-vX.Y.Z` (rulesets:
   tags are immutable, only repository admins create them).
3. `release-core` verifies the tag is on `main`, runs tests, the proprietary
   leak scan and the secret scan, and publishes the release with a source
   archive, checksums, an SPDX SBOM, the third-party licence report and a
   build provenance attestation.
4. Cloud upgrades through its bot; a broken Cloud is fixed on the Cloud side
   or with a Core patch release. A published Core release is never rewritten.

## Breaking changes

Semantic versioning. A breaking change to a crate API, a schema or an event
is a major version, announced in the release notes under "Breaking changes".
Cloud treats major and minor upgrades as human-reviewed; patch upgrades may
be merged automatically once all checks pass.

## Stability

- Stable: the specification schemas, the CLI command surface, the SDK public
  APIs, the shared crates' public items.
- Unstable: anything under `examples/`, `prompts/`, `proto/`, and internal
  modules not re-exported.

## Contributing

```bash
git clone --recurse-submodules https://github.com/treansai/agenomic.git
cargo test --workspace                 # shared crates
./scripts/ci/proprietary-leak-check.sh # what CI runs on your pull request
./scripts/license/check-all.sh          # licence metadata and OSS/Cloud boundary
```

Pull requests from forks run the same checks with no secret. Reviews follow
`.github/CODEOWNERS`.

By contributing to an Agenomic Community repository, you agree that your
contribution may be distributed under that repository's licence, as recorded
in `scripts/license/packages.tsv`. There is no contributor licence agreement
and no copyright assignment; contributors keep their copyright.
