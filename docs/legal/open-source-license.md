# Open source licence

Operational reference. It says which licence applies to what, where the texts
live and what to do when a package is added. It is not legal advice and it
does not interpret the licences; the licence texts govern.

## Community edition

The Agenomic Community edition is distributed under the GNU Affero General
Public License v3.0, SPDX identifier `AGPL-3.0-only`. The canonical text is
[`LICENSE`](../../LICENSE) at the root of this repository, byte-identical in
every AGPL-licensed Community repository. `AGPL-3.0-or-later` is deliberately not used; do
not mix the two.

## What carries which licence

`scripts/license/packages.tsv` is the machine-readable source of truth and
what the CI checks read. The table below is its human-readable form.

| Package | Edition | Licence | Published |
|---|---|---|---|
| `agenomic` (this repository) | OSS | AGPL-3.0-only | GitHub releases |
| `crates/agenomic-fingerprint` | OSS | AGPL-3.0-only | crates.io |
| `crates/agenomic-metrics` | OSS | AGPL-3.0-only | crates.io |
| `agenomic-cli` (25 crates, one workspace licence) | OSS | AGPL-3.0-only | crates.io |
| `agenomic-codedrift` | OSS | AGPL-3.0-only | no |
| `agenomic-examples` | OSS | AGPL-3.0-only | no |
| `agenomic-python` | OSS | Apache-2.0 | PyPI |
| `agenomic-typescript` | OSS | MIT | npm |
| `agenomic-spec` | OSS | Apache-2.0 | no |
| `agenomic-cloud` | Cloud | proprietary | no |
| `agenomic-web` | Cloud | proprietary | no |
| `agenomic-governance-agents` | Cloud | proprietary | no |
| `agenomic-infra` | Cloud | proprietary | no |

The SDKs and the specification stay permissive on purpose: they are linked
into user applications and consumed by third-party generators, and a copyleft
obligation there would reach every instrumented application. The Cloud
components are proprietary, live in private repositories and are never
relicensed by an OSS synchronisation.

## Per-file headers

This repository does not use per-file `SPDX-License-Identifier` headers, and
the migration did not add any. The convention is the root `LICENSE`, the
package metadata and the CI checks. A new file needs no header.

## Third-party code

Dependencies keep their own licences. Nothing in `vendor/`, `third_party/`,
`node_modules/`, a lockfile, an SBOM or a `THIRD_PARTY_LICENSES.md` report is
ever rewritten by a licence change here, and required attributions are kept as
they are. `scripts/release/third-party-licenses.sh --check` fails a release
when a dependency declares no licence or one outside the redistributable
allow-list; that allow-list is about dependencies, not about this project's
own licence, and copyleft identifiers stay out of it.
`agenomic-cli/deny.toml` allows `AGPL-3.0-only` only through explicit
per-crate exceptions for the workspace crates, so a third-party AGPL
dependency still fails `cargo deny check`.

## Adding an OSS package

1. Classify the package as OSS and choose its licence: `AGPL-3.0-only` for
   product code, permissive only for an SDK or a specification, and say why in
   the pull request.
2. Declare the SPDX identifier in the manifest (`license` in `package.json`
   and `Cargo.toml`, `license` in `pyproject.toml`; no stale trove
   classifier).
3. Put a `LICENSE` file next to the manifest when the package is published on
   its own, so the artifact carries it.
4. Add the row to `scripts/license/packages.tsv`.
5. Run `./scripts/license/check-all.sh`.
6. Inspect the published artifact (`npm pack`, `python -m build`,
   `cargo package --list`) and confirm the licence metadata and the `LICENSE`
   file are in it.

## Adding a Cloud-only package

1. Classify it as private; it does not belong in this repository.
2. Do not apply `AGPL-3.0-only` to it.
3. Keep it out of every public artifact and out of the placeholder packages.
4. Add its name as a `cloud` row in `packages.tsv`, with no path, so
   `check-public-packages.sh` fails if its files ever appear here.
5. `scripts/ci/proprietary-leak-check.sh` stays the second line of defence.

## Checks

```sh
./scripts/license/check-all.sh          # what CI runs
./scripts/license/test-license-checks.sh # the checks are themselves tested
```

`ci-core` runs both on every pull request. `release-core` runs `check-all.sh`
before publishing and verifies that the source archive carries the AGPL text.
