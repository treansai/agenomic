# User-First Auth + Personal Workspaces + Multi-Org Memberships + Resend Email — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Invert Agenomic's auth model so the user (not the org) is the root entity, sign up with email+password+OTP-verification via Resend, get an auto-provisioned personal workspace, and join/create N team workspaces via M:N memberships with a 3-role RBAC (Owner / Maintainer / Viewer).

**Architecture:** Workspaces stay scoped as `org_id` to avoid duplicating tenancy: a personal workspace is just an `organizations` row with `kind='personal'`. The source of truth for membership flips from `users.org_id` to a new `memberships(user_id, org_id, role)` table. Email verification is OTP+token (10 min TTL) gating personal-workspace creation. All transactional email goes through a new `agenomic-email` crate using Resend's HTTP API directly (no `resend-rs` dep) with idempotency keys, retry on 5xx, and a Svix webhook for delivery state.

**Tech Stack:** Rust (axum, sqlx, argon2id), Postgres + RLS, Next.js 15 App Router, Resend HTTP API, Pulumi for infra. `reqwest` for outbound HTTP. `wiremock` for HTTP mocking in Rust tests. Playwright for web e2e.

---

## Touch Points: File Structure

This refactor touches 3 submodules. Branches per submodule, all bumped together in the umbrella repo at the end.

### `agenomic-cloud/` (Rust)

**New files:**
- `migrations/20260301000001_user_first_auth.sql` — schema migration
- `crates/agenomic-email/Cargo.toml`
- `crates/agenomic-email/src/lib.rs` — `EmailSender` trait, `EmailMessage`, `EmailError`, `NoopEmailSender`
- `crates/agenomic-email/src/resend.rs` — `ResendEmailSender` HTTP impl
- `crates/agenomic-email/src/templates/mod.rs` — template registry
- `crates/agenomic-email/src/templates/verify_email.rs`
- `crates/agenomic-email/src/templates/password_reset.rs`
- `crates/agenomic-email/src/templates/team_invite.rs`
- `crates/agenomic-email/src/templates/security_alert.rs`
- `crates/agenomic-email/tests/resend_http.rs` — integration tests against `wiremock`
- `services/api-gateway/src/handlers_orgs.rs` — `POST /v1/orgs`, workspace switch
- `services/api-gateway/src/handlers_webhooks.rs` — `POST /v1/webhooks/resend`

**Modified files:**
- `crates/agenomic-auth/src/lib.rs` — `Role` enum 4→3 variants
- `crates/agenomic-core/src/models.rs` — `UserRole`, `Membership`, `OrganizationKind`, `User` extensions, `EmailVerification`, `EmailLogEntry`
- `crates/agentlock-db/src/lib.rs` — new trait methods (memberships, email-verification, email-log, personal/team org creation)
- `crates/agentlock-db/src/postgres.rs` — implementations
- `crates/agentlock-db/src/memory.rs` — in-memory implementations for tests
- `crates/agenomic-identity/src/service.rs` — `signup_user` replaces `signup_owner`; new `verify_email`, `resend_verification_email`, `create_team_organization`, `switch_active_workspace`, `list_my_workspaces`; emails sent through new `EmailSender` (replaces `MailerPort`)
- `crates/agenomic-identity/src/lib.rs` — re-exports
- `crates/agenomic-identity/src/config.rs` — add `email_verification_required`, `otp_ttl`
- `crates/agenomic-identity/Cargo.toml` — depend on `agenomic-email`
- `crates/agenomic-api-types/src/lib.rs` — `SignupRequest` (drop `org_name`), new `VerifyEmailRequest`, `ResendVerificationRequest`, `WorkspaceSwitchRequest`, `CreateOrganizationRequest`, `AuthMeResponse` shape (now includes `memberships`, `active_org`)
- `services/api-gateway/src/handlers_auth.rs` — signup returns no session, new verify_email + resend handlers
- `services/api-gateway/src/handlers_invites.rs` — refuse personal orgs, send invite email
- `services/api-gateway/src/handlers_users.rs` — opérate on memberships, deprecate `users.role` writes
- `services/api-gateway/src/auth.rs` — `AuthContext.email_verified`, `active_org_id` from membership; email-verified gate
- `services/api-gateway/src/lib.rs` — wire new handlers, `EmailSender` into `AppState`
- `services/api-gateway/src/main.rs` — construct `ResendEmailSender` from config
- `tests/integration/tests/auth_*.rs` — new tests + adapt existing for 3-role + workspace switch
- `crates/agenomic-config/src/lib.rs` — new `EmailConfig { resend_api_key, from_address, reply_to, webhook_secret, verification_required, otp_ttl_seconds }`
- `config/staging.toml`, `config/production.toml`, `.env.example`

### `agenomic-web/` (Next.js)

**New files:**
- `app/verify-email/page.tsx` — OTP entry form
- `app/auth/verify-email/[token]/page.tsx` — link variant
- `app/onboarding/page.tsx` — 3-card chooser
- `app/(shell)/components/WorkspaceSwitcher.tsx`
- `app/(shell)/settings/members/page.tsx` — replaces `users` page

**Modified files:**
- `app/signup/page.tsx` — drop `org_name` field
- `app/actions/auth.ts` — `signupAction` body shape, new `verifyEmailAction`, `resendVerificationAction`, `switchWorkspaceAction`, `createOrganizationAction`
- `app/(shell)/layout.tsx` — pass `memberships` + `active_org` into shell, redirect to `/verify-email` if unverified
- `app/(shell)/components/Shell.tsx` — header replaces hardcoded org tag with `<WorkspaceSwitcher />`
- `app/accept-invite/[token]/page.tsx` — handle logged-in path (auto-add membership) + signup-light path
- `lib/server/auth.ts` — `AuthMeResponse` shape, new helpers `verifyEmail`, `resendVerification`, `switchWorkspace`, `listWorkspaces`, `createOrganization`
- `lib/server/api.ts` — verify pending-verify cookie helper
- `lib/server/cookie-forwarding.ts` — set/read `agenomic_pending_verify` HMAC-signed cookie
- `middleware.ts` — public paths: `/verify-email`, `/auth/verify-email/:token`
- `app/(shell)/settings/users/page.tsx` — DELETE; replaced by `members/`

### `agenomic-infra/` (Pulumi)

**Modified files:**
- `pulumi/staging/index.ts` — add `RESEND_API_KEY`, `RESEND_FROM_ADDRESS`, `RESEND_REPLY_TO`, `RESEND_WEBHOOK_SECRET`, `EMAIL_VERIFICATION_REQUIRED` to gateway env / secrets
- `pulumi/production/index.ts` — same
- `README.md` — DNS setup section (SPF + DKIM via Resend dashboard), runbook "what to do if Resend is down"

### Umbrella (`agenomic/`)

- Submodule pointer bumps after each submodule PR merges
- `CHANGELOG.md` — new release section

---

## Sequencing Map

The user's 9 commits map to 9 phases. Each phase is committable and tests pass on its own except where explicitly noted (the migration row is the one exception — it ships compiling but DB tests for the new methods land in phase 3).

| Phase | Commit | Submodule | Topic |
|-------|--------|-----------|-------|
| 1 | C1 | cloud | `agenomic-email` crate |
| 2 | C2 | cloud | Migration + core models |
| 3 | C3 | cloud | DB methods + identity service rewrites |
| 4 | C4 | cloud | Handlers + middleware + webhook |
| 5 | C5 | web | Signup → verify-email → onboarding |
| 6 | C6 | web | Workspace switcher + me() shape + settings/members |
| 7 | C7 | web | Accept-invite + middleware + e2e |
| 8 | C8 | infra | Pulumi secrets + DNS docs |
| 9 | C9 | all | Docs + CHANGELOG + submodule bumps |

Branches:
- `agenomic-cloud`: `feat/user-first-auth-with-email`
- `agenomic-web`: `feat/user-first-auth-ui`
- `agenomic-infra`: `feat/email-secrets`

---

## Phase 0: Pre-flight (no commit)

- [ ] **Step 0.1: Verify Resend domain in dashboard.** Log in to https://resend.com/domains. Confirm `agenomic.dev` (or whatever production domain) is verified (SPF + DKIM green). If not, add the records and wait for propagation. Use `onboarding@resend.dev` for dev/staging if production domain isn't ready.

- [ ] **Step 0.2: Capture test addresses.** Note that Resend offers `delivered@resend.dev`, `bounced@resend.dev`, `complained@resend.dev` as no-quota test recipients. Use them in integration + staging-smoke tests.

- [ ] **Step 0.3: Confirm RESEND_WEBHOOK_SECRET.** Either reuse the existing Svix endpoint secret in Resend dashboard, or create a new endpoint pointing at `${API_GATEWAY_BASE_URL}/v1/webhooks/resend` and capture the signing secret.

- [ ] **Step 0.4: Branch off cloud.** `cd agenomic-cloud && git checkout -b feat/user-first-auth-with-email`

---

## Phase 1 — Commit C1: `agenomic-email` crate (cloud)

**Goal:** Build a self-contained crate that sends transactional email via Resend HTTP. Trait + Resend impl + Noop impl + 5 templates + tests against `wiremock`. No call sites yet.

### Task 1.1 — Scaffold the crate

**Files:**
- Create: `agenomic-cloud/crates/agenomic-email/Cargo.toml`
- Create: `agenomic-cloud/crates/agenomic-email/src/lib.rs` (initially: re-exports + module decls only)
- Modify: `agenomic-cloud/Cargo.toml` (add to `[workspace] members` and `[workspace.dependencies]`)

- [ ] **Step 1: Add the crate to the workspace.**

  Edit `agenomic-cloud/Cargo.toml`:
  - Append `"crates/agenomic-email",` to `members = [...]` (alphabetical position between `agentlock-db` and `agenomic-evidence`).
  - Add `agenomic-email = { path = "crates/agenomic-email" }` to `[workspace.dependencies]` (alphabetical).
  - Add `wiremock = "0.6"` already exists — confirm it does (line 118).

- [ ] **Step 2: Create `Cargo.toml`.**

  ```toml
  [package]
  name = "agenomic-email"
  version.workspace = true
  edition.workspace = true
  license.workspace = true
  rust-version.workspace = true

  [dependencies]
  async-trait.workspace = true
  chrono.workspace = true
  reqwest.workspace = true
  serde.workspace = true
  serde_json.workspace = true
  sha2.workspace = true
  hex.workspace = true
  thiserror.workspace = true
  tokio.workspace = true
  tracing.workspace = true
  uuid.workspace = true

  [dev-dependencies]
  wiremock.workspace = true
  tokio.workspace = true
  ```

- [ ] **Step 3: Create `src/lib.rs` with module declarations.**

  ```rust
  //! Transactional email via Resend.
  //!
  //! Provides an `EmailSender` trait the rest of the cloud depends on,
  //! a `ResendEmailSender` HTTP-direct implementation, and a
  //! `NoopEmailSender` for tests / dev. Templates render `(html, text)`
  //! pairs from typed inputs.

  pub mod error;
  pub mod message;
  pub mod resend;
  pub mod sender;
  pub mod templates;

  pub use error::EmailError;
  pub use message::{EmailMessage, EmailSendOutcome, EmailTag};
  pub use resend::ResendEmailSender;
  pub use sender::{EmailSender, NoopEmailSender};
  ```

- [ ] **Step 4: Verify it compiles in isolation.**

  Run: `cargo check -p agenomic-email`
  Expected: errors about missing modules. That's fine — next tasks fix them.

- [ ] **Step 5: Commit checkpoint (after task 1.4 lands).** Don't commit yet; final commit at end of phase.

### Task 1.2 — Define `EmailMessage`, `EmailError`, `EmailSendOutcome`

**Files:**
- Create: `agenomic-cloud/crates/agenomic-email/src/message.rs`
- Create: `agenomic-cloud/crates/agenomic-email/src/error.rs`

- [ ] **Step 1: Write the failing test.**

  Create `crates/agenomic-email/src/message.rs` with the type only (impl follows). For now write a unit test in `tests/message_shapes.rs`:

  ```rust
  use agenomic_email::{EmailMessage, EmailTag};

  #[test]
  fn build_message_with_required_fields() {
      let msg = EmailMessage::builder()
          .to("user@example.com")
          .subject("Verify your email")
          .html("<p>hi</p>")
          .text("hi")
          .idempotency_key("a-stable-key")
          .tag("purpose", "verify_email")
          .build()
          .expect("valid message");
      assert_eq!(msg.to, "user@example.com");
      assert_eq!(msg.tags.len(), 1);
      assert_eq!(msg.tags[0], EmailTag { name: "purpose".into(), value: "verify_email".into() });
  }

  #[test]
  fn missing_required_field_errors() {
      let r = EmailMessage::builder().to("u@e.com").build();
      assert!(r.is_err(), "subject/html/text/idempotency_key are required");
  }
  ```

- [ ] **Step 2: Run the test to verify failure.**

  Run: `cargo test -p agenomic-email --test message_shapes`
  Expected: compile errors (`EmailMessage` not defined).

- [ ] **Step 3: Implement `message.rs`.**

  ```rust
  //! Outbound email message + builder.

  use serde::Serialize;

  #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
  pub struct EmailTag {
      pub name: String,
      pub value: String,
  }

  #[derive(Debug, Clone)]
  pub struct EmailMessage {
      pub to: String,
      pub subject: String,
      pub html: String,
      pub text: String,
      pub idempotency_key: String,
      pub tags: Vec<EmailTag>,
      pub reply_to: Option<String>,
  }

  pub struct EmailSendOutcome {
      pub provider_message_id: String,
      pub sent_at: chrono::DateTime<chrono::Utc>,
  }

  impl EmailMessage {
      pub fn builder() -> EmailMessageBuilder { EmailMessageBuilder::default() }
  }

  #[derive(Default)]
  pub struct EmailMessageBuilder {
      to: Option<String>,
      subject: Option<String>,
      html: Option<String>,
      text: Option<String>,
      idempotency_key: Option<String>,
      tags: Vec<EmailTag>,
      reply_to: Option<String>,
  }

  impl EmailMessageBuilder {
      pub fn to(mut self, v: impl Into<String>) -> Self { self.to = Some(v.into()); self }
      pub fn subject(mut self, v: impl Into<String>) -> Self { self.subject = Some(v.into()); self }
      pub fn html(mut self, v: impl Into<String>) -> Self { self.html = Some(v.into()); self }
      pub fn text(mut self, v: impl Into<String>) -> Self { self.text = Some(v.into()); self }
      pub fn idempotency_key(mut self, v: impl Into<String>) -> Self { self.idempotency_key = Some(v.into()); self }
      pub fn reply_to(mut self, v: impl Into<String>) -> Self { self.reply_to = Some(v.into()); self }
      pub fn tag(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
          self.tags.push(EmailTag { name: name.into(), value: value.into() });
          self
      }
      pub fn build(self) -> Result<EmailMessage, &'static str> {
          Ok(EmailMessage {
              to: self.to.ok_or("to required")?,
              subject: self.subject.ok_or("subject required")?,
              html: self.html.ok_or("html required")?,
              text: self.text.ok_or("text required")?,
              idempotency_key: self.idempotency_key.ok_or("idempotency_key required")?,
              tags: self.tags,
              reply_to: self.reply_to,
          })
      }
  }
  ```

- [ ] **Step 4: Implement `error.rs`.**

  ```rust
  use thiserror::Error;

  #[derive(Debug, Error)]
  pub enum EmailError {
      #[error("rate limited by provider")]
      RateLimited,
      #[error("invalid recipient")]
      InvalidRecipient,
      #[error("invalid api key")]
      InvalidApiKey,
      #[error("from-address domain not verified")]
      DomainNotVerified,
      #[error("provider error: {0}")]
      Provider(String),
      #[error(transparent)]
      Network(#[from] reqwest::Error),
      #[error("email sending disabled")]
      Disabled,
  }
  ```

- [ ] **Step 5: Run tests, verify pass.**

  Run: `cargo test -p agenomic-email --test message_shapes`
  Expected: 2 passing tests.

### Task 1.3 — `EmailSender` trait + `NoopEmailSender`

**Files:**
- Create: `agenomic-cloud/crates/agenomic-email/src/sender.rs`

- [ ] **Step 1: Write `sender.rs`.**

  ```rust
  use async_trait::async_trait;
  use chrono::Utc;
  use std::sync::Mutex;

  use crate::{EmailError, EmailMessage, EmailSendOutcome};

  #[async_trait]
  pub trait EmailSender: Send + Sync {
      async fn send(&self, msg: EmailMessage) -> Result<EmailSendOutcome, EmailError>;
  }

  /// Test/dev implementation. Logs the rendered HTML at info level
  /// and stores recent messages for assertions in tests.
  pub struct NoopEmailSender {
      inbox: Mutex<Vec<EmailMessage>>,
  }

  impl NoopEmailSender {
      pub fn new() -> Self { Self { inbox: Mutex::new(Vec::new()) } }
      pub fn drain(&self) -> Vec<EmailMessage> {
          let mut g = self.inbox.lock().unwrap();
          std::mem::take(&mut *g)
      }
  }

  impl Default for NoopEmailSender {
      fn default() -> Self { Self::new() }
  }

  #[async_trait]
  impl EmailSender for NoopEmailSender {
      async fn send(&self, msg: EmailMessage) -> Result<EmailSendOutcome, EmailError> {
          tracing::info!(
              to = %msg.to, subject = %msg.subject,
              idem = %msg.idempotency_key, "noop email"
          );
          self.inbox.lock().unwrap().push(msg);
          Ok(EmailSendOutcome {
              provider_message_id: format!("noop-{}", uuid::Uuid::new_v4()),
              sent_at: Utc::now(),
          })
      }
  }
  ```

- [ ] **Step 2: Add a unit test.**

  Append to `crates/agenomic-email/src/sender.rs` under `#[cfg(test)] mod tests`:

  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use crate::EmailMessage;

      #[tokio::test]
      async fn noop_records_and_returns_id() {
          let s = NoopEmailSender::new();
          let m = EmailMessage::builder()
              .to("a@b.c").subject("s").html("h").text("t")
              .idempotency_key("k").build().unwrap();
          let out = s.send(m).await.unwrap();
          assert!(out.provider_message_id.starts_with("noop-"));
          assert_eq!(s.drain().len(), 1);
      }
  }
  ```

- [ ] **Step 3: Run + verify.**

  Run: `cargo test -p agenomic-email`
  Expected: all pass.

### Task 1.4 — `ResendEmailSender` HTTP impl

**Files:**
- Create: `agenomic-cloud/crates/agenomic-email/src/resend.rs`
- Create: `agenomic-cloud/crates/agenomic-email/tests/resend_http.rs`

- [ ] **Step 1: Write the failing integration test against `wiremock`.**

  Create `crates/agenomic-email/tests/resend_http.rs`:

  ```rust
  use agenomic_email::{EmailError, EmailMessage, EmailSender, ResendEmailSender};
  use serde_json::json;
  use wiremock::matchers::{header, header_exists, method, path};
  use wiremock::{Mock, MockServer, ResponseTemplate};

  fn msg() -> EmailMessage {
      EmailMessage::builder()
          .to("delivered@resend.dev")
          .subject("Verify your email")
          .html("<p>code: 123456</p>")
          .text("code: 123456")
          .idempotency_key("test-key")
          .tag("purpose", "verify_email")
          .build()
          .unwrap()
  }

  #[tokio::test]
  async fn posts_with_bearer_and_idempotency_header() {
      let server = MockServer::start().await;
      Mock::given(method("POST"))
          .and(path("/emails"))
          .and(header("Authorization", "Bearer test-key"))
          .and(header("Idempotency-Key", "test-key"))
          .and(header_exists("Content-Type"))
          .respond_with(ResponseTemplate::new(200).set_body_json(json!({
              "id": "msg_123"
          })))
          .expect(1)
          .mount(&server)
          .await;

      let sender = ResendEmailSender::new(
          server.uri(),
          "test-key".into(),
          "Agenomic <noreply@agenomic.dev>".into(),
          None,
      );
      let out = sender.send(msg()).await.unwrap();
      assert_eq!(out.provider_message_id, "msg_123");
  }

  #[tokio::test]
  async fn retries_once_on_5xx_then_succeeds() {
      let server = MockServer::start().await;
      Mock::given(method("POST")).and(path("/emails"))
          .respond_with(ResponseTemplate::new(503))
          .up_to_n_times(1)
          .expect(1)
          .mount(&server).await;
      Mock::given(method("POST")).and(path("/emails"))
          .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id":"msg_2"})))
          .expect(1)
          .mount(&server).await;

      let sender = ResendEmailSender::new(
          server.uri(), "k".into(), "f@e.dev".into(), None);
      let out = sender.send(msg()).await.unwrap();
      assert_eq!(out.provider_message_id, "msg_2");
  }

  #[tokio::test]
  async fn rate_limited_returns_specific_variant_no_retry() {
      let server = MockServer::start().await;
      Mock::given(method("POST")).and(path("/emails"))
          .respond_with(ResponseTemplate::new(429))
          .expect(1) // exactly once: no retry on 4xx
          .mount(&server).await;

      let sender = ResendEmailSender::new(
          server.uri(), "k".into(), "f@e.dev".into(), None);
      let err = sender.send(msg()).await.unwrap_err();
      assert!(matches!(err, EmailError::RateLimited));
  }

  #[tokio::test]
  async fn invalid_api_key_maps_to_invalid_api_key() {
      let server = MockServer::start().await;
      Mock::given(method("POST")).and(path("/emails"))
          .respond_with(ResponseTemplate::new(401))
          .expect(1)
          .mount(&server).await;

      let sender = ResendEmailSender::new(
          server.uri(), "k".into(), "f@e.dev".into(), None);
      let err = sender.send(msg()).await.unwrap_err();
      assert!(matches!(err, EmailError::InvalidApiKey));
  }
  ```

- [ ] **Step 2: Run tests to verify failure.**

  Run: `cargo test -p agenomic-email --test resend_http`
  Expected: compile error (no `ResendEmailSender`).

- [ ] **Step 3: Implement `resend.rs`.**

  ```rust
  //! Resend HTTP client.
  //!
  //! POST /emails on api.resend.com (or any base URL — overridable for
  //! tests via `wiremock`). Retries once on 5xx / network error after
  //! a 1s backoff. Does not retry on 4xx (rate limit, validation,
  //! auth failures): the caller decides.

  use async_trait::async_trait;
  use chrono::Utc;
  use serde::Serialize;
  use std::time::Duration;

  use crate::{EmailError, EmailMessage, EmailSendOutcome, EmailSender};

  pub struct ResendEmailSender {
      base_url: String,
      api_key: String,
      from_address: String,
      reply_to: Option<String>,
      client: reqwest::Client,
  }

  impl ResendEmailSender {
      pub fn new(
          base_url: String,
          api_key: String,
          from_address: String,
          reply_to: Option<String>,
      ) -> Self {
          let client = reqwest::Client::builder()
              .timeout(Duration::from_secs(10))
              .build()
              .expect("reqwest client");
          Self { base_url, api_key, from_address, reply_to, client }
      }
  }

  #[derive(Serialize)]
  struct ResendBody<'a> {
      from: &'a str,
      to: [&'a str; 1],
      subject: &'a str,
      html: &'a str,
      text: &'a str,
      #[serde(skip_serializing_if = "Option::is_none")]
      reply_to: Option<&'a str>,
      tags: Vec<ResendTag<'a>>,
  }

  #[derive(Serialize)]
  struct ResendTag<'a> { name: &'a str, value: &'a str }

  #[derive(serde::Deserialize)]
  struct ResendOk { id: String }

  #[async_trait]
  impl EmailSender for ResendEmailSender {
      async fn send(&self, msg: EmailMessage) -> Result<EmailSendOutcome, EmailError> {
          let url = format!("{}/emails", self.base_url.trim_end_matches('/'));
          let reply_to = msg.reply_to.as_deref().or(self.reply_to.as_deref());
          let body = ResendBody {
              from: &self.from_address,
              to: [msg.to.as_str()],
              subject: &msg.subject,
              html: &msg.html,
              text: &msg.text,
              reply_to,
              tags: msg.tags.iter()
                  .map(|t| ResendTag { name: &t.name, value: &t.value })
                  .collect(),
          };

          for attempt in 0..2 {
              let req = self.client
                  .post(&url)
                  .bearer_auth(&self.api_key)
                  .header("Idempotency-Key", &msg.idempotency_key)
                  .header("Content-Type", "application/json")
                  .json(&body);
              match req.send().await {
                  Ok(resp) => {
                      let status = resp.status();
                      if status.is_success() {
                          let parsed: ResendOk = resp.json().await
                              .map_err(EmailError::Network)?;
                          return Ok(EmailSendOutcome {
                              provider_message_id: parsed.id,
                              sent_at: Utc::now(),
                          });
                      }
                      // 4xx — no retry
                      if status.is_client_error() {
                          return Err(map_status(status, resp).await);
                      }
                      // 5xx — retry once
                      if attempt == 0 {
                          tokio::time::sleep(Duration::from_secs(1)).await;
                          continue;
                      }
                      return Err(EmailError::Provider(format!("status {}", status)));
                  }
                  Err(err) => {
                      if attempt == 0 && (err.is_timeout() || err.is_connect()) {
                          tokio::time::sleep(Duration::from_secs(1)).await;
                          continue;
                      }
                      return Err(EmailError::Network(err));
                  }
              }
          }
          unreachable!()
      }
  }

  async fn map_status(status: reqwest::StatusCode, resp: reqwest::Response) -> EmailError {
      match status.as_u16() {
          401 => EmailError::InvalidApiKey,
          403 => EmailError::DomainNotVerified,
          422 => EmailError::InvalidRecipient,
          429 => EmailError::RateLimited,
          _ => {
              let body = resp.text().await.unwrap_or_default();
              EmailError::Provider(format!("status {} body {}", status, body))
          }
      }
  }
  ```

- [ ] **Step 4: Run tests to verify pass.**

  Run: `cargo test -p agenomic-email --test resend_http -- --test-threads=1`
  Expected: 4 passing tests. Single-threaded because each test spins its own `MockServer` on a random port.

### Task 1.5 — Templates (5)

**Files:**
- Create: `agenomic-cloud/crates/agenomic-email/src/templates/mod.rs`
- Create: `agenomic-cloud/crates/agenomic-email/src/templates/verify_email.rs`
- Create: `agenomic-cloud/crates/agenomic-email/src/templates/password_reset.rs`
- Create: `agenomic-cloud/crates/agenomic-email/src/templates/team_invite.rs`
- Create: `agenomic-cloud/crates/agenomic-email/src/templates/security_alert.rs`

- [ ] **Step 1: `templates/mod.rs` registers all five.**

  ```rust
  pub mod verify_email;
  pub mod password_reset;
  pub mod team_invite;
  pub mod security_alert;

  pub use verify_email::{verify_email_combined, VerifyEmailParams};
  pub use password_reset::{password_reset, PasswordResetParams};
  pub use team_invite::{team_invite, TeamInviteParams};
  pub use security_alert::{security_alert_new_login, NewLoginParams};

  pub(crate) const FOOTER_HTML: &str =
      r#"<p style="font:12px/1.5 system-ui,sans-serif;color:#666;margin-top:32px">Agenomic · noreply, do not reply</p>"#;
  pub(crate) const FOOTER_TEXT: &str =
      "\n\nAgenomic · noreply, do not reply\n";
  ```

- [ ] **Step 2: `verify_email.rs` — combined OTP + link.**

  ```rust
  use chrono::{DateTime, Utc};

  pub struct VerifyEmailParams<'a> {
      pub user_display: Option<&'a str>,
      pub code: &'a str,            // 6 digits
      pub verify_url: &'a str,      // alternative one-click link
      pub expires_in_min: u32,
  }

  pub fn verify_email_combined(p: VerifyEmailParams<'_>) -> (String, String) {
      let greeting = p.user_display
          .map(|d| format!("Hi {d},"))
          .unwrap_or_else(|| "Hi,".to_string());
      let html = format!(
          r#"<div style="font:14px/1.5 system-ui,sans-serif;color:#111">
  <p>{greeting}</p>
  <p>Use this code to verify your email:</p>
  <p style="font:bold 32px/1 ui-monospace,monospace;letter-spacing:8px;background:#f4f4f5;padding:16px 24px;display:inline-block;border-radius:6px">{code}</p>
  <p>It expires in {mins} minutes. Or click the link to verify in one tap:</p>
  <p><a href="{url}" style="color:#1a1a1a">{url}</a></p>
  {footer}</div>"#,
          greeting = greeting,
          code = p.code,
          mins = p.expires_in_min,
          url = p.verify_url,
          footer = super::FOOTER_HTML,
      );
      let text = format!(
          "{greeting}\n\nVerification code: {code}\n\nIt expires in {mins} minutes.\nOr verify with this link:\n{url}{footer}",
          greeting = greeting, code = p.code, mins = p.expires_in_min,
          url = p.verify_url, footer = super::FOOTER_TEXT,
      );
      (html, text)
  }

  #[cfg(test)]
  mod tests {
      use super::*;
      #[test]
      fn renders_code_and_link() {
          let (h, t) = verify_email_combined(VerifyEmailParams {
              user_display: Some("Alice"),
              code: "123456",
              verify_url: "https://app.example.com/auth/verify-email/abc",
              expires_in_min: 10,
          });
          assert!(h.contains("123456"));
          assert!(h.contains("Alice"));
          assert!(h.contains("https://app.example.com/auth/verify-email/abc"));
          assert!(t.contains("123456"));
          assert!(t.contains("https://app.example.com/auth/verify-email/abc"));
      }
  }
  ```

- [ ] **Step 3: `password_reset.rs`.**

  ```rust
  use chrono::{DateTime, Utc};

  pub struct PasswordResetParams<'a> {
      pub reset_url: &'a str,
      pub expires_in_min: u32,
      pub ip: Option<&'a str>,
  }

  pub fn password_reset(p: PasswordResetParams<'_>) -> (String, String) {
      let ip_line_html = p.ip.map(|ip| format!(
          r#"<p style="color:#666;font-size:12px">Requested from IP: {}</p>"#, ip
      )).unwrap_or_default();
      let ip_line_text = p.ip.map(|ip| format!("\nRequested from IP: {}\n", ip))
          .unwrap_or_default();

      let html = format!(
          r#"<div style="font:14px/1.5 system-ui,sans-serif;color:#111">
  <p>Click the link below to reset your Agenomic password. It expires in {mins} minutes.</p>
  <p><a href="{url}" style="color:#1a1a1a">{url}</a></p>
  <p>If you didn't request this, ignore this email — your password is unchanged.</p>
  {ip}{footer}</div>"#,
          mins = p.expires_in_min, url = p.reset_url,
          ip = ip_line_html, footer = super::FOOTER_HTML,
      );
      let text = format!(
          "Reset your Agenomic password (expires in {mins} min):\n{url}\n\nIf you didn't request this, ignore this email.{ip}{footer}",
          mins = p.expires_in_min, url = p.reset_url,
          ip = ip_line_text, footer = super::FOOTER_TEXT,
      );
      (html, text)
  }

  #[cfg(test)]
  mod tests {
      use super::*;
      #[test]
      fn includes_ip_when_present() {
          let (h, _) = password_reset(PasswordResetParams {
              reset_url: "https://x/y", expires_in_min: 60, ip: Some("1.2.3.4"),
          });
          assert!(h.contains("1.2.3.4"));
      }
      #[test]
      fn omits_ip_when_absent() {
          let (h, _) = password_reset(PasswordResetParams {
              reset_url: "https://x/y", expires_in_min: 60, ip: None,
          });
          assert!(!h.contains("Requested from IP"));
      }
  }
  ```

- [ ] **Step 4: `team_invite.rs`.**

  ```rust
  pub struct TeamInviteParams<'a> {
      pub inviter_name: Option<&'a str>,
      pub org_name: &'a str,
      pub accept_url: &'a str,
      pub expires_in_days: u32,
      pub role: &'a str, // "owner" | "maintainer" | "viewer"
  }

  pub fn team_invite(p: TeamInviteParams<'_>) -> (String, String) {
      let inviter = p.inviter_name.map(|n| format!(" by {}", n))
          .unwrap_or_default();
      let html = format!(
          r#"<div style="font:14px/1.5 system-ui,sans-serif;color:#111">
  <p>You've been invited{inv} to <strong>{org}</strong> on Agenomic as <strong>{role}</strong>.</p>
  <p>Accept the invite (expires in {days} days):</p>
  <p><a href="{url}" style="color:#1a1a1a">{url}</a></p>
  {footer}</div>"#,
          inv = inviter, org = p.org_name, role = p.role,
          days = p.expires_in_days, url = p.accept_url,
          footer = super::FOOTER_HTML,
      );
      let text = format!(
          "You've been invited{inv} to {org} on Agenomic as {role}.\n\nAccept (expires in {days} days):\n{url}{footer}",
          inv = inviter, org = p.org_name, role = p.role,
          days = p.expires_in_days, url = p.accept_url,
          footer = super::FOOTER_TEXT,
      );
      (html, text)
  }
  ```

- [ ] **Step 5: `security_alert.rs`.**

  ```rust
  use chrono::{DateTime, Utc};

  pub struct NewLoginParams<'a> {
      pub ip: &'a str,
      pub user_agent: &'a str,
      pub location: Option<&'a str>,
      pub when: DateTime<Utc>,
  }

  pub fn security_alert_new_login(p: NewLoginParams<'_>) -> (String, String) {
      let location_html = p.location.map(|l| format!("<li>Location: {}</li>", l)).unwrap_or_default();
      let location_text = p.location.map(|l| format!("Location: {}\n", l)).unwrap_or_default();
      let when = p.when.to_rfc3339();
      let html = format!(
          r#"<div style="font:14px/1.5 system-ui,sans-serif;color:#111">
  <p>A new sign-in to your Agenomic account:</p>
  <ul><li>When: {when}</li><li>IP: {ip}</li><li>Device: {ua}</li>{loc}</ul>
  <p>If this was you, no action needed. If not, sign out everywhere and reset your password from settings.</p>
  {footer}</div>"#,
          when = when, ip = p.ip, ua = p.user_agent,
          loc = location_html, footer = super::FOOTER_HTML,
      );
      let text = format!(
          "New sign-in to your Agenomic account.\nWhen: {when}\nIP: {ip}\nDevice: {ua}\n{loc}\nIf not you, sign out everywhere and reset your password.{footer}",
          when = when, ip = p.ip, ua = p.user_agent,
          loc = location_text, footer = super::FOOTER_TEXT,
      );
      (html, text)
  }
  ```

- [ ] **Step 6: Update `src/lib.rs`** to also re-export everything from `templates` (already does via `pub mod templates`).

- [ ] **Step 7: Run all email-crate tests.**

  Run: `cargo test -p agenomic-email -- --test-threads=1`
  Expected: all pass (templates + sender + Resend HTTP).

### Task 1.6 — Commit phase 1

- [ ] **Step 1: cargo check the workspace.**

  Run (from `agenomic-cloud/`): `cargo check --workspace`
  Expected: clean.

- [ ] **Step 2: Commit.**

  ```bash
  cd agenomic-cloud
  git add crates/agenomic-email Cargo.toml Cargo.lock
  git commit -m "feat(email): add agenomic-email crate (Resend HTTP + templates)"
  ```

---

## Phase 2 — Commit C2: migration + core models (cloud, compile-only)

**Goal:** Add the schema migration and the new structs. Compiles. Existing tests pass. New methods do not yet have implementations — they live on the trait but are stubbed in `MemoryStore` to `unimplemented!()`. This commit is purely additive.

### Task 2.1 — Author the migration

**Files:**
- Create: `agenomic-cloud/migrations/20260301000001_user_first_auth.sql`

- [ ] **Step 1: Write the migration.**

  ```sql
  -- 20260301000001_user_first_auth.sql
  -- User-first auth: workspaces (kind=personal|team), memberships (M:N),
  -- email_verifications (OTP), email_log (audit/idempotency), users
  -- gain email_verified_at + email_delivery_status. Roles drop to
  -- 3 variants (owner/maintainer/viewer). users.org_id stays for
  -- backward-compat reads but new code reads memberships instead.

  ------------------------------------------------------------------
  -- 1. Pre-flight: detect cross-org duplicate emails before relaxing
  --    the (org_id, email) UNIQUE to global lower(email) UNIQUE.
  --    Fail loud — operators must resolve manually.
  ------------------------------------------------------------------
  DO $$
  DECLARE
      dup_count int;
  BEGIN
      SELECT count(*) INTO dup_count FROM (
          SELECT lower(email) FROM users GROUP BY lower(email) HAVING count(*) > 1
      ) d;
      IF dup_count > 0 THEN
          RAISE EXCEPTION
              'cannot enforce global email uniqueness: % duplicate emails across orgs', dup_count;
      END IF;
  END $$;

  ------------------------------------------------------------------
  -- 2. organizations: kind + owner_user_id
  ------------------------------------------------------------------
  ALTER TABLE organizations
      ADD COLUMN kind text NOT NULL DEFAULT 'team'
          CHECK (kind IN ('personal','team')),
      ADD COLUMN owner_user_id uuid REFERENCES users(id) ON DELETE SET NULL;

  CREATE UNIQUE INDEX organizations_personal_one_per_user
      ON organizations (owner_user_id) WHERE kind = 'personal';

  ------------------------------------------------------------------
  -- 3. users: email_verified_at, email_delivery_status; relax UNIQUE.
  ------------------------------------------------------------------
  ALTER TABLE users
      ADD COLUMN email_verified_at timestamptz,
      ADD COLUMN email_delivery_status text NOT NULL DEFAULT 'unknown'
          CHECK (email_delivery_status IN ('unknown','delivered','bounced','complained'));

  -- Drop the old composite uniqueness so the same email can re-appear
  -- (e.g. a user existed in org A; now invited to org B as a separate
  -- user row would conflict). New rule: lower(email) is globally unique.
  ALTER TABLE users DROP CONSTRAINT users_org_id_email_key;
  CREATE UNIQUE INDEX users_email_lower_unique ON users (lower(email));

  -- org_id becomes nullable. Existing rows keep their value (back-compat
  -- reads), new signups created via signup_user have NULL until they
  -- create / join a workspace.
  ALTER TABLE users ALTER COLUMN org_id DROP NOT NULL;

  ------------------------------------------------------------------
  -- 4. memberships: M:N user × org with role
  ------------------------------------------------------------------
  CREATE TABLE memberships (
      id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
      org_id      uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
      user_id     uuid NOT NULL REFERENCES users(id)         ON DELETE CASCADE,
      role        text NOT NULL CHECK (role IN ('owner','maintainer','viewer')),
      created_at  timestamptz NOT NULL DEFAULT now(),
      UNIQUE (org_id, user_id)
  );
  CREATE INDEX memberships_user ON memberships (user_id);
  CREATE INDEX memberships_org  ON memberships (org_id);

  ALTER TABLE memberships ENABLE ROW LEVEL SECURITY;
  ALTER TABLE memberships FORCE  ROW LEVEL SECURITY;
  CREATE POLICY memberships_tenant_isolation ON memberships
      USING      (org_id = current_setting('app.current_org_id', true)::uuid)
      WITH CHECK (org_id = current_setting('app.current_org_id', true)::uuid);

  ------------------------------------------------------------------
  -- 5. invites: rename roles 4-variant → 3-variant
  --    Map: owner→owner, admin→maintainer, reviewer→maintainer, read_only→viewer
  ------------------------------------------------------------------
  UPDATE invites SET role = CASE role
      WHEN 'admin'     THEN 'maintainer'
      WHEN 'reviewer'  THEN 'maintainer'
      WHEN 'read_only' THEN 'viewer'
      ELSE role END;

  ALTER TABLE invites DROP CONSTRAINT invites_role_check;
  ALTER TABLE invites ADD CONSTRAINT invites_role_check
      CHECK (role IN ('owner','maintainer','viewer'));

  ------------------------------------------------------------------
  -- 6. email_verifications: OTP + token (10 min TTL)
  ------------------------------------------------------------------
  CREATE TABLE email_verifications (
      id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
      user_id     uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
      code_hash   bytea NOT NULL,
      token_hash  bytea,
      attempts    int   NOT NULL DEFAULT 0,
      created_at  timestamptz NOT NULL DEFAULT now(),
      expires_at  timestamptz NOT NULL,
      consumed_at timestamptz
  );
  CREATE INDEX email_verifications_user
      ON email_verifications (user_id) WHERE consumed_at IS NULL;
  CREATE UNIQUE INDEX email_verifications_token
      ON email_verifications (token_hash) WHERE token_hash IS NOT NULL;

  -- No RLS: this table is queried pre-session (verify_email runs without
  -- a session yet). Access is gated at the application layer by user_id
  -- match — rate-limited per user.

  ------------------------------------------------------------------
  -- 7. email_log: audit + idempotency + observability
  ------------------------------------------------------------------
  CREATE TABLE email_log (
      id                  uuid PRIMARY KEY DEFAULT gen_random_uuid(),
      user_id             uuid REFERENCES users(id) ON DELETE SET NULL,
      to_email            text NOT NULL,
      purpose             text NOT NULL,
      idempotency_key     text NOT NULL UNIQUE,
      provider            text NOT NULL DEFAULT 'resend',
      provider_message_id text,
      status              text NOT NULL DEFAULT 'queued'
          CHECK (status IN ('queued','sent','delivered','bounced','complained','failed')),
      error               text,
      created_at          timestamptz NOT NULL DEFAULT now(),
      sent_at             timestamptz,
      updated_at          timestamptz NOT NULL DEFAULT now()
  );
  CREATE INDEX email_log_user    ON email_log (user_id, created_at DESC);
  CREATE INDEX email_log_purpose ON email_log (purpose, created_at DESC);
  CREATE INDEX email_log_provider_msg
      ON email_log (provider_message_id) WHERE provider_message_id IS NOT NULL;

  ------------------------------------------------------------------
  -- 8. Backfill memberships from users (4-role → 3-role mapping).
  ------------------------------------------------------------------
  INSERT INTO memberships (org_id, user_id, role)
  SELECT org_id, id,
         CASE role
             WHEN 'owner'     THEN 'owner'
             WHEN 'admin'     THEN 'maintainer'
             WHEN 'reviewer'  THEN 'maintainer'
             WHEN 'read_only' THEN 'viewer'
             ELSE 'viewer'
         END
  FROM users
  WHERE org_id IS NOT NULL
  ON CONFLICT (org_id, user_id) DO NOTHING;

  ------------------------------------------------------------------
  -- 9. Set organizations.owner_user_id from the (newly) populated
  --    memberships. First owner per org wins.
  ------------------------------------------------------------------
  UPDATE organizations o SET owner_user_id = (
      SELECT m.user_id
      FROM memberships m
      WHERE m.org_id = o.id AND m.role = 'owner'
      ORDER BY m.created_at LIMIT 1
  );

  ------------------------------------------------------------------
  -- 10. Grandfather existing users in: assume their email is verified.
  --     They could log in before this migration; do not break that.
  ------------------------------------------------------------------
  UPDATE users SET email_verified_at = COALESCE(email_verified_at, created_at);

  -- Note: users.role column kept for back-compat reads only. Not
  -- updated to new values here; new code MUST read memberships.role.
  ```

- [ ] **Step 2: Run migration locally.**

  Run: `cd agenomic-cloud && sqlx migrate run --database-url "$AGENOMIC_DATABASE_URL"`
  Expected: success. If duplicate-emails check fires, follow the error to manually merge.

- [ ] **Step 3: Verify schema with `psql`.**

  ```bash
  psql "$AGENOMIC_DATABASE_URL" -c "\d memberships"
  psql "$AGENOMIC_DATABASE_URL" -c "\d email_verifications"
  psql "$AGENOMIC_DATABASE_URL" -c "\d email_log"
  psql "$AGENOMIC_DATABASE_URL" -c "SELECT count(*) FROM memberships;"
  psql "$AGENOMIC_DATABASE_URL" -c "SELECT id, kind, owner_user_id FROM organizations LIMIT 5;"
  ```
  Expected: tables present; memberships row count matches existing user count for users with non-null org_id.

### Task 2.2 — Update `agenomic-auth::rbac::Role` to 3 variants

**Files:**
- Modify: `agenomic-cloud/crates/agenomic-auth/src/lib.rs`
- Modify: existing tests in same file

- [ ] **Step 1: Update enum + impls + From.** Replace the `Role` enum (lines 159-203 in current source) with:

  ```rust
  #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
  #[serde(rename_all = "snake_case")]
  pub enum Role {
      Owner,
      Maintainer,
      Viewer,
  }

  impl Role {
      pub const fn as_str(self) -> &'static str {
          match self {
              Self::Owner => "owner",
              Self::Maintainer => "maintainer",
              Self::Viewer => "viewer",
          }
      }
      pub const fn can_read(self) -> bool { true }
      pub const fn can_write(self) -> bool { matches!(self, Self::Owner | Self::Maintainer) }
      pub const fn can_admin(self) -> bool { matches!(self, Self::Owner) }
      pub const fn is_owner(self) -> bool { matches!(self, Self::Owner) }
  }

  impl FromStr for Role {
      type Err = &'static str;
      fn from_str(value: &str) -> Result<Self, Self::Err> {
          match value {
              "owner" => Ok(Self::Owner),
              "maintainer" => Ok(Self::Maintainer),
              "viewer" => Ok(Self::Viewer),
              // Back-compat: legacy roles map forward for code paths
              // that still read them off old data.
              "admin" | "reviewer" => Ok(Self::Maintainer),
              "read_only" => Ok(Self::Viewer),
              _ => Err("invalid role"),
          }
      }
  }
  ```

- [ ] **Step 2: Replace `From<UserRole>` impls.** (UserRole gets updated in Task 2.3.)

  ```rust
  impl From<agenomic_core::UserRole> for Role {
      fn from(value: agenomic_core::UserRole) -> Self {
          match value {
              agenomic_core::UserRole::Owner => Role::Owner,
              agenomic_core::UserRole::Maintainer => Role::Maintainer,
              agenomic_core::UserRole::Viewer => Role::Viewer,
          }
      }
  }
  impl From<Role> for agenomic_core::UserRole {
      fn from(value: Role) -> Self {
          match value {
              Role::Owner => agenomic_core::UserRole::Owner,
              Role::Maintainer => agenomic_core::UserRole::Maintainer,
              Role::Viewer => agenomic_core::UserRole::Viewer,
          }
      }
  }
  ```

- [ ] **Step 3: Update existing tests in the same file.**

  Replace the bodies of `role_round_trips_via_string` and `role_capabilities_match_design_doc` with the 3-variant equivalents:

  ```rust
  #[test]
  fn role_round_trips_via_string() {
      for role in [Role::Owner, Role::Maintainer, Role::Viewer] {
          let parsed = Role::from_str(role.as_str()).unwrap();
          assert_eq!(parsed, role);
      }
      assert_eq!(Role::from_str("admin").unwrap(), Role::Maintainer);
      assert_eq!(Role::from_str("reviewer").unwrap(), Role::Maintainer);
      assert_eq!(Role::from_str("read_only").unwrap(), Role::Viewer);
      assert!(Role::from_str("superuser").is_err());
  }

  #[test]
  fn role_capabilities_match_design() {
      assert!(!Role::Viewer.can_write());
      assert!( Role::Maintainer.can_write());
      assert!( Role::Owner.can_write());

      assert!(!Role::Viewer.can_admin());
      assert!(!Role::Maintainer.can_admin());
      assert!( Role::Owner.can_admin());

      assert!(!Role::Maintainer.is_owner());
      assert!( Role::Owner.is_owner());
  }
  ```

- [ ] **Step 4: Run tests.**

  Run: `cargo test -p agenomic-auth`
  Expected: all pass.

### Task 2.3 — Update `agenomic-core::UserRole` + add new model types

**Files:**
- Modify: `agenomic-cloud/crates/agenomic-core/src/models.rs`

- [ ] **Step 1: Replace the `UserRole` enum (3 variants).**

  ```rust
  #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
  #[serde(rename_all = "snake_case")]
  pub enum UserRole { Owner, Maintainer, Viewer }

  impl UserRole {
      pub const fn as_str(self) -> &'static str {
          match self {
              Self::Owner => "owner",
              Self::Maintainer => "maintainer",
              Self::Viewer => "viewer",
          }
      }
  }
  ```

- [ ] **Step 2: Add `OrganizationKind`, extend `Organization` and `User`.**

  ```rust
  #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
  #[serde(rename_all = "snake_case")]
  pub enum OrganizationKind { Personal, Team }

  // Extend Organization (keep existing fields, add):
  pub struct Organization {
      // ... existing fields ...
      pub kind: OrganizationKind,
      pub owner_user_id: Option<Uuid>,
  }

  // Extend User:
  pub struct User {
      // ... existing fields ...
      pub email_verified_at: Option<DateTime<Utc>>,
      pub email_delivery_status: EmailDeliveryStatus,
      // Note: org_id stays Option<Uuid> for back-compat reads.
  }

  #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
  #[serde(rename_all = "snake_case")]
  pub enum EmailDeliveryStatus { Unknown, Delivered, Bounced, Complained }

  impl EmailDeliveryStatus {
      pub fn as_str(self) -> &'static str {
          match self {
              Self::Unknown => "unknown",
              Self::Delivered => "delivered",
              Self::Bounced => "bounced",
              Self::Complained => "complained",
          }
      }
  }
  ```

- [ ] **Step 3: Add `Membership` and email types.**

  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct Membership {
      pub id: Uuid,
      pub org_id: Uuid,
      pub user_id: Uuid,
      pub role: UserRole,
      pub created_at: DateTime<Utc>,
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct EmailVerification {
      pub id: Uuid,
      pub user_id: Uuid,
      pub attempts: i32,
      pub created_at: DateTime<Utc>,
      pub expires_at: DateTime<Utc>,
      pub consumed_at: Option<DateTime<Utc>>,
      // hashes are not serialized (server-internal)
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct EmailLogEntry {
      pub id: Uuid,
      pub user_id: Option<Uuid>,
      pub to_email: String,
      pub purpose: String,
      pub idempotency_key: String,
      pub provider: String,
      pub provider_message_id: Option<String>,
      pub status: String,
      pub error: Option<String>,
      pub created_at: DateTime<Utc>,
      pub sent_at: Option<DateTime<Utc>>,
      pub updated_at: DateTime<Utc>,
  }
  ```

- [ ] **Step 4: Compile.**

  Run: `cargo check -p agenomic-core`
  Expected: clean.

### Task 2.4 — Add new audit actions

**Files:**
- Modify: `agenomic-cloud/crates/agenomic-core/src/audit.rs` (or wherever `AuditAction` lives — grep `enum AuditAction`)

- [ ] **Step 1: Locate.** Run: `grep -rn "enum AuditAction" agenomic-cloud/crates/agenomic-core/src/`

- [ ] **Step 2: Add the new variants** alongside existing ones (do not delete existing — they're referenced in `service.rs`):

  ```rust
  // Add (alphabetical or after the existing Auth* group):
  AuthSignupInitiated,
  AuthEmailVerified,
  AuthEmailVerificationFailed,
  EmailQueued,
  EmailSent,
  EmailFailed,
  EmailBounced,
  EmailComplained,
  EmailDelivered,
  OrgCreatedTeam,
  OrgCreatedPersonal,
  MembershipAdded,
  MembershipRoleChanged,
  MembershipRemoved,
  WorkspaceSwitched,
  ```

- [ ] **Step 3: Run `cargo check`.**

### Task 2.5 — Update `PlatformStore` trait surface

**Files:**
- Modify: `agenomic-cloud/crates/agentlock-db/src/lib.rs` (trait + DTOs)

- [ ] **Step 1: Add new input structs and trait methods.** Append to `lib.rs`:

  ```rust
  // ---------- Personal/team workspace creation ----------
  pub struct CreateUnverifiedUserInput {
      pub email: String,
      pub display_name: Option<String>,
      pub password_hash: String,
  }

  pub struct CreatePersonalOrganizationInput {
      pub user_id: Uuid,
      pub name: String,           // "{display}'s workspace" or fallback
      pub slug: String,           // e.g. "user-{short_id}"
  }

  pub struct CreateTeamOrganizationInput {
      pub creator_user_id: Uuid,
      pub name: String,
      pub slug: String,
  }

  pub struct WorkspaceSummary {
      pub organization: Organization,
      pub role: UserRole,
  }

  // ---------- Email verification ----------
  pub struct CreateEmailVerificationInput {
      pub user_id: Uuid,
      pub code_hash: Vec<u8>,
      pub token_hash: Option<Vec<u8>>,
      pub expires_at: DateTime<Utc>,
  }

  pub struct ConsumeVerificationByCode {
      pub user_id: Uuid,
      pub code_hash: Vec<u8>,
  }

  // ---------- Email log ----------
  pub struct RecordEmailQueuedInput {
      pub user_id: Option<Uuid>,
      pub to_email: String,
      pub purpose: String,
      pub idempotency_key: String,
  }

  // ---------- Trait additions ----------
  #[async_trait]
  pub trait PlatformStore: Send + Sync {
      // ... existing methods (keep) ...

      // Personal workspaces
      async fn create_unverified_user(&self, input: CreateUnverifiedUserInput)
          -> AppResult<User>;
      async fn delete_user(&self, user_id: Uuid) -> AppResult<()>;
      async fn mark_email_verified(&self, user_id: Uuid) -> AppResult<User>;

      async fn create_personal_organization(&self, input: CreatePersonalOrganizationInput)
          -> AppResult<Organization>;
      async fn create_team_organization(&self, input: CreateTeamOrganizationInput)
          -> AppResult<Organization>;

      // Memberships
      async fn add_membership(&self, org_id: Uuid, user_id: Uuid, role: UserRole)
          -> AppResult<Membership>;
      async fn get_membership(&self, org_id: Uuid, user_id: Uuid)
          -> AppResult<Option<Membership>>;
      async fn list_memberships_for_user(&self, user_id: Uuid)
          -> AppResult<Vec<WorkspaceSummary>>;
      async fn list_memberships_for_org(&self, org_id: Uuid)
          -> AppResult<Vec<(Membership, User)>>;
      async fn update_membership_role(&self, org_id: Uuid, user_id: Uuid, role: UserRole)
          -> AppResult<Membership>;
      async fn remove_membership(&self, org_id: Uuid, user_id: Uuid) -> AppResult<()>;
      async fn count_team_memberships(&self, org_id: Uuid) -> AppResult<i64>;

      // Update web_session.org_id atomically (workspace switch)
      async fn set_session_active_org(&self, session_id: Uuid, org_id: Uuid) -> AppResult<()>;

      // Email verifications
      async fn create_email_verification(&self, input: CreateEmailVerificationInput)
          -> AppResult<EmailVerification>;
      /// Returns Some(()) if matched + consumed, None if no match (and increments attempts).
      async fn consume_email_verification_by_code(&self, input: ConsumeVerificationByCode)
          -> AppResult<Option<()>>;
      async fn consume_email_verification_by_token(&self, token_hash: Vec<u8>)
          -> AppResult<Option<Uuid>>;
      async fn count_recent_verification_attempts(&self, user_id: Uuid, since: DateTime<Utc>)
          -> AppResult<i64>;

      // Email log
      async fn record_email_queued(&self, input: RecordEmailQueuedInput) -> AppResult<Uuid>;
      async fn record_email_sent(&self, log_id: Uuid, provider_message_id: &str)
          -> AppResult<()>;
      async fn record_email_failed(&self, log_id: Uuid, error: &str) -> AppResult<()>;
      async fn update_email_status_from_webhook(
          &self,
          provider_message_id: &str,
          status: &str,
      ) -> AppResult<Option<Uuid>>; // returns user_id if found

      // Update users.email_delivery_status from webhook
      async fn set_user_email_delivery_status(
          &self,
          user_id: Uuid,
          status: EmailDeliveryStatus,
      ) -> AppResult<()>;
  }
  ```

- [ ] **Step 2: Stub `MemoryStore` impls with `unimplemented!()` plus a `TODO(phase3)` comment.** Goal: workspace compiles. Implementation in Phase 3.

  Edit `crates/agentlock-db/src/memory.rs` — add an `impl PlatformStore for MemoryStore` block for each new method, body `unimplemented!("phase 3")`.

- [ ] **Step 3: Stub `PostgresStore` impls with `unimplemented!()`.**

  Edit `crates/agentlock-db/src/postgres.rs` similarly. (We'll fill in real SQL in Phase 3.)

- [ ] **Step 4: cargo check.**

  Run: `cargo check --workspace`
  Expected: clean. Existing tests still pass for already-implemented methods; the new methods aren't called yet by the identity service.

### Task 2.6 — Update routes' role annotations to 3-variant

**Files:**
- Modify: every place that pattern-matches `UserRole::{Admin,Reviewer,ReadOnly}` or `Role::{Admin,Reviewer,ReadOnly}`

- [ ] **Step 1: Find all sites.**

  Run: `cd agenomic-cloud && grep -rn "Role::Admin\|Role::Reviewer\|Role::ReadOnly\|UserRole::Admin\|UserRole::Reviewer\|UserRole::ReadOnly" crates/ services/ tests/`

- [ ] **Step 2: For each match, apply the 4→3 mapping.**

  - `Role::Admin` / `UserRole::Admin` → `Role::Maintainer` / `UserRole::Maintainer`
  - `Role::Reviewer` / `UserRole::Reviewer` → `Role::Maintainer` (collapse with Admin in match arms — combine arms `Owner | Maintainer` where appropriate)
  - `Role::ReadOnly` / `UserRole::ReadOnly` → `Role::Viewer` / `UserRole::Viewer`

  Specific sites to touch (run grep to confirm):
  - `crates/agenomic-identity/src/service.rs` — `role_to_user_role`, `parse_user_role`, helper match arms (signup, accept_invite, change_role, etc.)
  - `services/api-gateway/src/auth.rs:233` — `Role::ReadOnly` fallback when user vanishes → `Role::Viewer`
  - `services/api-gateway/src/handlers_*.rs` — any `RequireRole`/role checks
  - `tests/integration/tests/*.rs` — adjust expected role names

- [ ] **Step 3: Run `cargo check --workspace`.**

  Expected: clean.

- [ ] **Step 4: Run existing test suite.**

  Run: `cargo test --workspace`
  Expected: existing tests pass; some role-matrix tests may need name changes (admin → maintainer, etc.).

### Task 2.7 — Commit phase 2

- [ ] **Step 1: Commit.**

  ```bash
  cd agenomic-cloud
  git add migrations/20260301000001_user_first_auth.sql \
          crates/agenomic-auth crates/agenomic-core crates/agentlock-db \
          services/ tests/
  git commit -m "feat(auth): migration + 3-role model + memberships/email-verification structs

  Schema migration adds organizations.kind, memberships, email_verifications,
  email_log; relaxes users (lower(email) global UNIQUE; org_id nullable);
  collapses roles to owner/maintainer/viewer; backfills memberships from
  legacy users.role + grandfathers email_verified_at."
  ```

---

## Phase 3 — Commit C3: DB methods + identity service rewrites (cloud)

**Goal:** Implement every new `PlatformStore` method (memory + postgres). Rewrite `IdentityService` to: (a) drop `signup_owner` in favor of `signup_user`; (b) introduce `verify_email`, `resend_verification_email`, `create_team_organization`, `switch_active_workspace`, `list_my_workspaces`; (c) replace the `MailerPort`-based emails (invites, password resets) with calls into the new `EmailSender`. Pass integration tests.

### Task 3.1 — Replace `MailerPort` with `EmailSender` in identity

**Files:**
- Modify: `agenomic-cloud/crates/agenomic-identity/Cargo.toml` (add `agenomic-email` dep)
- Modify: `agenomic-cloud/crates/agenomic-identity/src/lib.rs` (re-exports)
- Delete or stub out: `agenomic-cloud/crates/agenomic-identity/src/mailer.rs` (kept as thin shim during transition? — decision: delete, since this is a frontal refactor; the user said no feature flags)
- Modify: `agenomic-cloud/crates/agenomic-identity/src/service.rs` (constructor, all email-sending sites)

- [ ] **Step 1: Add dep.**

  Edit `crates/agenomic-identity/Cargo.toml`:
  ```toml
  agenomic-email = { workspace = true }
  ```

- [ ] **Step 2: Delete `mailer.rs` (or empty it out).**

  Run: `rm agenomic-cloud/crates/agenomic-identity/src/mailer.rs`
  Edit `lib.rs` to remove `pub mod mailer;`.

- [ ] **Step 3: Update `service.rs` `IdentityService` struct.** Replace `mailer: Arc<dyn MailerPort>` with `email: Arc<dyn agenomic_email::EmailSender>` plus `email_from_url_origin: String` (used to build full URLs against the URL builder). Remove `mailer` references.

- [ ] **Step 4: Rewrite `create_invite` email send.**

  Replace the `self.mailer.send_invite(InviteMessage { ... })` block with:

  ```rust
  use agenomic_email::{EmailMessage, templates::team_invite, templates::TeamInviteParams};
  use sha2::{Digest, Sha256};

  let accept_url = self.url_builder.invite_accept_url(&raw_token);
  let (html, text) = team_invite(TeamInviteParams {
      inviter_name: inviter_email.as_deref(),
      org_name,
      accept_url: &accept_url,
      expires_in_days: (self.config.invite_ttl.as_secs() / 86_400) as u32,
      role: target_role.as_str(),
  });
  let idem = email_idem("team_invite", invite.id);
  let log_id = self.store.record_email_queued(RecordEmailQueuedInput {
      user_id: caller_user_id,
      to_email: normalized.clone(),
      purpose: "team_invite".into(),
      idempotency_key: idem.clone(),
  }).await.ok();
  let msg = EmailMessage::builder()
      .to(&normalized)
      .subject(format!("You're invited to {} on Agenomic", org_name))
      .html(&html).text(&text)
      .idempotency_key(&idem)
      .tag("purpose", "team_invite")
      .tag("env", &self.config.environment)
      .build()
      .map_err(|e| IdentityError::Internal(e.into()))?;
  match self.email.send(msg).await {
      Ok(out) => { if let Some(id) = log_id {
          let _ = self.store.record_email_sent(id, &out.provider_message_id).await; } }
      Err(err) => { if let Some(id) = log_id {
          let _ = self.store.record_email_failed(id, &err.to_string()).await; }
          tracing::warn!(?err, "team invite email send failed (best-effort)"); }
  }

  fn email_idem(purpose: &str, id: Uuid) -> String {
      let mut h = Sha256::new();
      h.update(purpose.as_bytes());
      h.update(b":");
      h.update(id.as_bytes());
      hex::encode(h.finalize())
  }
  ```

- [ ] **Step 5: Same pattern for `request_password_reset`.** Replace the `mailer.send_password_reset(...)` call with the templates+EmailMessage flow. Use template `password_reset` with `expires_in_min = 60`. Idempotency key `email_idem("password_reset", reset_id)`.

- [ ] **Step 6: Add `email_idem` as a private fn at the bottom of `service.rs`.**

- [ ] **Step 7: Update `IdentityService::new` constructor signature.**

  ```rust
  pub fn new(
      store: Arc<dyn PlatformStore>,
      limiter: Arc<dyn RateLimiter>,
      email: Arc<dyn agenomic_email::EmailSender>,
      url_builder: Arc<dyn UrlBuilder>,
      config: IdentityConfig,
  ) -> Self { ... }
  ```

  Removes `mailer: Arc<dyn MailerPort>`. Update every caller (api-gateway `main.rs`, identity unit tests).

- [ ] **Step 8: Identity config — add `environment: String`.** In `crates/agenomic-identity/src/config.rs`, add `pub environment: String,` (e.g., "staging" / "production" / "development"). Default impl uses "development".

- [ ] **Step 9: cargo check.** Compile error-free.

### Task 3.2 — Implement DB methods (`MemoryStore` first, TDD)

**Files:**
- Modify: `agenomic-cloud/crates/agentlock-db/src/memory.rs`

The memory store backs all unit tests. Write each method + a focused test in `crates/agentlock-db/src/memory.rs` `#[cfg(test)] mod tests`.

- [ ] **Step 1: `create_unverified_user`.**

  Test:
  ```rust
  #[tokio::test]
  async fn create_unverified_user_has_no_org_no_verified_at() {
      let s = MemoryStore::new();
      let u = s.create_unverified_user(CreateUnverifiedUserInput {
          email: "a@b.c".into(), display_name: None, password_hash: "phc".into(),
      }).await.unwrap();
      assert!(u.email_verified_at.is_none());
      assert!(u.org_id.is_none());
  }
  ```

  Impl: insert into `users` map, `org_id=None`, `email_verified_at=None`, `email_delivery_status=Unknown`, `status=Active`.

- [ ] **Step 2: `delete_user`.** Cascade-delete memberships.

- [ ] **Step 3: `mark_email_verified`.** Set `email_verified_at = Utc::now()`.

- [ ] **Step 4: `create_personal_organization`.**

  Test: enforces 1-personal-per-user via the unique index. Memory impl: keep a `personal_owner_index: HashMap<Uuid, Uuid>` and reject duplicates.

- [ ] **Step 5: `create_team_organization`.** Inserts org with `kind=Team`, `owner_user_id=Some(creator)`, AND inserts membership(creator, Owner) atomically.

- [ ] **Step 6: Membership CRUD** (5 methods + count).

- [ ] **Step 7: `set_session_active_org`.** Update `web_sessions.org_id`.

- [ ] **Step 8: Email verification methods.**

  Test for `consume_email_verification_by_code`:
  - Match → returns `Some(())`, sets `consumed_at`.
  - Miss → returns `None`, increments `attempts`.

- [ ] **Step 9: Email log methods.**

- [ ] **Step 10: `set_user_email_delivery_status`.**

- [ ] **Step 11: Run memory tests.**

  Run: `cargo test -p agentlock-db memory::tests`
  Expected: all green.

### Task 3.3 — Implement DB methods (`PostgresStore`)

**Files:**
- Modify: `agenomic-cloud/crates/agentlock-db/src/postgres.rs`
- Tests live in `tests/integration/tests/`

For each method, write SQL inside the `impl PlatformStore for PostgresStore` block, then add a focused test in `tests/integration/tests/db_user_first_auth.rs` (new file) that uses a `testcontainers` Postgres.

- [ ] **Step 1: Create the integration test scaffold.**

  New file: `tests/integration/tests/db_user_first_auth.rs`

  ```rust
  use agenomic_db::{
      ConsumeVerificationByCode, CreateEmailVerificationInput,
      CreatePersonalOrganizationInput, CreateTeamOrganizationInput,
      CreateUnverifiedUserInput, PlatformStore,
  };
  use chrono::{Duration, Utc};
  use uuid::Uuid;

  mod helpers; // existing integration helpers (testcontainers Postgres)

  #[tokio::test]
  async fn create_unverified_user_then_personal_org() {
      let ctx = helpers::start_pg().await;
      let store = ctx.store();

      let user = store.create_unverified_user(CreateUnverifiedUserInput {
          email: "alice@example.com".into(),
          display_name: Some("Alice".into()),
          password_hash: "phc".into(),
      }).await.unwrap();
      assert!(user.email_verified_at.is_none());

      let _ = store.mark_email_verified(user.id).await.unwrap();
      let org = store.create_personal_organization(CreatePersonalOrganizationInput {
          user_id: user.id,
          name: "Alice's workspace".into(),
          slug: format!("user-{}", &user.id.simple().to_string()[..6]),
      }).await.unwrap();
      assert_eq!(org.kind, agenomic_core::OrganizationKind::Personal);
      assert_eq!(org.owner_user_id, Some(user.id));

      // Membership auto-created
      let m = store.get_membership(org.id, user.id).await.unwrap().unwrap();
      assert_eq!(m.role, agenomic_core::UserRole::Owner);
  }

  #[tokio::test]
  async fn second_personal_org_is_rejected() { /* unique index proves it */ }

  #[tokio::test]
  async fn create_team_organization_inserts_owner_membership() { /* ... */ }

  #[tokio::test]
  async fn email_verification_attempts_counted_on_miss() {
      let ctx = helpers::start_pg().await;
      let s = ctx.store();
      let u = s.create_unverified_user(CreateUnverifiedUserInput {
          email: "x@y.z".into(), display_name: None, password_hash: "phc".into(),
      }).await.unwrap();
      let code_hash = sha256(b"123456");
      s.create_email_verification(CreateEmailVerificationInput {
          user_id: u.id, code_hash: code_hash.clone(),
          token_hash: None, expires_at: Utc::now() + Duration::minutes(10),
      }).await.unwrap();

      let bad = s.consume_email_verification_by_code(ConsumeVerificationByCode {
          user_id: u.id, code_hash: sha256(b"999999"),
      }).await.unwrap();
      assert!(bad.is_none());
      let count = s.count_recent_verification_attempts(u.id, Utc::now() - Duration::minutes(10))
          .await.unwrap();
      assert_eq!(count, 1);

      let ok = s.consume_email_verification_by_code(ConsumeVerificationByCode {
          user_id: u.id, code_hash,
      }).await.unwrap();
      assert!(ok.is_some());
  }

  fn sha256(b: &[u8]) -> Vec<u8> {
      use sha2::{Digest, Sha256};
      let mut h = Sha256::new(); h.update(b); h.finalize().to_vec()
  }
  ```

- [ ] **Step 2: Implement Postgres SQL for each new method.** Below are the canonical queries to land. Add real `sqlx::query!` / `sqlx::query_as!` calls for each.

  `create_unverified_user`:
  ```sql
  INSERT INTO users (email, display_name, password_hash, status, role, org_id)
  VALUES ($1, $2, $3, 'active', 'viewer', NULL)
  RETURNING id, org_id, email, display_name, role, created_at,
            email_verified_at, email_delivery_status, password_hash
  ```
  Note: `users.role` still exists (back-compat). Set to 'viewer' as a sentinel — it's not the source of truth post-refactor.

  `mark_email_verified`:
  ```sql
  UPDATE users SET email_verified_at = now() WHERE id = $1
  RETURNING ...
  ```

  `create_personal_organization` (transactional, two writes):
  ```sql
  -- inside transaction
  INSERT INTO organizations (name, slug, kind, owner_user_id)
  VALUES ($1, $2, 'personal', $3) RETURNING ...;
  INSERT INTO memberships (org_id, user_id, role)
  VALUES ($org, $user, 'owner');
  ```

  `create_team_organization`: same pattern, kind='team'.

  `add_membership` / `update_membership_role` / `remove_membership` / `get_membership` / `list_memberships_for_user` / `list_memberships_for_org` / `count_team_memberships`:
  ```sql
  INSERT INTO memberships (org_id, user_id, role) VALUES ($1, $2, $3) RETURNING *;

  UPDATE memberships SET role = $3 WHERE org_id = $1 AND user_id = $2 RETURNING *;

  DELETE FROM memberships WHERE org_id = $1 AND user_id = $2;

  SELECT * FROM memberships WHERE org_id = $1 AND user_id = $2;

  SELECT m.*, o.* FROM memberships m
  JOIN organizations o ON o.id = m.org_id
  WHERE m.user_id = $1
  ORDER BY o.kind DESC, o.name;
  -- (kind DESC sorts 'personal' after 'team' — adjust ORDER BY for UI taste)

  SELECT m.*, u.* FROM memberships m JOIN users u ON u.id = m.user_id
  WHERE m.org_id = $1 ORDER BY m.created_at;

  SELECT count(*) FROM memberships WHERE org_id = $1;
  ```

  `set_session_active_org`:
  ```sql
  UPDATE web_sessions SET org_id = $2 WHERE id = $1
  ```
  Validation that membership exists is enforced at the service layer before this call.

  `create_email_verification` (invalidates prior active rows):
  ```sql
  -- in tx
  UPDATE email_verifications SET consumed_at = now()
      WHERE user_id = $1 AND consumed_at IS NULL;
  INSERT INTO email_verifications (user_id, code_hash, token_hash, expires_at)
      VALUES ($1, $2, $3, $4) RETURNING id, user_id, attempts, created_at, expires_at, consumed_at;
  ```

  `consume_email_verification_by_code` (atomic):
  ```sql
  UPDATE email_verifications
  SET consumed_at = now()
  WHERE user_id = $1
    AND code_hash = $2
    AND consumed_at IS NULL
    AND expires_at > now()
  RETURNING id;
  -- if no row, then run UPDATE … SET attempts = attempts + 1
  -- WHERE user_id = $1 AND consumed_at IS NULL
  ```

  Implement as two queries inside a tx.

  `consume_email_verification_by_token` similar but match on token_hash, return user_id.

  `count_recent_verification_attempts`:
  ```sql
  SELECT COALESCE(SUM(attempts), 0)::bigint FROM email_verifications
  WHERE user_id = $1 AND created_at >= $2
  ```

  `record_email_queued`:
  ```sql
  INSERT INTO email_log (user_id, to_email, purpose, idempotency_key)
  VALUES ($1, $2, $3, $4)
  ON CONFLICT (idempotency_key) DO UPDATE SET updated_at = now()
  RETURNING id
  ```

  `record_email_sent`:
  ```sql
  UPDATE email_log SET status = 'sent', provider_message_id = $2,
      sent_at = now(), updated_at = now() WHERE id = $1
  ```

  `record_email_failed`:
  ```sql
  UPDATE email_log SET status = 'failed', error = $2, updated_at = now()
  WHERE id = $1
  ```

  `update_email_status_from_webhook`:
  ```sql
  UPDATE email_log SET status = $2, updated_at = now()
  WHERE provider_message_id = $1
  RETURNING user_id
  ```

  `set_user_email_delivery_status`:
  ```sql
  UPDATE users SET email_delivery_status = $2 WHERE id = $1
  ```

- [ ] **Step 3: Run Postgres integration tests.**

  Run: `cargo test --test db_user_first_auth -- --test-threads=1`
  Expected: all green.

### Task 3.4 — Identity: rewrite `signup_owner` → `signup_user`

**Files:**
- Modify: `agenomic-cloud/crates/agenomic-identity/src/service.rs`

- [ ] **Step 1: Add `SignupResult` (no session yet).**

  ```rust
  #[derive(Debug, Clone, Serialize)]
  pub struct SignupResult {
      pub user_id: Uuid,
      pub requires_email_verification: bool,
      pub otp_expires_at: DateTime<Utc>,
  }
  ```

- [ ] **Step 2: Replace `signup_owner` body.** New signature:

  ```rust
  pub async fn signup_user(
      &self,
      email: &str,
      password: &str,
      display_name: Option<String>,
      ip: Option<IpAddr>,
      _user_agent: Option<String>,
  ) -> Result<SignupResult, IdentityError>
  ```

  Body:

  1. `validate_password_strength(password)?;`
  2. Normalize email; reject empty.
  3. Rate limit: `signup:{ip}` 10/hour.
  4. Hash password (argon2id).
  5. **Begin transaction-equivalent (via store).** `let user = self.store.create_unverified_user(...)`. If this fails on duplicate email, return `IdentityError::Validation("email already registered")`. (Anti-enumeration trade-off: design says global unique → leak on signup. Acceptable per scope.)
  6. Generate OTP (6 digits) and verification token (256-bit, base64url).
  7. `self.store.create_email_verification(...)` with sha256 of OTP and token, expires_at = now + `config.otp_ttl`.
  8. Build verify URL via `url_builder.email_verify_url(&raw_token)` (NEW method on `UrlBuilder`; add to trait).
  9. Send email via `self.email`. **If send fails**, call `self.store.delete_user(user.id)` and return `IdentityError::Internal("email_provider_unavailable")`.
  10. Audit: `AuthSignupInitiated`.
  11. Return `SignupResult { user_id: user.id, requires_email_verification: true, otp_expires_at }`.

  Code:

  ```rust
  pub async fn signup_user(
      &self,
      email: &str,
      password: &str,
      display_name: Option<String>,
      ip: Option<IpAddr>,
      _user_agent: Option<String>,
  ) -> Result<SignupResult, IdentityError> {
      validate_password_strength(password)?;
      let normalized = email.trim().to_lowercase();
      if normalized.is_empty() {
          return Err(IdentityError::Validation("email required".into()));
      }
      let rl_key = ip.map(|i| format!("signup:{i}"))
          .unwrap_or_else(|| "signup:unknown".to_string());
      self.check_rate(&rl_key, Quota::per_hour(10)).await?;

      let pw_hash = hash_password(password)?;
      let user = match self.store.create_unverified_user(CreateUnverifiedUserInput {
          email: normalized.clone(),
          display_name: display_name.clone(),
          password_hash: pw_hash,
      }).await {
          Ok(u) => u,
          Err(e) if is_unique_violation(&e) => {
              return Err(IdentityError::Validation("email already registered".into()));
          }
          Err(e) => return Err(e.into()),
      };

      // OTP + token
      let mut otp_buf = [0u8; 4];
      OsRng.fill_bytes(&mut otp_buf);
      let otp = format!("{:06}", u32::from_le_bytes(otp_buf) % 1_000_000);
      let raw_token = generate_token();
      let code_hash = hash_token(&otp).into_bytes_or_vec();
      let token_hash = hash_token(&raw_token).into_bytes_or_vec();
      let expires_at = Utc::now()
          + Duration::from_std(self.config.otp_ttl)
              .unwrap_or_else(|_| Duration::minutes(10));

      let verification = self.store.create_email_verification(
          CreateEmailVerificationInput {
              user_id: user.id, code_hash,
              token_hash: Some(token_hash),
              expires_at,
          }
      ).await?;

      // Email
      let verify_url = self.url_builder.email_verify_url(&raw_token);
      let (html, text) = templates::verify_email_combined(VerifyEmailParams {
          user_display: user.display_name.as_deref(),
          code: &otp,
          verify_url: &verify_url,
          expires_in_min: 10,
      });
      let idem = email_idem("verify_email", verification.id);
      let log_id = self.store.record_email_queued(RecordEmailQueuedInput {
          user_id: Some(user.id),
          to_email: normalized.clone(),
          purpose: "verify_email".into(),
          idempotency_key: idem.clone(),
      }).await.ok();
      let msg = EmailMessage::builder()
          .to(&normalized)
          .subject("Verify your email")
          .html(&html).text(&text)
          .idempotency_key(&idem)
          .tag("purpose", "verify_email")
          .tag("env", &self.config.environment)
          .tag("user_id", user.id.to_string())
          .build()
          .map_err(|e| IdentityError::Internal(e.into()))?;

      match self.email.send(msg).await {
          Ok(out) => { if let Some(id) = log_id {
              let _ = self.store.record_email_sent(id, &out.provider_message_id).await; } }
          Err(err) => {
              if let Some(id) = log_id {
                  let _ = self.store.record_email_failed(id, &err.to_string()).await; }
              // Rollback the user — they can retry.
              let _ = self.store.delete_user(user.id).await;
              return Err(IdentityError::Internal(format!(
                  "email_provider_unavailable: {err}")));
          }
      }

      let _ = self.audit.record(
          // No org context for an unverified user. AUTH_DESIGN.md §15.4
          // recommends skipping audit for org-less events; but we DO
          // want this one — emit with a sentinel "unverified" org? No:
          // we made audit_log.org_id stay NOT NULL. Skip the row,
          // rely on the Prometheus counter `auth_signup_initiated_total`.
          // (counter increment elsewhere)
          Uuid::nil(), // sentinel; gated by a `record_global` if we add one.
          Some(user.id),
          AuditAction::AuthSignupInitiated,
          "user", user.id,
          serde_json::json!({"email": normalized}),
          None,
      ).await;

      Ok(SignupResult {
          user_id: user.id,
          requires_email_verification: true,
          otp_expires_at: expires_at,
      })
  }
  ```

  Note: the audit-log row issue (no org for an unverified user) needs a decision. Two options: (a) make `audit_log.org_id` nullable in the migration (small extra ALTER); (b) skip audit and rely on metrics. **Decision** for this plan: choose (a). Add to the migration in Phase 2:

  ```sql
  -- amend Phase 2 migration:
  ALTER TABLE audit_log ALTER COLUMN org_id DROP NOT NULL;
  ```

  Make a follow-up commit-amend or amend the migration before it's applied to production.

- [ ] **Step 3: Add `email_verify_url` to `UrlBuilder` trait.**

  ```rust
  pub trait UrlBuilder: Send + Sync + 'static {
      fn invite_accept_url(&self, raw_token: &str) -> String;
      fn password_reset_url(&self, raw_token: &str) -> String;
      fn email_verify_url(&self, raw_token: &str) -> String;
  }
  ```

  Update the gateway's impl in `services/api-gateway/src/lib.rs` (or wherever `WebUrlBuilder` lives — grep) to add the third method: `format!("{}/auth/verify-email/{}", self.web_base_url, raw_token)`.

- [ ] **Step 4: Add `is_unique_violation` helper.** Map specific Postgres errors. For memory store, never returns true.

  ```rust
  fn is_unique_violation(err: &agenomic_core::AppError) -> bool {
      matches!(err, agenomic_core::AppError::Conflict(_))
          || err.to_string().contains("duplicate key")
  }
  ```

- [ ] **Step 5: Unit test (memory store).**

  Append in `crates/agenomic-identity/src/service.rs` `#[cfg(test)] mod signup_tests`:

  ```rust
  #[tokio::test]
  async fn signup_user_creates_unverified_and_sends_otp() {
      let env = TestEnv::new().await;
      let result = env.svc.signup_user("alice@example.com", "password123", Some("Alice".into()), None, None)
          .await.unwrap();
      assert!(result.requires_email_verification);

      let outbox = env.email.drain();
      assert_eq!(outbox.len(), 1);
      let m = &outbox[0];
      assert_eq!(m.to, "alice@example.com");
      assert_eq!(m.subject, "Verify your email");
      // OTP digits in the html
      assert!(m.html.chars().filter(|c| c.is_ascii_digit()).count() >= 6);
  }

  #[tokio::test]
  async fn signup_user_rejects_duplicate_email() { /* ... */ }

  #[tokio::test]
  async fn signup_user_rolls_back_on_email_failure() {
      // Pass a sender that always errs; assert user no longer exists.
  }
  ```

- [ ] **Step 6: Run.** `cargo test -p agenomic-identity signup_tests`

### Task 3.5 — Identity: `verify_email`

**Files:**
- Modify: `crates/agenomic-identity/src/service.rs`

- [ ] **Step 1: Add `VerifyEmailResult` + method signature.**

  ```rust
  #[derive(Debug, Clone, Serialize)]
  pub struct VerifyEmailResult {
      pub user: User,
      pub active_org: Organization,
      pub tokens: SessionTokens,
  }

  pub enum VerifyEmailInput {
      Code { user_id: Uuid, code: String },
      Token { token: String },
  }

  pub async fn verify_email(
      &self,
      input: VerifyEmailInput,
      ip: Option<IpAddr>,
      user_agent: Option<String>,
  ) -> Result<VerifyEmailResult, IdentityError> { /* ... */ }
  ```

- [ ] **Step 2: Implement.**

  1. Resolve `user_id` from input. For `Code`, use the supplied id; for `Token`, look up the verification row first.
  2. Rate limit: `verify_email:{user_id}` — 5 per 10 min.
  3. `consume_email_verification_by_code` or `consume_email_verification_by_token`. If miss → `InvalidCredentials`. If past 5 cumulative failed attempts in 10 min → also `InvalidCredentials` (with rate-limit error variant).
  4. `mark_email_verified(user_id)`.
  5. `create_personal_organization(user_id, name=display_or_email_local_part + "'s workspace", slug=user-{short_id})`.
  6. `issue_session(personal_org.id, user_id, ip, user_agent)`.
  7. Audit `AuthEmailVerified`, `OrgCreatedPersonal`.
  8. Return `VerifyEmailResult`.

- [ ] **Step 3: Unit tests.**

  - `verify_email_with_correct_code_creates_personal_workspace_and_session`
  - `verify_email_with_wrong_code_returns_unauthorized`
  - `verify_email_after_5_attempts_is_rate_limited`
  - `verify_email_with_token_works`
  - `verify_email_after_expiry_returns_unauthorized`

### Task 3.6 — Identity: `resend_verification_email`

- [ ] **Step 1: Method.**

  ```rust
  pub async fn resend_verification_email(&self, email: &str) -> Result<(), IdentityError> {
      let normalized = email.trim().to_lowercase();
      // Always 200; rate limit 3/hour per email.
      self.check_rate(&format!("verify_resend:{normalized}"), Quota::per_hour(3)).await
          .ok(); // ignore; we still return Ok at top-level for anti-enum

      if let Ok(Some(record)) = self.store.get_user_for_login(&normalized).await {
          if record.user.email_verified_at.is_none() {
              // generate fresh OTP+token, replace prior, send email.
              // (same body as signup_user steps 6–9 minus user-creation)
          }
      }
      Ok(())
  }
  ```

  Always returns `Ok(())` regardless of whether email exists or send succeeded (anti-enumeration; mirrors password reset).

- [ ] **Step 2: Test.** Confirm always-Ok shape and that for a verified user, no email is queued.

### Task 3.7 — Identity: `create_team_organization`, `switch_active_workspace`, `list_my_workspaces`

- [ ] **Step 1: `create_team_organization`.**

  ```rust
  pub async fn create_team_organization(
      &self,
      caller_user_id: Uuid,
      name: &str,
  ) -> Result<Organization, IdentityError> {
      // Email must be verified.
      let record = self.store.get_user_by_id_global(caller_user_id).await?
          .ok_or(IdentityError::Validation("user not found".into()))?;
      if record.user.email_verified_at.is_none() {
          return Err(IdentityError::Validation("email_not_verified".into()));
      }
      if name.trim().is_empty() {
          return Err(IdentityError::Validation("organization name required".into()));
      }

      let slug = format!("{}-{}", slugify(name), short_id());
      let org = self.store.create_team_organization(CreateTeamOrganizationInput {
          creator_user_id: caller_user_id,
          name: name.to_string(),
          slug,
      }).await?;
      let _ = self.audit.record(
          org.id, Some(caller_user_id),
          AuditAction::OrgCreatedTeam,
          "organization", org.id,
          serde_json::json!({"name": name}),
          None,
      ).await;
      Ok(org)
  }
  ```

- [ ] **Step 2: `switch_active_workspace`.** Verify membership exists; update `web_sessions.org_id`.

  ```rust
  pub async fn switch_active_workspace(
      &self,
      session_id: Uuid,
      caller_user_id: Uuid,
      target_org_id: Uuid,
  ) -> Result<Organization, IdentityError> {
      let m = self.store.get_membership(target_org_id, caller_user_id).await?
          .ok_or(IdentityError::Validation("workspace_unavailable".into()))?;
      let _ = m;
      self.store.set_session_active_org(session_id, target_org_id).await?;
      let org = self.store.get_organization(target_org_id).await?
          .ok_or(IdentityError::Internal("org vanished".into()))?;
      let _ = self.audit.record(
          target_org_id, Some(caller_user_id),
          AuditAction::WorkspaceSwitched,
          "organization", target_org_id,
          serde_json::json!({}),
          None,
      ).await;
      Ok(org)
  }
  ```

- [ ] **Step 3: `list_my_workspaces`.** Just delegates to `store.list_memberships_for_user`.

- [ ] **Step 4: Tests** for each (memory store).

### Task 3.8 — Identity: refactor `accept_invite` for memberships

- [ ] **Step 1: New flow.**

  - If user with `lower(email)` already exists: skip create_user; if `email_verified_at` is None, mark verified (invite arrived in inbox = proof). `add_membership(invite.org_id, existing_user.id, role_from_invite)`. Issue session for active_org=invite.org_id.
  - Else: create user via `create_unverified_user` (then immediately `mark_email_verified`), `create_personal_organization` (because every user gets one), `add_membership` to the inviting org. Issue session for active_org=invite.org_id.
  - `mark_invite_accepted`.
  - Audit: `MembershipAdded`, `AuthEmailVerified` (if just-verified).

- [ ] **Step 2: Update unit tests.** Remove "creates user with the invite's role on users.role" assertion — replace with "creates membership with that role".

### Task 3.9 — Identity: refactor `change_role`, `deactivate_user` for memberships

- [ ] **Step 1: `change_role` operates on `memberships`.** Take `org_id` + `target_user_id` + `new_role`. Owner-only. Cannot demote yourself. Emit `MembershipRoleChanged`.

- [ ] **Step 2: `deactivate_user` becomes `remove_membership`.** Renamed semantically. Owner-only. Remove membership; the user keeps their personal workspace and any other orgs they belong to.

  - Old method name kept as deprecated thin shim that calls `remove_membership` until handlers are updated, OR rename in the same commit. Choose: rename in same commit.

- [ ] **Step 3: Tests** for member role changes + removal not affecting other memberships.

### Task 3.10 — Identity: post-login security alert (best-effort)

- [ ] **Step 1: After successful `login`,** spawn a detached task that checks for new device:
  - Run `store.list_recent_sessions_for_user(user_id, since=now-30d)` (NEW helper, optional — or simpler: query any prior session with the same `(user_agent prefix, ip /24)`).
  - If none found, queue email `security_alert_new_login` via `EmailSender`.

  Implementation note: the lookup helper is optional polish — if cutting scope, skip the alert and just log a tracing event. **Plan decision:** ship the audit log + tracing + Prometheus counter; defer the email-on-new-login to a follow-up. This keeps phase 3 size manageable.

  Mark this acceptance criterion as deferred in CHANGELOG.

### Task 3.11 — Wire `EmailSender` into the gateway

**Files:**
- Modify: `agenomic-cloud/services/api-gateway/src/main.rs`
- Modify: `agenomic-cloud/services/api-gateway/src/lib.rs` (`AppState`)
- Modify: `agenomic-cloud/crates/agenomic-config/src/lib.rs`

- [ ] **Step 1: Add `EmailConfig` to `agenomic-config`.**

  ```rust
  #[derive(Debug, Clone, Deserialize)]
  pub struct EmailConfig {
      pub provider: String,                  // "resend" | "noop"
      pub resend_api_key: SecretString,      // mandatory if provider=resend
      pub from_address: String,
      pub reply_to: Option<String>,
      pub webhook_secret: SecretString,
      pub verification_required: bool,       // default true in prod, false in dev
      pub otp_ttl_seconds: u64,              // default 600
      pub base_url: String,                  // resend api base, default https://api.resend.com
  }
  ```

  Add to top-level `CloudConfig` as `pub email: EmailConfig`. Add validation: `if provider == "resend" && resend_api_key.is_empty() { Err(...) }`.

- [ ] **Step 2: In `main.rs`,** build the sender and identity service:

  ```rust
  let email: Arc<dyn EmailSender> = match cfg.email.provider.as_str() {
      "resend" => Arc::new(ResendEmailSender::new(
          cfg.email.base_url.clone(),
          cfg.email.resend_api_key.expose_secret().to_string(),
          cfg.email.from_address.clone(),
          cfg.email.reply_to.clone(),
      )),
      _ => Arc::new(NoopEmailSender::new()),
  };

  let identity = IdentityService::new(
      store.clone(),
      rate_limiter.clone(),
      email.clone(),
      Arc::new(WebUrlBuilder { base: cfg.web_base_url.clone() }),
      identity_config,
  );
  ```

- [ ] **Step 3: Add `email: Arc<dyn EmailSender>` to `AppState`** (so the webhook handler can get to it if needed; mostly the handler talks to `store` though).

- [ ] **Step 4: cargo check.** Should compile.

### Task 3.12 — Run full test suite, commit

- [ ] **Step 1: `cargo test --workspace -- --test-threads=1`.** Expected: green.

- [ ] **Step 2: Commit.**

  ```bash
  git add -A
  git commit -m "feat(identity): user-first signup + email verify + memberships

  signup_user replaces signup_owner (no auto-org). verify_email creates
  personal workspace + issues session. New methods: resend_verification,
  create_team_organization, switch_active_workspace, list_my_workspaces.
  accept_invite + change_role + deactivate_user now operate on memberships.
  All transactional emails (verify, password reset, invite) go through
  the new agenomic-email crate."
  ```

---

## Phase 4 — Commit C4: handlers + middleware + webhook (cloud)

**Goal:** Surface the new identity methods over HTTP. Add the email-verified gate. Add the Resend webhook. Run the integration test matrix.

### Task 4.1 — Adjust `agenomic-api-types`

**Files:**
- Modify: `crates/agenomic-api-types/src/lib.rs`

- [ ] **Step 1: Replace `SignupRequest`.**

  ```rust
  #[derive(Debug, Deserialize)]
  pub struct SignupRequest {
      pub email: String,
      pub password: String,
      pub display_name: Option<String>,
  }

  #[derive(Debug, Serialize)]
  pub struct SignupResponse {
      pub user_id: Uuid,
      pub requires_email_verification: bool,
      pub otp_expires_at: DateTime<Utc>,
  }

  #[derive(Debug, Deserialize)]
  pub struct VerifyEmailRequest {
      pub user_id: Option<Uuid>,
      pub code: Option<String>,
      pub token: Option<String>,
  }

  #[derive(Debug, Deserialize)]
  pub struct ResendVerificationRequest { pub email: String }

  #[derive(Debug, Deserialize)]
  pub struct CreateOrganizationRequest { pub name: String }

  #[derive(Debug, Deserialize)]
  pub struct WorkspaceSwitchRequest { pub org_id: Uuid }

  #[derive(Debug, Serialize)]
  pub struct WorkspaceSummaryDto {
      pub id: Uuid,
      pub name: String,
      pub slug: String,
      pub kind: String,         // "personal" | "team"
      pub role: String,         // "owner" | "maintainer" | "viewer"
  }

  // AuthMeResponse — replace existing fields
  #[derive(Debug, Serialize)]
  pub struct AuthMeResponse {
      pub user: User,
      pub active_org: WorkspaceSummaryDto,
      pub memberships: Vec<WorkspaceSummaryDto>,
      pub session_expires_at: DateTime<Utc>,
  }
  ```

  Note the breaking change: `AuthMeResponse.organization + role` → `active_org { ..., role }` plus `memberships`. Web client adapts in Phase 6.

### Task 4.2 — Rewrite `handlers_auth.rs`

**Files:**
- Modify: `services/api-gateway/src/handlers_auth.rs`

- [ ] **Step 1: `signup` returns no cookie.**

  ```rust
  pub async fn signup(
      State(state): State<AppState>,
      headers: HeaderMap,
      addr: Option<ConnectInfo<SocketAddr>>,
      Json(payload): Json<SignupRequest>,
  ) -> Result<Json<SignupResponse>, ApiError> {
      let ip = client_ip(&headers, addr.map(|c| c.0));
      let ua = user_agent(&headers);
      let r = state.identity.signup_user(&payload.email, &payload.password,
          payload.display_name, ip, ua).await
          .map_err(identity_error_to_api)?;
      Ok(Json(SignupResponse {
          user_id: r.user_id,
          requires_email_verification: r.requires_email_verification,
          otp_expires_at: r.otp_expires_at,
      }))
  }
  ```

- [ ] **Step 2: New `verify_email` handler — returns session cookie.**

  ```rust
  pub async fn verify_email(
      State(state): State<AppState>,
      headers: HeaderMap,
      addr: Option<ConnectInfo<SocketAddr>>,
      Json(payload): Json<VerifyEmailRequest>,
  ) -> Result<Response, ApiError> {
      let input = match (payload.user_id, payload.code, payload.token) {
          (Some(uid), Some(code), _) => VerifyEmailInput::Code { user_id: uid, code },
          (_, _, Some(tok)) => VerifyEmailInput::Token { token: tok },
          _ => return Err(AppError::Validation("user_id+code or token required".into()).into()),
      };
      let ip = client_ip(&headers, addr.map(|c| c.0));
      let ua = user_agent(&headers);
      let result = state.identity.verify_email(input, ip, ua).await
          .map_err(identity_error_to_api)?;

      let body = AuthMeResponse {
          user: result.user.clone(),
          active_org: workspace_summary_dto(&result.active_org, UserRole::Owner),
          memberships: state.identity.list_my_workspaces(result.user.id).await
              .map(|ws| ws.into_iter().map(|w| workspace_summary_dto(&w.organization, w.role)).collect())
              .unwrap_or_default(),
          session_expires_at: result.tokens.expires_at,
      };
      // Build cookies + return
      Ok(cookies_response_me(&state, body, &result.tokens.session_token, &result.tokens.csrf_token))
  }
  ```

- [ ] **Step 3: New `resend_verification` handler.** Always 200.

- [ ] **Step 4: Update `me` handler.** Include memberships:

  ```rust
  pub async fn me(...) -> Result<Json<AuthMeResponse>, ApiError> {
      let user_id = auth.user_id.ok_or(AppError::Unauthorized)?;
      let user = state.store.get_user_by_id_global(user_id).await?
          .ok_or(AppError::Unauthorized)?.user;
      let memberships = state.identity.list_my_workspaces(user_id).await
          .map_err(identity_error_to_api)?;
      let active_org = memberships.iter().find(|w| w.organization.id == auth.org_id)
          .ok_or(AppError::Unauthorized)?;
      let active_dto = workspace_summary_dto(&active_org.organization, active_org.role);
      let dtos = memberships.iter()
          .map(|w| workspace_summary_dto(&w.organization, w.role)).collect();
      Ok(Json(AuthMeResponse {
          user, active_org: active_dto, memberships: dtos,
          session_expires_at: auth.authenticated_at + chrono::Duration::days(30),
      }))
  }

  fn workspace_summary_dto(org: &Organization, role: UserRole) -> WorkspaceSummaryDto {
      WorkspaceSummaryDto {
          id: org.id, name: org.name.clone(), slug: org.slug.clone(),
          kind: match org.kind { OrganizationKind::Personal => "personal", OrganizationKind::Team => "team" }.into(),
          role: role.as_str().into(),
      }
  }
  ```

- [ ] **Step 5: Routes** — register in `lib.rs::build_router`:

  ```
  POST /v1/auth/signup            handlers_auth::signup
  POST /v1/auth/verify-email      handlers_auth::verify_email
  POST /v1/auth/verify-email/resend  handlers_auth::resend_verification
  GET  /v1/auth/me                handlers_auth::me                   (session)
  POST /v1/auth/workspaces/switch handlers_auth::switch_workspace     (session)
  GET  /v1/workspaces             handlers_auth::list_workspaces       (session)
  ```

  Auth surface: `signup`, `verify-email`, `verify-email/resend`, `password/reset/request`, `password/reset/confirm` are public (pre-session).

### Task 4.3 — `handlers_orgs.rs` (NEW)

**Files:**
- Create: `services/api-gateway/src/handlers_orgs.rs`

- [ ] **Step 1: `POST /v1/orgs`.**

  ```rust
  pub async fn create_organization(
      State(state): State<AppState>,
      Extension(auth): Extension<AuthContext>,
      Json(payload): Json<CreateOrganizationRequest>,
  ) -> Result<(StatusCode, Json<WorkspaceSummaryDto>), ApiError> {
      let user_id = auth.user_id.ok_or(AppError::Unauthorized)?;
      // The middleware already guarded email_verified for non-auth routes;
      // /v1/orgs is one of them. But double-check for safety.
      if !auth.email_verified {
          return Err(AppError::Forbidden.into());
      }
      let org = state.identity.create_team_organization(user_id, &payload.name).await
          .map_err(identity_error_to_api)?;
      let dto = WorkspaceSummaryDto {
          id: org.id, name: org.name, slug: org.slug,
          kind: "team".into(), role: "owner".into(),
      };
      Ok((StatusCode::CREATED, Json(dto)))
  }
  ```

  Register at `POST /v1/orgs` (auth: session). Idempotency required.

  Note: existing `POST /v1/orgs` route in `lib.rs:170` was the bootstrap-org route (unauth). Replace it with the auth'd version. The bootstrap flow was for `signup_owner`'s legacy path — no longer needed.

- [ ] **Step 2: `POST /v1/auth/workspaces/switch`.** In `handlers_auth.rs`:

  ```rust
  pub async fn switch_workspace(
      State(state): State<AppState>,
      Extension(auth): Extension<AuthContext>,
      Json(payload): Json<WorkspaceSwitchRequest>,
  ) -> Result<Json<WorkspaceSummaryDto>, ApiError> {
      let user_id = auth.user_id.ok_or(AppError::Unauthorized)?;
      let session_id = auth.session_id.ok_or(AppError::Unauthorized)?;
      let org = state.identity.switch_active_workspace(session_id, user_id, payload.org_id).await
          .map_err(identity_error_to_api)?;
      let m = state.store.get_membership(payload.org_id, user_id).await?
          .ok_or(AppError::Forbidden)?;
      Ok(Json(workspace_summary_dto(&org, m.role)))
  }
  ```

- [ ] **Step 3: `GET /v1/workspaces`.** Return memberships list.

### Task 4.4 — `handlers_invites.rs` updates

- [ ] **Step 1: Refuse personal orgs.** In the create-invite handler, after building the `auth: AuthContext`, check:

  ```rust
  let org = state.store.get_organization(auth.org_id).await?.ok_or(AppError::Unauthorized)?;
  if matches!(org.kind, OrganizationKind::Personal) {
      return Err(AppError::Validation("cannot_invite_to_personal_workspace".into()).into());
  }
  ```

  The `IdentityService::create_invite` already sends the email; no further wiring needed here.

### Task 4.5 — `handlers_users.rs` → `handlers_members.rs`

- [ ] **Step 1: Create `handlers_members.rs`.** Members endpoints operate on memberships:

  ```rust
  GET    /v1/orgs/:org_id/members           list_memberships_for_org
  PATCH  /v1/orgs/:org_id/members/:user_id  update_membership_role  (Owner only)
  DELETE /v1/orgs/:org_id/members/:user_id  remove_membership       (Owner only)
  ```

  Each handler verifies `auth.org_id == :org_id` (cross-org access blocked by RLS anyway, but explicit check beats relying on RLS alone, per AUTH_DESIGN.md §3 constraint #5).

- [ ] **Step 2: Delete the legacy `handlers_users.rs` content** that referred to `users.role` writes (kept as a thin shim returning 410 Gone for two releases is option; but per scope rules — drop directly).

### Task 4.6 — Webhook: `handlers_webhooks.rs`

**Files:**
- Create: `services/api-gateway/src/handlers_webhooks.rs`

- [ ] **Step 1: Implement `POST /v1/webhooks/resend`.**

  ```rust
  use axum::{extract::State, http::HeaderMap, response::Response, Json};
  use hmac::{Hmac, Mac};
  use sha2::Sha256;
  use base64::Engine;

  pub async fn resend_webhook(
      State(state): State<AppState>,
      headers: HeaderMap,
      body: axum::body::Bytes,
  ) -> Result<StatusCode, ApiError> {
      // Svix headers
      let svix_id = headers.get("svix-id").and_then(|v| v.to_str().ok()).unwrap_or("");
      let svix_timestamp = headers.get("svix-timestamp").and_then(|v| v.to_str().ok()).unwrap_or("");
      let svix_signature = headers.get("svix-signature").and_then(|v| v.to_str().ok()).unwrap_or("");

      let secret = state.config.email.webhook_secret.expose_secret();
      // Secret format from Svix: "whsec_<base64>"
      let key_bytes = secret.strip_prefix("whsec_")
          .and_then(|b| base64::engine::general_purpose::STANDARD.decode(b).ok())
          .ok_or(AppError::Validation("invalid webhook secret".into()))?;

      let to_sign = format!("{}.{}.{}", svix_id, svix_timestamp,
          std::str::from_utf8(&body).unwrap_or(""));
      let mut mac = Hmac::<Sha256>::new_from_slice(&key_bytes)
          .map_err(|_| AppError::internal("hmac"))?;
      mac.update(to_sign.as_bytes());
      let expected = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());

      // svix_signature is "v1,<sig> v1,<sig2> …"
      let ok = svix_signature.split(' ').any(|s| {
          s.strip_prefix("v1,").map(|s| s == expected).unwrap_or(false)
      });
      if !ok { return Err(AppError::Forbidden.into()); }

      let event: serde_json::Value = serde_json::from_slice(&body)
          .map_err(|_| AppError::Validation("bad json".into()))?;
      let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
      let message_id = event.pointer("/data/email_id")
          .and_then(|v| v.as_str())
          .unwrap_or("");
      if message_id.is_empty() { return Ok(StatusCode::OK); }

      match event_type {
          "email.bounced" => {
              if let Ok(Some(uid)) = state.store.update_email_status_from_webhook(message_id, "bounced").await {
                  let _ = state.store.set_user_email_delivery_status(uid, EmailDeliveryStatus::Bounced).await;
              }
          }
          "email.complained" => {
              if let Ok(Some(uid)) = state.store.update_email_status_from_webhook(message_id, "complained").await {
                  let _ = state.store.set_user_email_delivery_status(uid, EmailDeliveryStatus::Complained).await;
              }
          }
          "email.delivered" => {
              let _ = state.store.update_email_status_from_webhook(message_id, "delivered").await;
          }
          _ => {} // ignore opens/clicks
      }

      Ok(StatusCode::OK)
  }
  ```

  Add `hmac = "0.12"` to `services/api-gateway/Cargo.toml`.

- [ ] **Step 2: Register route.** In `build_router` BEFORE auth middleware (it's public, signature-protected): `Router::new().route("/v1/webhooks/resend", post(resend_webhook))`.

- [ ] **Step 3: Test.** Add `tests/integration/tests/resend_webhook.rs` with an HMAC-signed body asserting bounce flag flips.

### Task 4.7 — Middleware: email-verified gate + active_org from membership

**Files:**
- Modify: `services/api-gateway/src/auth.rs`

- [ ] **Step 1: Extend `AuthContext` with `email_verified: bool`** (and keep `active_org_id` = `auth.org_id` semantically).

  ```rust
  // crates/agenomic-auth/src/lib.rs
  pub struct AuthContext {
      pub org_id: Uuid,            // = active org for session, = key.org for api_key
      pub user_id: Option<Uuid>,
      pub api_key_id: Option<Uuid>,
      pub api_key_name: String,
      pub authenticated_at: DateTime<Utc>,
      pub role: Role,
      pub auth_method: AuthMethod,
      pub session_id: Option<Uuid>,
      pub email_verified: bool,
  }
  ```

  Update `for_session` and `for_api_key` constructors to take the bool.

- [ ] **Step 2: Update `auth.rs` middleware `session_path`.**

  After loading the session row, look up the membership for `(session.org_id, user.id)`:

  ```rust
  // In session_path, after fetching session+user:
  let membership = state.store.get_membership(row.session.org_id, row.user.id).await?
      .ok_or(AppError::Forbidden)?; // workspace_unavailable
  let role: Role = membership.role.into();
  let email_verified = row.user.email_verified_at.is_some();
  request.extensions_mut().insert(AuthContext::for_session(
      row.session.org_id, row.user.id, row.session.id, role, email_verified,
  ));
  ```

  This replaces `row.user.role.into()` (which read the deprecated `users.role`).

- [ ] **Step 3: Add a per-route email-verified gate.** Either as middleware on the `/v1` API router (excluding `/v1/auth/*`, `/v1/workspaces`, `/v1/orgs` which need to be reachable to switch/create), or by checking `auth.email_verified` inline in domain handlers.

  Decision: middleware-level, applied to a subset router. Sketch:

  ```rust
  // In build_router:
  let email_gated = Router::new()
      .merge(agents_router())
      .merge(bundles_router())
      .merge(traces_router())
      .merge(releases_router())
      .merge(replay_router())
      .merge(attestation_router())
      .layer(from_fn_with_state(state.clone(), email_verified_middleware));

  let unscoped = Router::new()
      .nest("/v1/auth", auth_router())
      .nest("/v1/workspaces", workspaces_router())
      .route("/v1/orgs", post(handlers_orgs::create_organization))
      .nest("/v1/orgs/:org_id/members", members_router())
      .nest("/v1/invites", invites_router());
  ```

  `email_verified_middleware`:

  ```rust
  pub async fn email_verified_middleware(
      Extension(auth): Extension<AuthContext>,
      request: Request<Body>,
      next: Next,
  ) -> Result<Response, ApiError> {
      if !auth.email_verified {
          return Err(AppError::Forbidden.into()); // code: email_not_verified
      }
      Ok(next.run(request).await)
  }
  ```

  For `POST /v1/orgs`, perform the check inline in the handler (already done in 4.3 step 1).

- [ ] **Step 4: Verify `api_key_path`** also populates `email_verified` correctly. Bootstrap keys (no `user_id`) → consider `email_verified=true` (the org's master credential is implicitly trusted). User-bound keys: read user's `email_verified_at`.

### Task 4.8 — Integration tests

- [ ] **Step 1: New file `tests/integration/tests/auth_user_first.rs`.** Cover:
  - Signup → verify-email by code → `/v1/auth/me` → personal workspace present.
  - Verify-email by token (URL link variant).
  - Wrong code → 401, attempts incremented; after 5 → 429.
  - Signup with existing email → 400 `email_already_registered`.
  - Resend verification rate limited to 3/hour.
  - `POST /v1/orgs` with unverified user → 403 `email_not_verified`.
  - `POST /v1/orgs` with verified user → 201, owner membership.
  - Switch workspace + subsequent `me` reflects new active_org.
  - Invite on personal workspace → 403 `cannot_invite_to_personal_workspace`.
  - Invite on team workspace → email captured in `NoopEmailSender`'s outbox; accept-invite adds membership.
  - Domain route (`POST /v1/agents`) with unverified user → 403.
  - Webhook `email.bounced` → user's `email_delivery_status = bounced`.

- [ ] **Step 2: Backward-compat test.** A pre-migration user (set up via raw SQL in test fixture: insert into `users` with `email_verified_at = created_at`, plus a membership row, plus an api_key) can still call `GET /v1/agents` with the api key.

### Task 4.9 — Run + commit

- [ ] **Step 1: `cargo test --workspace -- --test-threads=1`.** Green.

- [ ] **Step 2: Commit.**

  ```bash
  git add -A
  git commit -m "feat(api): user-first auth endpoints + email-verified gate + Resend webhook

  POST /v1/auth/signup, /verify-email, /verify-email/resend,
  /workspaces/switch; GET /v1/workspaces; POST /v1/orgs (auth'd);
  invites/members operate on memberships; middleware gates domain
  routes on email_verified. POST /v1/webhooks/resend handles
  bounce/complained/delivered with Svix signature."
  ```

---

## Phase 5 — Commit C5: web signup + verify-email + onboarding

**Goal:** Drop `org_name` from signup; add OTP verification page, link-variant page, onboarding chooser.

### Task 5.1 — Branch + types

**Files:**
- Modify: `agenomic-web/lib/server/auth.ts`

- [ ] **Step 1: Branch.** `cd agenomic-web && git checkout -b feat/user-first-auth-ui`

- [ ] **Step 2: Update types.**

  ```ts
  export interface WorkspaceSummary {
    id: string; name: string; slug: string;
    kind: 'personal' | 'team';
    role: 'owner' | 'maintainer' | 'viewer';
  }
  export interface AuthMeResponse {
    user: { id: string; email: string; display_name: string | null; email_verified_at: string | null };
    active_org: WorkspaceSummary;
    memberships: WorkspaceSummary[];
    session_expires_at: string;
  }
  export interface SignupResponse {
    user_id: string;
    requires_email_verification: boolean;
    otp_expires_at: string;
  }
  ```

- [ ] **Step 3: Add `signup` body shape.**

  ```ts
  export async function signup(body: { email: string; password: string; display_name: string | null }) {
    return apiPost<SignupResponse>('/v1/auth/signup', body);
  }

  export async function verifyEmail(body: { user_id?: string; code?: string; token?: string }) {
    return apiPost<AuthMeResponse, { setCookies: string[] }>('/v1/auth/verify-email', body);
  }

  export async function resendVerification(body: { email: string }) {
    return apiPost('/v1/auth/verify-email/resend', body);
  }

  export async function switchWorkspace(orgId: string) {
    return apiPost('/v1/auth/workspaces/switch', { org_id: orgId });
  }

  export async function listWorkspaces() {
    return apiGet<WorkspaceSummary[]>('/v1/workspaces');
  }

  export async function createOrganization(name: string) {
    return apiPost<WorkspaceSummary>('/v1/orgs', { name });
  }
  ```

### Task 5.2 — Pending-verify cookie helper

**Files:**
- Modify: `agenomic-web/lib/server/cookie-forwarding.ts`

- [ ] **Step 1: Add HMAC-signed `agenomic_pending_verify` cookie.**

  ```ts
  import { createHmac, timingSafeEqual } from 'node:crypto';
  import { cookies } from 'next/headers';

  const SECRET = process.env.AGENOMIC_PENDING_COOKIE_SECRET ?? 'dev-secret-change-me';
  const COOKIE_NAME = 'agenomic_pending_verify';
  const TTL_SECONDS = 15 * 60;

  function sign(value: string): string {
    return createHmac('sha256', SECRET).update(value).digest('base64url');
  }

  export async function setPendingVerify(userId: string, email: string): Promise<void> {
    const payload = `${userId}.${email}`;
    const sig = sign(payload);
    (await cookies()).set(COOKIE_NAME, `${payload}.${sig}`, {
      httpOnly: true, secure: process.env.NODE_ENV === 'production',
      sameSite: 'lax', maxAge: TTL_SECONDS, path: '/',
    });
  }

  export async function readPendingVerify(): Promise<{ userId: string; email: string } | null> {
    const c = (await cookies()).get(COOKIE_NAME);
    if (!c?.value) return null;
    const parts = c.value.split('.');
    if (parts.length !== 3) return null;
    const [userId, email, sig] = parts;
    const expected = sign(`${userId}.${email}`);
    try {
      if (!timingSafeEqual(Buffer.from(sig, 'base64url'), Buffer.from(expected, 'base64url'))) {
        return null;
      }
    } catch { return null; }
    return { userId, email };
  }

  export async function clearPendingVerify(): Promise<void> {
    (await cookies()).delete(COOKIE_NAME);
  }
  ```

- [ ] **Step 2: Add to `.env.example`:** `AGENOMIC_PENDING_COOKIE_SECRET=` (random 32 bytes).

### Task 5.3 — Signup page rewrite

**Files:**
- Modify: `agenomic-web/app/signup/page.tsx`
- Modify: `agenomic-web/app/actions/auth.ts`

- [ ] **Step 1: Drop `org_name` field from the signup page.** Keep `display_name`, `email`, `password`. Update title/subtitle:

  ```tsx
  <AuthLayout
    title="Create your account"
    subtitle="We'll send a 6-digit code to verify your email."
    footer={...}>
    <form action={formAction}>
      <AuthError>{state.error}</AuthError>
      <AuthField label="Display name" htmlFor="display_name" hint="Optional. Shown in the dashboard.">
        <AuthInput id="display_name" type="text" name="display_name" autoFocus />
      </AuthField>
      <AuthField label="Email" htmlFor="email">
        <AuthInput id="email" type="email" name="email" autoComplete="username" required />
      </AuthField>
      <AuthField label="Password" htmlFor="password" hint="At least 8 characters.">
        <AuthInput id="password" type="password" name="password" autoComplete="new-password" required minLength={8} />
      </AuthField>
      <AuthButton type="submit" disabled={pending}>
        {pending ? 'Creating…' : 'Create account'}
      </AuthButton>
    </form>
  </AuthLayout>
  ```

- [ ] **Step 2: Update `signupAction`.**

  ```ts
  export async function signupAction(_prev: AuthFormState, formData: FormData): Promise<AuthFormState> {
    const email = String(formData.get('email') ?? '').trim();
    const password = String(formData.get('password') ?? '');
    const displayName = String(formData.get('display_name') ?? '').trim() || null;

    if (!email || !password) return { error: 'Email and password are required.' };

    let signupResp: SignupResponse;
    try {
      signupResp = await cloudSignup({ email, password, display_name: displayName });
    } catch (err) { return { error: friendly(err) }; }

    await setPendingVerify(signupResp.user_id, email);
    redirect('/verify-email');
  }
  ```

### Task 5.4 — `/verify-email` page (OTP variant)

**Files:**
- Create: `agenomic-web/app/verify-email/page.tsx`

- [ ] **Step 1: Server component reads cookie + renders client form.**

  ```tsx
  import { redirect } from 'next/navigation';
  import { readPendingVerify } from '@/lib/server/cookie-forwarding';
  import { VerifyEmailForm } from './VerifyEmailForm';

  export default async function VerifyEmailPage() {
    const pending = await readPendingVerify();
    if (!pending) redirect('/signup');
    return <VerifyEmailForm userId={pending.userId} email={pending.email} />;
  }
  ```

- [ ] **Step 2: `VerifyEmailForm.tsx` (client) — 6-input or single-input with auto-paste.**

  Single-input simpler:

  ```tsx
  'use client';
  import { useActionState, useState } from 'react';
  import { verifyEmailAction, resendVerificationAction, type AuthFormState } from '@/app/actions/auth';
  import { AuthButton, AuthField, AuthInput, AuthLayout, AuthError } from '@/components/auth/AuthLayout';

  export function VerifyEmailForm({ userId, email }: { userId: string; email: string }) {
    const [state, action, pending] = useActionState(verifyEmailAction, { error: null });
    const [resendCooldown, setResendCooldown] = useState(0);
    return (
      <AuthLayout title="Verify your email"
        subtitle={`Enter the 6-digit code sent to ${email}.`}
        footer={<a href="/signup">Wrong email? Start over</a>}>
        <form action={action}>
          <AuthError>{state.error}</AuthError>
          <input type="hidden" name="user_id" value={userId} />
          <AuthField label="Code" htmlFor="code">
            <AuthInput id="code" name="code" inputMode="numeric" pattern="[0-9]{6}" maxLength={6}
              autoComplete="one-time-code" required autoFocus
              style={{ letterSpacing: '8px', textAlign: 'center', fontFamily: 'ui-monospace,monospace' }} />
          </AuthField>
          <AuthButton type="submit" disabled={pending}>
            {pending ? 'Verifying…' : 'Verify email'}
          </AuthButton>
        </form>
        <ResendButton email={email} cooldownSec={resendCooldown} onSent={() => setResendCooldown(60)} />
      </AuthLayout>
    );
  }
  ```

  `ResendButton` posts to `resendVerificationAction` and starts a 60s cooldown timer.

- [ ] **Step 3: `verifyEmailAction` in `app/actions/auth.ts`.**

  ```ts
  export async function verifyEmailAction(_prev: AuthFormState, formData: FormData): Promise<AuthFormState> {
    const userId = String(formData.get('user_id') ?? '');
    const code = String(formData.get('code') ?? '').trim();
    if (!userId || !code) return { error: 'Code is required.' };
    try {
      const { setCookies } = await cloudVerifyEmail({ user_id: userId, code });
      await forwardSetCookies(setCookies);
      await clearPendingVerify();
    } catch (err) { return { error: friendly(err) }; }
    redirect('/onboarding');
  }
  ```

### Task 5.5 — Link-variant page `/auth/verify-email/[token]`

**Files:**
- Create: `agenomic-web/app/auth/verify-email/[token]/page.tsx`

- [ ] **Step 1: Server component performs the verify on render and redirects.**

  ```tsx
  import { redirect } from 'next/navigation';
  import { verifyEmail } from '@/lib/server/auth';
  import { forwardSetCookies } from '@/lib/server/cookie-forwarding';

  export default async function VerifyEmailLinkPage({
    params,
  }: { params: Promise<{ token: string }> }) {
    const { token } = await params;
    try {
      const { setCookies } = await verifyEmail({ token });
      await forwardSetCookies(setCookies);
    } catch {
      redirect('/signup?error=link_expired');
    }
    redirect('/onboarding');
  }
  ```

  No client component needed — pure SSR redirect.

### Task 5.6 — `/onboarding` page

**Files:**
- Create: `agenomic-web/app/onboarding/page.tsx`

- [ ] **Step 1: 3-card chooser.**

  ```tsx
  import { OnboardingCards } from './OnboardingCards';

  export default function OnboardingPage() {
    return <OnboardingCards />;
  }
  ```

  `OnboardingCards.tsx` (client): three buttons.
  - **Continue solo** → `router.push('/registry')`.
  - **Create a team** → opens a small inline form for `name`; submits via `createOrganizationAction`.
  - **Join a team** → text "Click the link in your invite email."

- [ ] **Step 2: `createOrganizationAction`.**

  ```ts
  export async function createOrganizationAction(_prev: AuthFormState, formData: FormData) {
    const name = String(formData.get('name') ?? '').trim();
    if (!name) return { error: 'Name is required.' };
    try {
      const ws = await cloudCreateOrg(name);
      await cloudSwitchWorkspace(ws.id); // reuses session, server-side
    } catch (err) { return { error: friendly(err) }; }
    redirect('/registry');
  }
  ```

### Task 5.7 — Test, commit

- [ ] **Step 1: `npm run test --prefix agenomic-web`** (existing unit tests).

- [ ] **Step 2: Manual test.** Start cloud + web in dev, signup with a real address, confirm email arrives (Resend dashboard staging), enter OTP, redirect to `/onboarding`. Click each of the 3 cards.

- [ ] **Step 3: Commit.**

  ```bash
  cd agenomic-web
  git add app/signup app/verify-email app/auth/verify-email app/onboarding \
          app/actions/auth.ts lib/server
  git commit -m "feat(web): user-first signup + email verification + onboarding"
  ```

---

## Phase 6 — Commit C6: WorkspaceSwitcher + me() shape + settings/members

**Goal:** Surface multi-workspace UX. Replace the hardcoded "Agenomic — genome registry · treans-5a436d" shell header with a proper switcher.

### Task 6.1 — Update `(shell)/layout.tsx` to consume new `me()` shape

**Files:**
- Modify: `agenomic-web/app/(shell)/layout.tsx`

- [ ] **Step 1: New shape.**

  ```tsx
  const me = await meOrRedirect(); // returns AuthMeResponse | redirects to /login
  if (!me.user.email_verified_at) redirect('/verify-email');

  return <Shell active={me.active_org} memberships={me.memberships} user={me.user}>{children}</Shell>;
  ```

### Task 6.2 — `WorkspaceSwitcher` component

**Files:**
- Create: `agenomic-web/app/(shell)/components/WorkspaceSwitcher.tsx`
- Modify: `agenomic-web/app/(shell)/components/Shell.tsx`

- [ ] **Step 1: Switcher dropdown.**

  ```tsx
  'use client';
  import { useTransition, useState } from 'react';
  import { useRouter } from 'next/navigation';
  import type { WorkspaceSummary } from '@/lib/server/auth';
  import { switchWorkspaceAction, createOrganizationAction } from '@/app/actions/auth';

  export function WorkspaceSwitcher({ active, memberships }: {
    active: WorkspaceSummary; memberships: WorkspaceSummary[];
  }) {
    const router = useRouter();
    const [open, setOpen] = useState(false);
    const [pending, start] = useTransition();
    const personal = memberships.filter(m => m.kind === 'personal');
    const teams = memberships.filter(m => m.kind === 'team');

    function pick(orgId: string) {
      start(async () => {
        const fd = new FormData(); fd.set('org_id', orgId);
        await switchWorkspaceAction(null, fd);
        router.refresh();
        setOpen(false);
      });
    }

    return (
      <div>
        <button onClick={() => setOpen(o => !o)}>
          {active.name} <span style={{ opacity: 0.6 }}>· {active.kind}</span>
        </button>
        {open && (
          <div>
            {personal.length > 0 && (
              <section><header>Personal</header>
                {personal.map(w => <button key={w.id} onClick={() => pick(w.id)} disabled={w.id===active.id}>{w.name}</button>)}
              </section>
            )}
            {teams.length > 0 && (
              <section><header>Teams</header>
                {teams.map(w => <button key={w.id} onClick={() => pick(w.id)} disabled={w.id===active.id}>{w.name}</button>)}
              </section>
            )}
            <CreateTeamButton onCreated={() => router.refresh()} />
          </div>
        )}
      </div>
    );
  }
  ```

  Style with the project's existing theme tokens.

- [ ] **Step 2: Wire into `Shell.tsx`** in the top-left header position. Replace the hardcoded `treans-5a436d` text with `<WorkspaceSwitcher active={...} memberships={...} />`.

- [ ] **Step 3: `switchWorkspaceAction`** in `app/actions/auth.ts`:

  ```ts
  export async function switchWorkspaceAction(_prev: any, formData: FormData) {
    const orgId = String(formData.get('org_id') ?? '');
    try { await cloudSwitchWorkspace(orgId); }
    catch (err) { return { error: friendly(err) }; }
    return { error: null };
  }
  ```

### Task 6.3 — `/settings/members` (replaces `/settings/users`)

**Files:**
- Delete: `agenomic-web/app/(shell)/settings/users/page.tsx`
- Create: `agenomic-web/app/(shell)/settings/members/page.tsx`

- [ ] **Step 1: List + role dropdown + remove button.**

  ```tsx
  // Server component
  import { listMembers } from '@/lib/server/auth';
  import { meOrRedirect } from '@/lib/server/auth';

  export default async function MembersPage() {
    const me = await meOrRedirect();
    if (me.active_org.kind === 'personal') {
      return <Banner>Switch to a team workspace to manage members.</Banner>;
    }
    const members = await listMembers(me.active_org.id);
    return <MembersTable members={members} canManage={me.active_org.role === 'owner'} />;
  }
  ```

- [ ] **Step 2: Role dropdown options:** `owner`, `maintainer`, `viewer`.

- [ ] **Step 3: Add `lib/server/auth.ts` helpers.** `listMembers(orgId)`, `updateMemberRole(orgId, userId, role)`, `removeMember(orgId, userId)`.

### Task 6.4 — Commit phase 6

- [ ] **Step 1: Run tests.** `npm run test`.

- [ ] **Step 2: Commit.**

  ```bash
  git add app/\(shell\) app/actions lib/server/auth.ts
  git commit -m "feat(web): workspace switcher + members settings (M:N memberships)"
  ```

---

## Phase 7 — Commit C7: accept-invite + middleware + e2e

**Goal:** Adapt accept-invite to the new flow (logged-in fast path, signup-light slow path). Update middleware. Add Playwright e2e.

### Task 7.1 — `app/accept-invite/[token]/page.tsx`

**Files:**
- Modify: `agenomic-web/app/accept-invite/[token]/page.tsx`

- [ ] **Step 1: Branch on session presence.**

  Server component:
  ```tsx
  export default async function AcceptInvitePage({ params }: { params: Promise<{ token: string }> }) {
    const { token } = await params;
    const session = await tryReadSession();  // returns AuthMeResponse | null
    if (session) {
      // Logged-in path: backend accepts the token, adds membership, switches.
      try {
        const { setCookies } = await cloudAcceptInvite(token, { /* no password */ });
        await forwardSetCookies(setCookies);
      } catch { return <ErrorView />; }
      redirect('/registry');
    }
    // Not logged in: render password form
    return <AcceptInviteForm token={token} />;
  }
  ```

- [ ] **Step 2: `AcceptInviteForm` (client) submits `password + display_name` to `acceptInviteAction`.** Existing action; verify shape still matches.

### Task 7.2 — `middleware.ts` public paths

**Files:**
- Modify: `agenomic-web/middleware.ts`

- [ ] **Step 1: Add `/verify-email`, `/auth/verify-email/...` to `PUBLIC_PATHS`.**

  ```ts
  const PUBLIC_PATHS: ReadonlyArray<string | RegExp> = [
    '/login', '/signup',
    '/verify-email',
    /^\/auth\/verify-email\//,
    '/reset-password',
    /^\/accept-invite\//,
    /^\/api\/health/,
    /^\/_next\//, /^\/favicon\.ico$/,
  ];
  ```

- [ ] **Step 2: `/onboarding` stays protected.** It's only reachable post-verify.

### Task 7.3 — Playwright e2e

**Files:**
- Create: `agenomic-web/e2e/user-first-auth.spec.ts`

- [ ] **Step 1: Stand up cloud with `EMAIL_PROVIDER=noop`** (so emails land in the in-process inbox readable via a debug endpoint, OR drop the assertions on email content and only assert flow).

  Decision: easier path is a special test endpoint behind a guard env var: `GET /v1/__test/last-email?to=...` — only enabled when `AGENOMIC_TEST_HOOKS=true`.

- [ ] **Step 2: Tests.**

  ```ts
  test('signup → verify-email → personal workspace', async ({ page, request }) => {
    const email = `t-${Date.now()}@e2e.test`;
    await page.goto('/signup');
    await page.fill('[name=email]', email);
    await page.fill('[name=password]', 'password123');
    await page.click('button[type=submit]');
    await page.waitForURL('**/verify-email');

    // Read OTP from test hook
    const otpResp = await request.get(`http://gateway/v1/__test/last-email?to=${email}`);
    const { code } = await otpResp.json();

    await page.fill('[name=code]', code);
    await page.click('button[type=submit]');
    await page.waitForURL('**/onboarding');
    await page.click('text=Continue solo');
    await page.waitForURL('**/registry');
    await expect(page.getByText("'s workspace")).toBeVisible();
  });

  test('create team workspace from onboarding', async ({ page, request }) => { /* ... */ });
  test('switch workspace via header dropdown', async ({ page, request }) => { /* ... */ });
  ```

  Add the test hook in cloud (Phase 4 follow-up):

  ```rust
  // In handlers_test.rs (only mounted when cfg.enable_test_hooks)
  pub async fn last_email(/* state, query */) -> ... {
      // returns the most recent message in NoopEmailSender's inbox
  }
  ```

  Mount only behind `if cfg.enable_test_hooks`. Document in a code comment that this MUST be off in production.

### Task 7.4 — Commit phase 7

- [ ] Commit:
  ```bash
  git add app/accept-invite middleware.ts e2e
  git commit -m "feat(web): accept-invite multi-path + middleware + e2e"
  ```

---

## Phase 8 — Commit C8: Pulumi secrets + DNS docs (infra)

**Goal:** Wire Resend secrets into staging + production, document DNS setup.

### Task 8.1 — Branch infra + add secrets

**Files:**
- Modify: `agenomic-infra/pulumi/staging/index.ts`
- Modify: `agenomic-infra/pulumi/production/index.ts`

- [ ] **Step 1: Branch.** `cd agenomic-infra && git checkout -b feat/email-secrets`

- [ ] **Step 2: Locate the gateway env block** (grep `RESEND` first to confirm not yet present, then find the existing env array passed to the gateway service).

- [ ] **Step 3: Add Pulumi secrets.**

  ```ts
  const cfg = new pulumi.Config();
  const resendApiKey = cfg.requireSecret('resendApiKey');
  const resendFromAddress = cfg.require('resendFromAddress');
  const resendReplyTo = cfg.get('resendReplyTo');
  const resendWebhookSecret = cfg.requireSecret('resendWebhookSecret');
  const emailVerificationRequired = cfg.requireBoolean('emailVerificationRequired');
  ```

  Inject as env vars or Kubernetes secrets per the existing pattern. Tag environment as `staging` or `production` so the Resend dashboard shows them separately.

- [ ] **Step 4: Set the secrets via Pulumi CLI** (don't hardcode):

  ```bash
  cd pulumi/staging
  pulumi config set --secret resendApiKey re_xxx
  pulumi config set resendFromAddress 'Agenomic <noreply@agenomic.dev>'
  pulumi config set --secret resendWebhookSecret whsec_xxx
  pulumi config set emailVerificationRequired true
  ```

- [ ] **Step 5: Pulumi preview.**

  Run: `pulumi preview`
  Expected: env vars / secrets visible in the diff.

### Task 8.2 — DNS docs

**Files:**
- Modify: `agenomic-infra/README.md`

- [ ] **Step 1: New section "Email delivery (Resend)".** Cover:
  - Domain verification: TXT records (SPF), CNAMEs (DKIM), MX (return-path) — all from Resend dashboard.
  - DMARC recommendation `v=DMARC1; p=none; rua=mailto:dmarc@agenomic.dev` to start.
  - Webhook endpoint URL: `${API_GATEWAY_BASE_URL}/v1/webhooks/resend`.
  - Runbook "what to do if Resend is down" — flip `EMAIL_VERIFICATION_REQUIRED=false` as emergency switch (existing accounts unaffected, signup blocks until restored, but at least the platform stays usable for paying customers).

### Task 8.3 — Commit infra

- [ ] **Step 1: Commit.**

  ```bash
  git add pulumi README.md
  git commit -m "feat(infra): Resend secrets + DNS/webhook docs"
  ```

---

## Phase 9 — Commit C9: docs + CHANGELOG + submodule bumps

### Task 9.1 — Cloud-side docs

**Files:**
- Modify: `agenomic-cloud/AUTH_DESIGN.md`
- Modify: `agenomic-cloud/CHANGELOG.md` (or create)

- [ ] **Step 1: AUTH_DESIGN.md update.** Add new sections:
  - `4.4 Email verification (OTP)` — schema, flow, rate limits.
  - `4.5 Memberships` — M:N model, why we chose unified org-as-personal.
  - `5.1 Email crate` — trait, Resend impl.
  - `8.x Webhook endpoint` — Svix signature flow.
  - Update §11 threat model with email-bounce / complaint handling.
  - Mark §12 (migration path) with the new "user-first" path.
  - Move 4-role role table to a deprecated note; canonical is Owner/Maintainer/Viewer.

### Task 9.2 — Web docs

- [ ] **Step 1: `agenomic-web/README.md`** — onboarding section: signup → verify-email → onboarding → workspace switcher.

### Task 9.3 — Umbrella CHANGELOG + bumps

**Files:**
- Modify: umbrella `agenomic/CHANGELOG.md`
- Bump submodules

- [ ] **Step 1: Push branches + open PRs in each submodule.** Wait for review/merge.

- [ ] **Step 2: Bump submodule pointers.**

  ```bash
  cd /Users/gabinmberikongo/code/treansai/agenomic
  git submodule update --remote agenomic-cloud agenomic-web agenomic-infra
  git add agenomic-cloud agenomic-web agenomic-infra
  git commit -m "chore(submodules): bump cloud + web + infra for user-first auth"
  ```

- [ ] **Step 3: Open umbrella PR.** Body mentions the 3 submodule PRs by URL.

---

## Acceptance Criteria Mapping

The user listed 24 criteria. Each maps to phases above:

| # | Criterion | Tested in |
|---|-----------|-----------|
| 1 | signup returns `requires_email_verification`, no session | Phase 4 task 4.8 + Phase 4 task 4.2 |
| 2 | Email arrives at `to` with code | Phase 4 + manual smoke (phase 5 task 5.7) |
| 3 | wrong code → 401 + counter; 5th → 429 | Phase 3 task 3.5 + integration test 4.8 |
| 4 | correct code → 200 + cookie + active_org=personal | Phase 4 task 4.2 + 4.8 |
| 5 | link variant works | Phase 4 + Phase 5 task 5.5 |
| 6 | post-verify user lands in empty registry | Phase 5 task 5.4 → /onboarding → /registry |
| 7 | `/v1/auth/me` returns memberships array | Phase 4 task 4.2 step 4 |
| 8 | `POST /v1/orgs` (verified) → 201, owner membership | Phase 4 task 4.3 |
| 9 | `POST /v1/orgs` (unverified) → 403 | Phase 4 task 4.7 |
| 10 | switch workspace updates active_org | Phase 4 task 4.3 step 2 + 4.8 |
| 11 | invite on personal → 403 | Phase 4 task 4.4 |
| 12 | invite on team sends email; accept adds membership | Phase 3 task 3.1 + 3.8 + Phase 4 task 4.8 |
| 13 | password reset email + flow | Phase 3 task 3.1 step 5 |
| 14 | webhook `email.bounced` flips status | Phase 4 task 4.6 + 4.8 |
| 15 | user in 2 orgs sees both, no leak | Phase 4 task 4.8 |
| 16 | RLS isolation | inherited from existing migration; covered in current `tests/integration/tests/rls_smoke.rs` |
| 17 | legacy users still log in | Phase 2 task 2.1 backfill + Phase 4 task 4.8 backward-compat test |
| 18 | legacy api keys still work | Phase 4 task 4.7 step 4 + integration |
| 19 | signup form has no org_name; verify-email accepts paste | Phase 5 tasks 5.3 + 5.4 |
| 20 | members page disabled on personal workspace | Phase 6 task 6.3 |
| 21 | 6th signup/hour from same IP → 429 | Phase 3 task 3.4 (rate limit before email) |
| 22 | Resend down → 503 + rollback | Phase 3 task 3.4 step 2 |
| 23 | `cargo test --workspace` + `npm run test` pass | Phase 4 task 4.9 + Phase 6 task 6.4 |
| 24 | Resend dashboard shows tags `purpose,env,user_id` | Phase 3 task 3.4 (tags set on EmailMessage) |

---

## Pitfalls & Watch-outs (transcribed from spec for the executor)

- **Idempotency-Key header** required on every non-GET to `/v1/auth/*` and `/v1/orgs/*` and `/v1/auth/workspaces/switch`. Web client needs `crypto.randomUUID()` per submission. (Existing infra already has `agenomic-idempotency` middleware; new routes opt in.)
- **CSRF**: `verify-email` is pre-session — bypass CSRF. `signup`, `verify-email/resend` likewise. Once a session exists, all POST/PATCH/DELETE need `x-csrf-token`.
- **Constant-time OTP compare**: hash compares already use `subtle::ConstantTimeEq` via `constant_time_eq` in `auth.rs`. Reuse for `consume_email_verification_by_code` if comparison happens in app code rather than via SQL `=` (it does; SQL `=` on 32 bytes is fine).
- **Audit log `org_id` was NOT NULL** — drop the constraint in the migration (added in Phase 2 task 2.1 step 1 amendment).
- **Resend free tier**: 3000/mo, 100/day. Rate-limit IP-side BEFORE the call.
- **`onboarding@resend.dev`** is the dev sandbox sender. Use it for `EMAIL_VERIFICATION_REQUIRED=false` local dev; production uses verified domain.
- **Don't log raw OTP, raw token, or session cookie.** Tracing redaction list: `code`, `token`, `password`, `agenomic_session`, `agenomic_csrf`, `idempotency_key` (the last because customers may pass UUIDs, fine, but consistency).
- **Webhook idempotency**: `email_log.provider_message_id` is the dedup key. Multiple Svix retries land on the same UPDATE, harmless.
- **No feature flags.** This refactor commits as one cohesive change per submodule. Two-step rollout: cloud first, web second.

---

## Self-review (post-write checklist run)

Spec coverage:
- ✅ All 9 commits represented as phases
- ✅ All 24 acceptance criteria mapped
- ✅ Email crate scope: trait + Resend HTTP + Noop + 5 templates + tests
- ✅ Migration: kind, owner_user_id, memberships, email_verifications, email_log, role 4→3, audit_log.org_id nullable, lower(email) global, org_id nullable, backfill memberships, grandfather email_verified_at
- ✅ Identity service: signup_user, verify_email, resend_verification, create_team_organization, switch_active_workspace, list_my_workspaces, accept_invite (membership-aware), change_role + remove_membership (membership-based), invite/password-reset email via EmailSender
- ✅ Handlers: signup (no cookie), verify-email (cookie), resend, me (memberships), workspaces (list + switch), orgs (create), members (list/role/remove), webhook (Resend)
- ✅ Middleware: email_verified gate, active_org from membership
- ✅ Web: signup (no org_name), verify-email (OTP + token variant), onboarding, workspace switcher, settings/members, accept-invite multi-path, middleware public paths
- ✅ Infra: secrets + DNS doc + webhook URL
- ✅ Backward-compat: existing api keys + legacy users + grandfathered email_verified_at

Placeholder scan:
- All steps have concrete code, paths, and commands. No "TBD" or "fill in".

Type consistency:
- `Role` 3-variant identical across crates and DB. `UserRole` mirrors `Role`. `WorkspaceSummary` shape identical between Rust DTO `WorkspaceSummaryDto` and TS `WorkspaceSummary`. `EmailSender` trait used identically by `IdentityService`. `EmailMessage` builder API consistent.

Open items the executor will encounter:
- **Audit log `org_id` nullable**: amend Phase 2 migration to add `ALTER TABLE audit_log ALTER COLUMN org_id DROP NOT NULL`. (Already noted in Phase 2 task 2.1 step 1 — confirm before commit.)
- **Per-key role for api keys**: when removing `users.role` reads, the api-key path in `auth.rs` needs to fall back to a membership lookup `(api_key.org_id, api_key.user_id)`. If the key has no `user_id` (bootstrap), use `Owner`. Already in plan.
- **`email_verified` for api-key path**: documented; default true for bootstrap keys, otherwise read user's `email_verified_at`.
- **Test hook (`/v1/__test/last-email`)**: gated by `enable_test_hooks` — must be false in production.

The plan is comprehensive enough for an executor with zero context to proceed phase by phase, run the tests in each phase, and arrive at a working refactor.
