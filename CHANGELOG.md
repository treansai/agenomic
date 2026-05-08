# Changelog

All notable changes to this workspace are documented here.
Dates are UTC. Format: [semver-ish release or date] — title.

---

## [Unreleased]

### Fixes (cross-repo final review, 2026-05-08)

- **B-1 — Rate limit returns HTTP 429.** `IdentityError::RateLimited` now maps to
  `AppError::TooManyRequests` (new variant) → `StatusCode::TOO_MANY_REQUESTS`. A `Retry-After: <N>`
  header is emitted alongside the 429 body. Previously it incorrectly returned 400.

- **B-2 — Signup IP rate limit is 10/hour (clarification).** One draft acceptance criterion (AC-21)
  said "6th signup/hour → 429". The canonical rate-limit table says 10/hour. The table is canonical;
  no code change was needed. AC-21 is corrected here for reference.

- **B-3 — `GET /v1/orgs/:org_id/members` returns `{ members: [...] }` wrapper.** The handler now
  returns `ListMembersResponse { members: Vec<MemberView> }` instead of a bare array. This matches
  the web client's expected shape and makes the envelope extensible (pagination, total counts, etc.).

- **B-4 — `MemberView` gains `joined_at`.** The membership row's `created_at` is now surfaced as
  `joined_at: DateTime<Utc>` in the `MemberView` DTO, matching the `OrgMember.joined_at` field
  already present in the TypeScript types.

- **F-3 — `AuthMeResponse.user` narrowed to `UserSummary` DTO.** A new `UserSummary { id, email,
  display_name, email_verified_at }` struct in `agentlock-api-types` replaces the raw `User` struct
  in `AuthMeResponse`. This avoids exposing internal fields (`email_delivery_status`, `role`, etc.)
  to web clients. The TypeScript side adds a matching `UserSummary` interface to `types.ts`.

---

## 2026-05-08 — User-first auth + multi-workspace + Resend email

Covers submodule branches:
- `agentlock-cloud` @ `feat/user-first-auth-with-email`
- `agentlock-web` @ `feat/user-first-auth-ui`
- `agentlock-infra` @ `feat/email-secrets`

### BREAKING CHANGES

- **Signup body shape changed.** The old shape `{ org_name, email, password }` (which created an
  org and issued a session in one step) is replaced by `{ email, password, display_name }`. Signup
  no longer issues a session cookie — callers must complete the verify-email step before a session
  is available.

- **Roles collapsed.** The four-role model (`owner / admin / reviewer / read_only`) is reduced to
  three roles (`owner / maintainer / viewer`). Mapping for existing data:
  `admin` → `maintainer`, `reviewer` → `maintainer`, `read_only` → `viewer`.
  Any code that compares against the string `"admin"`, `"reviewer"`, or `"read_only"` must be
  updated.

- **`POST /v1/orgs` (unauthenticated bootstrap) moved to `POST /v1/orgs/bootstrap`.** The
  original unauthenticated route is now at `/v1/orgs/bootstrap` and is intended for operator CLI
  scripts only. `POST /v1/orgs` now requires an authenticated, email-verified session and creates
  a team workspace. CLI scripts that bootstrapped orgs must update their path.

- **Legacy user-management endpoints removed.**
  - `PATCH /v1/users/:id/role` — use `PATCH /v1/orgs/:org_id/members/:user_id` instead.
  - `POST /v1/users/:id/deactivate` — use `DELETE /v1/orgs/:org_id/members/:user_id` instead.
  Both old paths return `410 Gone` with `code: legacy_endpoint_removed`.

- **`AuthMeResponse` shape changed.** The `organization` object and top-level `role` field are
  replaced by:
  - `active_org` — the currently active workspace object.
  - `memberships` — array of `{ org_id, org_name, org_kind, role }` for all workspaces the user
    belongs to.
  Web clients and SDK consumers that destructure `{ organization, role }` from `/v1/auth/me` must
  update to `{ active_org, memberships }`.

### New features

- **Email verification via Resend.** Signup triggers a 6-digit OTP email (10-minute TTL, 5
  attempts per 10 minutes). Verification completes the session handshake. Magic-link alternative
  available via `GET /v1/auth/verify-email?token=…`.

- **Personal workspaces.** Every user automatically receives a personal workspace
  (`organizations.kind = 'personal'`) on successful email verification. No manual org creation
  needed for solo users.

- **Multi-workspace memberships (M:N).** Users can belong to multiple organizations
  simultaneously. The active workspace is tracked in the session and can be changed at any time.
  - `POST /v1/auth/workspaces/switch` — switch active org within the session.
  - `GET /v1/workspaces` — list all workspaces the authenticated user belongs to.
  - Workspace switcher is available in the UI shell header.

- **Resend webhook for email telemetry.** `POST /v1/webhooks/resend` receives bounce, complaint,
  and delivery events from Resend (Svix signature verification). Bounced/complained users are
  flagged and blocked from receiving further emails (password-reset emails excepted).

### Database migration

Migration file: `migrations/20260301000001_user_first_auth.sql`

Schema changes:
- New tables: `memberships`, `email_verifications`, `email_log`.
- `organizations` gains `kind text` (`'personal' | 'team'`) and `owner_user_id uuid`.
- `users` gains `email_verified_at timestamptz` and `email_delivery_status text`.
- Global `UNIQUE (lower(email))` index on `users` replaces the old `UNIQUE (org_id, email)`.
- `users.org_id` and `users.role` columns removed (data moved to `memberships`).

Backfill: existing users receive `email_verified_at = created_at` (grandfathered). Their role is
migrated into a `memberships` row linked to their existing org.

### Operator action required (production deployments)

Before deploying this release to a production environment:

1. **Verify your sending domain in the Resend dashboard** (`resend.com/domains`). Emails will not
   be delivered until the domain is verified and DNS records are propagated.

2. **Configure the webhook endpoint** in the Resend dashboard:
   - URL: `${API_BASE_URL}/v1/webhooks/resend`
   - Events: `email.bounced`, `email.complained`, `email.delivered`
   Copy the signing secret Resend provides; you will need it in step 3.

3. **Set Pulumi secrets for the infra stack:**
   ```bash
   pulumi config set emailProvider resend
   pulumi config set --secret resendApiKey <your-resend-api-key>
   pulumi config set --secret resendWebhookSecret <webhook-signing-secret-from-step-2>
   ```
   Then run `pulumi up` to apply the new environment variables to the gateway container.
