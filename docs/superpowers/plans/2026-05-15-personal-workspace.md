# Personal Workspace `/me/*` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Porter proto-profile.jsx vers agenomic-web : 8 onglets sous `/me/*` + modal Create Organization 4-step. Le shell route vers cette surface via le WorkspaceSwitcher (workspace personal) et le bouton avatar.

**Architecture:** Sous-shell `(shell)/me/*` qui réutilise `SettingsShell` + `ProfileSidebar`. Server components pour le routing + fetch ; client components nommés `*View.tsx` pour le JSX porté. Fetchers serveur stub `BACKEND_GAP` quand l'endpoint cloud n'existe pas — empty states côté View, jamais de mock proto en dur.

**Tech Stack:** Next.js 15 App Router, React 19, Vitest, Playwright. Validation manuelle dans les server actions (pas de zod dans deps).

---

## File structure

| File | Action | Responsibility |
|---|---|---|
| `agenomic-web/lib/server/profile.ts` | create | Types + fetchers (12 fonctions). `BACKEND_GAP` retourne défauts typés. |
| `agenomic-web/app/actions/profile.ts` | create | Server actions Zod-libre (validation manuelle), wrappant cloud ou no-op `revalidatePath`. |
| `agenomic-web/components/profile/ProfileSidebar.tsx` | create | Rail gauche porté de proto-profile l.10-37. |
| `agenomic-web/components/profile/{Overview,Account,Keys,Notifications,Sessions,Workspace,Integrations,Danger}View.tsx` | create | 8 client components ; 1 fichier par tab. |
| `agenomic-web/components/profile/CreateOrgModal.tsx` | create | Modal 4-step (ModalShell + stepper). |
| `agenomic-web/app/(shell)/me/layout.tsx` | create | Guard kind!=personal → /registry. SettingsShell aside=ProfileSidebar. |
| `agenomic-web/app/(shell)/me/page.tsx` | create | redirect('/me/overview'). |
| `agenomic-web/app/(shell)/me/{overview,account,keys,notifications,sessions,workspace,integrations,danger}/page.tsx` | create | 8 server components, fetch → View. |
| `agenomic-web/components/Shell.tsx` | modify | UserFooter ajoute item Profile → /me/overview. |
| `agenomic-web/components/WorkspaceSwitcher.tsx` | modify | handleSwitch route vers /me/overview si kind==='personal'. |
| `agenomic-web/e2e/profile.spec.ts` | create | Navigation 8 tabs + Create Org modal. |

## Arbitrages retenus

1. **zod absent** → validation manuelle (pattern actions/auth.ts).
2. **Plan pricing** conservé du proto.
3. **Shell SETTINGS group** : pas de reroute auto pour le moment (sort of scope, le `/me/*` est accessible via avatar+switcher, suffisant pour le smoke).
4. **clearLocalCache** : client-only, pas de server action.
5. **createOrganizationFullAction** : succès → `router.replace('/registry')` + `router.refresh()`.

## Tasks

### Task 1 — Plan saved
- [x] Persisté ici.

### Task 2 — lib/server/profile.ts
- [ ] Écrire types + 12 fetchers BACKEND_GAP.
- [ ] Pas de tests Vitest (stubs purs ; tests rajoutables quand les endpoints landent).

### Task 3 — app/actions/profile.ts
- [ ] 16 server actions (no-op cloud aujourd'hui sauf createApiKey/revokeApiKey/revokeAllSessions qui réutilisent les wrappers existants).
- [ ] Validation manuelle (`String()`, regex, switch sur enums).

### Task 4 — /me shell
- [ ] `app/(shell)/me/layout.tsx` : meOrRedirect + guard kind, SettingsShell aside=ProfileSidebar.
- [ ] `app/(shell)/me/page.tsx` : redirect('/me/overview').
- [ ] `components/profile/ProfileSidebar.tsx` : porte le rail.

### Task 5 — 8 pages + Views
- [ ] Une à une, page server + View client.

### Task 6 — CreateOrgModal
- [ ] `components/profile/CreateOrgModal.tsx` (client) + intégration `/me/overview` via `?createOrg=1`.

### Task 7 — Shell + WorkspaceSwitcher
- [ ] Shell UserFooter : MenuItem Profile.
- [ ] WorkspaceSwitcher : push '/me/overview' si kind===personal.

### Task 8 — e2e
- [ ] e2e/profile.spec.ts : login → /me/overview → naviguer 8 tabs → ouvrir+fermer Create Org.

### Task 9 — verification
- [ ] pnpm typecheck && pnpm test.
