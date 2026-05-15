# Sidebar Hierarchy Refactor — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refondre la sidebar de agenomic-web pour refléter la vraie hiérarchie Workspace → Registry → Bucket → Bundle → Run, avec items contextuels (Bundle inspector, Version diff, Replay viewer + runs) conditionnels à la sélection.

**Architecture:** Projection serveur. Un nouveau `lib/server/shell-context.ts` calcule un `ShellContext` à partir du pathname (header `x-pathname` posé par middleware) + searchParams ; le `(shell)/layout.tsx` le passe au `<Shell>` qui rend une arborescence déployable au lieu d'une liste plate. Aucun fetch client ajouté.

**Tech Stack:** Next.js 15 (App Router, server components), React 19, Vitest pour la projection, Playwright pour les specs e2e.

---

## File structure

| File | Action | Responsibility |
|---|---|---|
| `agenomic-web/lib/server/shell-context.ts` | create | `buildShellContext({ pathname, searchParams })` → `ShellContext`. Réutilise `getBucket`, `listBuckets`, `getBundle`, `getAgent`, `listAgents`, `listBundlesForAgent`, `listReleasesForAgent`, `getLatestReplayForRelease`, `diffBundle`. Tolérant (`tryOrNull`) — pas de throw bloquant. |
| `agenomic-web/lib/server/__tests__/shell-context.test.ts` | create | Couvre les 5 états (registry / buckets / bucket / bundle-no-prev / bundle-with-runs) avec `vi.mock` sur `@/lib/server/api` et `@/lib/server/buckets`. |
| `agenomic-web/middleware.ts` | modify | Pose `x-pathname` et `x-search` sur la requête entrante via `NextResponse.next({ request: { headers } })`. |
| `agenomic-web/app/(shell)/layout.tsx` | modify | Lit les headers, appelle `buildShellContext`, passe `context` au `<Shell>`. |
| `agenomic-web/components/Shell.tsx` | modify | Accepte `context: ShellContext`. Réécrit groupe Workspace en arborescence ; items conditionnels avec fade ; SHORTCUTS dynamiques ; Header breadcrumb dérivé de `context`. |
| `agenomic-web/e2e/bundle-inspector.spec.ts` | modify | Assert sidebar montre `Bundle inspector` actif après navigation. |
| `agenomic-web/e2e/replay.spec.ts` | modify | Assert `Replay viewer` actif et runs listés. |
| `agenomic-web/e2e/diff.spec.ts` | modify | Assert `Version diff` actif sur `/diff?...`. |

---

## ShellContext signature

```ts
export interface ShellContextBundleSummary {
  id: string;          // bundle uuid
  name: string;        // agent.description ?? agent.name
  agentSlug: string;
  version: string;     // candidate release version ?? bundle.version
}

export interface ShellContextRunSummary {
  id: string;          // replay_job.id
  status: string;
  createdAt: string;   // ISO
}

export interface ShellContextDiffTarget {
  baselineReleaseId: string;
  candidateReleaseId: string;
  agentId: string;
}

export interface ShellContext {
  activeBucketSlug?: string;
  activeBundleId?: string;
  activeRunId?: string;
  bucketBundles?: ShellContextBundleSummary[];
  bundleRuns?: ShellContextRunSummary[];
  diff?: ShellContextDiffTarget | null;
}

export async function buildShellContext(input: {
  pathname: string;
  searchParams: URLSearchParams;
}): Promise<ShellContext>;
```

**Decisions:**
- `bucketBundles` peuplé uniquement si pathname matche `/buckets/[slug]` OU `/bundle/[bundleId]`.
- Un bundle "du bucket" = `listBundlesForAgent(agent.id)[0]` (le plus récent) pour chaque `agent.slug ∈ bucket.agent_ids`.
- `diff` non-null ssi : (1) `activeBundleId` connu, (2) release candidate trouvée, (3) `candidate.previous_release_id` non-null, (4) `diffBundle(...)` retourne un payload non-vide.
- `bundleRuns` = pour chaque release pinning `activeBundleId` → `getLatestReplayForRelease(release.id)` (filter null). Trié desc par `requested_at`. Limite 5.
- Sur `/replay?jobId=X`, `activeRunId = X` et on tente de remonter au bundle via `getReplayJob → release_id → release.bundle_id` pour peupler le bucket parent.
- Sur `/diff?baseline=X&candidate=Y&agentId=Z`, on extrait `activeBundleId` depuis le release candidate.

---

## Tasks

### Task 1: Plan saved

- [x] Plan persisté dans `docs/superpowers/plans/2026-05-15-sidebar-hierarchy.md`.

### Task 2: ShellContext tests + implementation

**Files:**
- Create: `agenomic-web/lib/server/shell-context.ts`
- Create: `agenomic-web/lib/server/__tests__/shell-context.test.ts`

- [ ] Step 1: Write failing tests covering 5 states.
- [ ] Step 2: `pnpm test shell-context` → fail (module not found).
- [ ] Step 3: Implement `buildShellContext` + helpers (`resolveActiveBundle`, `resolveActiveBucketForAgent`, `loadBucketBundles`, `loadBundleRuns`, `loadDiffTarget`). All API calls wrapped in `tryOrNull` or local try/catch.
- [ ] Step 4: `pnpm test shell-context` → pass.
- [ ] Step 5: Commit.

### Task 3: Middleware + layout wiring

**Files:**
- Modify: `agenomic-web/middleware.ts`
- Modify: `agenomic-web/app/(shell)/layout.tsx`

- [ ] Step 1: In `middleware.ts`, clone request headers, set `x-pathname` + `x-search`, return `NextResponse.next({ request: { headers } })` for both auth-pass and auth-skip paths.
- [ ] Step 2: In `(shell)/layout.tsx`, read headers via `headers()` (Next 15: `await headers()`), parse, call `buildShellContext`, pass to `<Shell>`.
- [ ] Step 3: `pnpm typecheck` → pass.
- [ ] Step 4: Commit.

### Task 4: Sidebar refactor + Header breadcrumb

**Files:**
- Modify: `agenomic-web/components/Shell.tsx`

- [ ] Step 1: Add `context: ShellContext` to `Shell` and `Sidebar` prop types.
- [ ] Step 2: Replace the flat WORKSPACE group with hierarchical render: Registry → Buckets → (bucket inline + bundles) → Bundle inspector → Version diff → Replay viewer → (runs inline). Use a `<NavItem>` variant accepting `level` for indent.
- [ ] Step 3: Conditional items mount only when their data exists; wrap each in `<div style={{ transition: 'opacity 120ms', opacity: 1 }}>` (mount/unmount handles disappearance — no flash since the parent rerenders on navigation).
- [ ] Step 4: Rework SHORTCUTS to a function `shortcutsFor(context)` consulted inside the keydown handler, silently skipping hidden items.
- [ ] Step 5: Replace `TITLES[pathname]` lookup in `Header` with `breadcrumbFromContext(context, pathname, user)` returning `Array<{ label, mono?, dim? }>`.
- [ ] Step 6: Smoke-render in `pnpm dev` against the pemb fixture across the 5 manual states.
- [ ] Step 7: Commit.

### Task 5: Update e2e specs

**Files:**
- Modify: `agenomic-web/e2e/bundle-inspector.spec.ts`
- Modify: `agenomic-web/e2e/replay.spec.ts`
- Modify: `agenomic-web/e2e/diff.spec.ts`

- [ ] Step 1: `bundle-inspector.spec.ts` — after `/bundle/{id}` loads, assert sidebar has a focused `Bundle inspector` button.
- [ ] Step 2: `replay.spec.ts` — after `/replay?jobId=...`, assert `Replay viewer` is in the sidebar tree.
- [ ] Step 3: `diff.spec.ts` — for the `changed_release` scenario, assert `Version diff` appears in the sidebar.
- [ ] Step 4: Commit.

### Task 6: Verification

- [ ] `pnpm typecheck`.
- [ ] `pnpm test`.
- [ ] Final commit if cleanup needed.

---

## Risks

1. **Header propagation**: `headers()` in Next 15 returns the *request* headers. Posting `x-pathname` from `middleware` + reading via `headers()` is the canonical Next pattern; verified against Next 15.5 docs.
2. **Cost**: `buildShellContext` runs on every (shell) request. Heavy fan-out only triggers when `/bundle/[id]` or `/buckets/[slug]` is active — otherwise the function short-circuits early.
3. **Bucket fixture**: agents are stored as slugs (`agent.case-intake.pemb`), not UUIDs. Resolution uses `listAgents()` + filter; tolerated to be empty (no bucket parent shown).
