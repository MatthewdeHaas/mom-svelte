# Feature flags

Feature flags gate incomplete or risky functionality. Precedence at
resolve time, highest-first:

1. `feature_flag_company.enabled` — per-company override
2. `feature_flag.enabled_global` — fleet-wide default
3. `Flag::DEFAULT` — compile-time fallback (every flag today: `false`)

The flag catalog is Rust-side ZSTs in
`admin-operations-rust/crates/flags/src/lib.rs:26-56`; the DB schema is
`scripts/migrations/009_feature_flags.sql`. SvelteKit never touches the
tables directly — it asks the Rust API (`/api/flags/check`) which flags
are enabled for the current `(user, company)`.

## 1. Adding a new flag

Three steps, in order:

1. **Register the ZST.** Add a struct to
   `admin-operations-rust/crates/flags/src/lib.rs` and append it to
   `catalog()`:

   ```rust
   pub struct MyFeature;
   impl Flag for MyFeature {
       const NAME: &'static str = "my_feature";
       const DESCRIPTION: &'static str = "What it does.";
   }
   // ...then in `catalog()`:
   d::<MyFeature>(),
   ```

2. **Run the TUI** (see section 4). It fetches the current DB state,
   notices `my_feature` isn't seeded, and emits a migration with an
   `INSERT ... ON CONFLICT (name) DO NOTHING` for it (default `FALSE`).
   You don't have to flip anything in the grid for the seed row to
   appear — the emitter detects "in catalog, not in DB".

3. **Commit the emitted migration file.** PR, merge, Railway redeploys,
   `sqlx::migrate!` picks it up on Rust API startup.

## 2. Toggling an existing flag

Just run the TUI (section 4). Flip the cell you care about with the
arrow keys + space, hit `w`, commit the emitted file. No code change
needed — flags are data.

A few shapes:

- **Fleet-wide on/off** — move the cursor to the `GLOBAL` column,
  press space (or `g` from any column in the same row).
- **One company only** — leave `GLOBAL` as-is, move to that company's
  column, press space to set `✓` (explicitly enabled) or `✗`
  (explicitly disabled). `·` means "no override" — the company falls
  through to the global default.
- **Clear a per-company override** — space-cycle back to `·`.

The migration body contains only the diff vs. what was in prod at the
moment the TUI pulled its snapshot. Idempotent SQL (`ON CONFLICT DO
UPDATE` for upserts, `DELETE WHERE ...` for clears) so a redeploy that
re-runs the migration against a DB already in the target state is a
no-op.

## 3. Migration-as-truth model

There is no live write path. The TUI holds no credentials for prod
writes; the Rust API has no endpoint that mutates `feature_flag` or
`feature_flag_company` at request time. **The only way a flag change
reaches prod is a `scripts/migrations/NNN_flags_*.sql` file applied
through the normal Railway redeploy chain.**

Why:

- **Audit by `git log`.** Every change is a reviewable commit by a
  named author.
- **Reversible.** Flipping a flag back is a new migration; `git
  revert` + emit is the same cost as the original change.
- **No ambient write surface.** A compromised admin session can't flip
  a flag; a compromised Railway deploy key can't either without a
  migration file in a PR.
- **Repo write access is the gate** — exactly what the user asked for.

See `docs/architecture.md`'s "Feature flags" section for the resolver
code pointers and the snapshot endpoint's threat model.

## 4. Running the TUI

### Env vars

The TUI needs three values from the operator's shell:

```sh
# Shared with the Rust service — same 32+ byte secret both sides HMAC with.
export INTERNAL_AUTH_SECRET='<secret>'

# Target service. Prod on Railway or `http://localhost:8080` for local dev.
export RUST_API_URL='https://<rust-service>.up.railway.app'

# Operator user id. Must match the Rust service's FLAGS_SNAPSHOT_USER_ID
# (which must also be set on the server — otherwise the snapshot
# endpoint fail-closes with 503).
export FLAGS_SNAPSHOT_USER_ID_LOCAL='<your-user-id>'
```

`FLAGS_SNAPSHOT_USER_ID_LOCAL` is deliberately a different name than
the server's `FLAGS_SNAPSHOT_USER_ID` so a leaked `.envrc` on a dev
box can't accidentally reconfigure prod's gate.

### Invocation

```sh
bash -c '. .direnv/flake-profile-*.rc && cd admin-operations-rust && cargo run -p flags-tui'
```

The TUI fetches `GET /api/flags/snapshot` once at startup. After that
every edit is in-memory until `w` writes the migration.

### Keyboard

| Key          | Action                                                       |
| ------------ | ------------------------------------------------------------ |
| `↑ ↓ ← →`    | Move cursor                                                  |
| `space`      | Cycle current cell: `·` → `✓` → `✗` → `·`                    |
| `g`          | Toggle `GLOBAL` for the current flag's row                   |
| `w`          | Write migration file to `scripts/migrations/` and exit       |
| `q` / `Esc`  | Quit without writing                                         |
| `?`          | Toggle inline help overlay                                   |

### Legend

- `·` — **unset**. The cell has no `feature_flag_company` row; the
  company falls through to `GLOBAL`, which itself falls through to the
  compile-time default. The `GLOBAL` column can't be unset — a new
  flag not yet in the DB shows its compile-time default there.
- `✓` — explicitly enabled.
- `✗` — explicitly disabled.
- A **bold / underlined** cell has been edited this session.

### What `w` writes

A single `scripts/migrations/NNN_flags_<UTC-timestamp>.sql` file where
`NNN` is one greater than the highest existing `NNN_*.sql` prefix. The
file contains only the delta vs. the snapshot (plus any new-catalog
seed rows); no diff → no file.

Section layout:

```sql
-- flags migration — generated by flags-tui at 2026-10-09T12:00:00Z
-- Snapshot source: https://rust.up.railway.app
-- Signed by: <operator-user-id>

-- New flags discovered in catalog but not yet in DB:
INSERT INTO feature_flag (name, description, enabled_global)
  VALUES ('insights', 'Enable the insights dashboard …', FALSE)
  ON CONFLICT (name) DO NOTHING;

-- Global default changes:
UPDATE feature_flag SET enabled_global = TRUE WHERE name = 'insights';

-- Per-company overrides (upserts + deletes):
INSERT INTO feature_flag_company (flag_name, company_id, enabled)
  VALUES ('insights', '…'::uuid, TRUE)
  ON CONFLICT (flag_name, company_id) DO UPDATE SET enabled = EXCLUDED.enabled;

DELETE FROM feature_flag_company
  WHERE flag_name = 'allergens' AND company_id = '…'::uuid;
```

Commit the file, open a PR, merge → Railway redeploys →
`sqlx::migrate!` picks it up when the Rust API binary starts.

## 5. Drift mirror at `/admin/flags`

`/admin/flags` is a read-only side-by-side view of **committed state**
(parsed server-side from `scripts/migrations/*.sql`) vs. **live DB
state** (via `GET /api/flags/drift`). Rows where the two disagree are
highlighted — handy when a migration has been merged but hasn't rolled
out yet (Railway redeploy in flight), or when someone has reached
into prod DB by hand (don't).

No edit controls. The TUI is the only editor; see section 4.
