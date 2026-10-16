# End-user auth

Everything about how end users log in. For service-to-service auth see
`docs/internal-auth.md`.

## Two paths, one dormant

- **Email+password** is the live customer-facing path.
  `emailAndPassword.enabled: true` at `src/lib/server/auth.ts:41`.
- **Magic-link** stays in the plugin array (`src/lib/server/auth.ts:76`)
  but the UI toggle only renders behind `import.meta.env.DEV` at
  `src/routes/login/+page.svelte:13` and the dev-only mode button at
  `src/routes/login/+page.svelte:325`. Production builds tree-shake it
  out. Rationale: `mealsonthemove.ca` is not yet a verified Resend
  sender, so delivery is unreliable.

## Password rules

- `minPasswordLength: 10` — `src/lib/server/auth.ts:44`.
- `autoSignIn: true` on signup — `src/lib/server/auth.ts:45`.
- Per-email throttle: 5 failures / 15 min, enforced in a Better Auth
  `before` hook at `src/lib/server/auth.ts:63`. Backed by the in-memory
  sliding window in `src/lib/server/rate-limit.ts:16`.
- Rate limiter is in-memory → a redeploy resets the window. Accepted
  tradeoff for a single-instance Railway deploy.

## Where the hash lives

Better Auth `account` table, `provider_id='credential'`, `password`
column. The column is provisioned by migration 001; `008_password_auth.sql`
is an idempotent sanity re-declaration so a future operator grepping
"password" in `scripts/migrations/` finds something.

Hashing is Better Auth's own `ctx.password.hash` — used at
`src/routes/admin/companies/[id]/+page.server.ts:200` on create and
`:275` on reset. **No cleartext is stored anywhere.** The generated
password is returned to the admin UI exactly once in the action
response (`:237`) and discarded on refresh.

## Admin bootstrap

`scripts/seed-admin.ts` creates the first admin from `ADMIN_EMAILS[0]`
+ `ADMIN_INITIAL_PASSWORD`. Idempotent. Runs on container start — see
`Dockerfile` (`pnpm run seed:admin && node build`; migrations run on
the Rust service's own startup via `sqlx::migrate!`). Rotate the
password from the admin UI after first login; the env var can then be
unset.

## Client onboarding

Operator flow for a new customer:

1. `/admin/companies/new` — create the company record.
2. `/admin/companies/[id]` — on the company page, use **Create user
   with password**. Handler at
   `src/routes/admin/companies/[id]/+page.server.ts:190` generates a
   random password via `src/lib/utils/generate-password.ts`, creates the
   user through Better Auth's `internalAdapter.createUser` +
   `linkAccount`, then stamps `company_id` atomically through the Rust
   `/api/users/:id/assign-company` endpoint
   (`+page.server.ts:225`).
3. The admin copies the one-time generated password off the page and
   shares it out of band. On refresh the password is gone — it only
   exists in the action-return value.

Resetting an existing user's password uses the same flow via the
`resetUserPassword` action at
`src/routes/admin/companies/[id]/+page.server.ts:244`. Lookups are
company-scoped (Rust returns 404 for cross-company ids) so an admin
viewing `/companies/X` cannot probe ids owned by `/companies/Y`.

## Role dispatch

`admin` plugin stores `role` on the user record. The `signIn.email`
response from Better Auth omits plugin-added fields, so the login page
re-fetches via `getSession()` and dispatches:

- `role === 'admin'` → `/admin/dashboard`
- otherwise → `/client`

See `routeByRole` at `src/routes/login/+page.svelte:17`.

## Sessions

Better Auth stores sessions in the Postgres `session` table (migrations
001 + 003). Default TTLs — not overridden. SvelteKit reads them on
every request:

- Hook `handleBetterAuth` at `src/hooks.server.ts:8` resolves the
  session, writes `event.locals.user` and `event.locals.session`.
- `App.Locals` typing at `src/app.d.ts:21`.
- Every `+page.server.ts` reads `locals.user` and passes it to
  `callerFromLocals(locals)` before calling the Rust API.

## How the Rust side knows who you are

It doesn't do its own user auth. SvelteKit's `callerFromLocals`
(`src/lib/server/api-client.ts:50`) packages `userId` + `role` +
`companyId` from `locals.user`; `internalFetch`
(`src/lib/server/api-client.ts:72`) HMAC-signs the request. Full
protocol in `docs/internal-auth.md`. Verifier:
`admin-operations-rust/crates/api/src/auth.rs:180`.
