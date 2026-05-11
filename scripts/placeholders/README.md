# Placeholder packages — namespace reservation

Minimal stub packages that claim the `agenomic*` names on PyPI and npm so nobody else can squat them. Each is a 1-file module / 1-line export pointing at https://github.com/treansai/agenomic. They are valid installable artifacts (both registries reject pure no-content squats), but they do nothing.

## What gets claimed

| Registry | Name |
|---|---|
| PyPI | `agenomic`, `agenomic-python`, `agenomic-sdk`, `agenomic-cli` |
| npm  | `agenomic`, `agenomic-typescript`, `agenomic-sdk`, `agenomic-cli` |

Verified all 8 names are currently available (HTTP 404 on the registry).

## Auth

### PyPI
Either:
```
~/.pypirc:
  [pypi]
  username = __token__
  password = pypi-AgEIcHlw…  (your token)
```
Or env vars in the shell session before running:
```sh
export TWINE_USERNAME=__token__
export TWINE_PASSWORD=pypi-AgEIcHlw…
```
Create a token at https://pypi.org/manage/account/token/ — scope it to "Entire account" for the first publish (PyPI requires this until the package exists), then narrow it later.

### npm
Interactive:
```sh
npm adduser
```
Or non-interactive — add to `~/.npmrc`:
```
//registry.npmjs.org/:_authToken=npm_…
```

## Publish

```sh
# Dry-run (builds packages, doesn't upload)
scripts/placeholders/publish-pypi.sh
scripts/placeholders/publish-npm.sh

# When auth is set up, upload for real
scripts/placeholders/publish-pypi.sh --apply
scripts/placeholders/publish-npm.sh --apply
```

Each script publishes the 4 packages for its registry sequentially. If one fails, fix it and re-run — `twine upload` and `npm publish` are idempotent on re-uploads of the same version (they'll error if you try to re-upload an existing version, which is fine).

## After publish

Verify:
```sh
pip install agenomic agenomic-python agenomic-sdk agenomic-cli
npm install agenomic agenomic-typescript agenomic-sdk agenomic-cli
```

The package metadata points at https://github.com/treansai/agenomic so anyone who installs the stub sees where the real package will live.

## Bumping to real releases

When you publish the actual SDK:
- For the **mirror** names (`agenomic-python`, `agenomic-typescript`, `agenomic-sdk`, `agenomic-cli`): the existing pyproject.toml / package.json in `agenomic-python/`, `agenomic-typescript/`, `agenomic-cli/` (already in the workspace) become the source. Bump version past `0.0.1` and publish from there; the placeholder gets replaced.
- For **`agenomic`** itself: decide whether it should be the Python SDK alias, the TypeScript SDK alias, or stay reserved. Publish whichever from the right repo.

There's no formal "unreserve" step — every subsequent version supersedes the placeholder.
