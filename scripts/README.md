# Local dev scripts

Three processes make up local dev. Two scripts orchestrate them:

- `./scripts/up.sh` — Postgres (compose) + Rust API (host via nix).
- `./scripts/down.sh` — stop both.

SvelteKit runs in a **third terminal** (`pnpm dev`). Keeping it
separate means Vite's logs don't drown out the Rust API log that
`up.sh` tails for you.

## Everyday loop

```sh
# terminal 1
./scripts/up.sh         # Postgres + Rust API; tails Rust log

# terminal 2
pnpm dev                # SvelteKit on :5173

# when done, in terminal 1 (Ctrl-C the tail first):
./scripts/down.sh       # stop both; data preserved
```

Re-running `./scripts/up.sh` is idempotent — Postgres is left alone
if already healthy, and the Rust API skips the start if a live
pidfile is present.

## Starting fresh

```sh
./scripts/up.sh --fresh         # wipe db, migrate, seed admin
./scripts/up.sh --fresh --seed  # wipe db, migrate, seed admin + dev data
```

`--fresh` nukes the `postgres_data` compose volume before starting.
Does **not** imply `--seed` — a fresh db just has migrations + the
admin upsert run against it. Add `--seed` explicitly if you want
`seed:dev`'s wipe-and-reseed on top (prompts before running).

## Admin login

`pnpm run seed:admin` runs on **every** `up.sh` invocation (it's an
idempotent upsert — safe to re-run). It reads two things from `.env`:

| Var | Purpose |
|---|---|
| `ADMIN_EMAILS` (or `ADMIN_EMAIL`) | Who gets seeded as an admin user. |
| `ADMIN_INITIAL_PASSWORD` | Their initial password. |

**If `ADMIN_INITIAL_PASSWORD` is empty**, the seed creates the user
row but **no credential account** — you can't log in with a password.
The seed script prints a warning in this case; check `up.sh`'s output.

Fix:

```sh
# edit .env, set:
ADMIN_INITIAL_PASSWORD=changeme-rotate-after-first-login

./scripts/up.sh   # re-runs seed:admin, attaches credential account
```

Rotate the password from the admin UI once you're in.

## Why Rust runs on the host, not in a container

The `admin-operations-rust/Dockerfile` is a **runtime-only** image
built by CI from a Nix-produced Linux ELF (`nix build .#api`). It
expects `target/release/api` to already be a Linux binary. On a dev
Mac, a local `cargo build` populates that path with a Mach-O
arm64/x86_64 binary; the Linux kernel in the container can't exec a
Mach-O and the container crashloops with:

```
rust-api-1 | exec /usr/local/bin/api: exec format error
```

`scripts/rust.sh` sidesteps this entirely: it enters the flake
devShell (`nix develop ..#default`) so you get the exact
`rust-bin.stable.latest` toolchain CI uses, then runs
`cargo run -p api --release` on the host. No cross-arch dance,
no stale-binary landmine.

Prod/CI are untouched — the Dockerfile still produces the GHCR image
Railway runs. The compose stack is now Postgres-only.

## Who runs migrations

The Rust API runs `sqlx::migrate!` on its own startup — see
`admin-operations-rust/crates/api/src/main.rs`. These scripts never
invoke a migrator directly; bringing the Rust API up applies any
pending migrations as a side effect.

## Flags

### `up.sh`

| Flag          | Effect                                                       |
| ------------- | ------------------------------------------------------------ |
| _(none)_      | Idempotent bring-up; tails Rust API log at the end.          |
| `--fresh`     | `docker compose down -v` first (destroys db volume).         |
| `--seed`      | Runs `pnpm run seed:dev` after admin seed (destructive).     |
| `--no-follow` | Skip tailing the Rust log; return the terminal.              |
| `-h`/`--help` | Print flag reference.                                        |

### `down.sh`

| Flag          | Effect                                                       |
| ------------- | ------------------------------------------------------------ |
| _(none)_      | Stop Rust API (via pidfile) and Postgres; keep data.         |
| `--volumes`   | Also wipe the postgres volume.                               |
| `-h`/`--help` | Print flag reference.                                        |

### `rust.sh`

Normally invoked by `up.sh` / `down.sh`; you can also run it alone
for foreground Rust logs (useful when debugging startup).

| Flag           | Effect                                                      |
| -------------- | ----------------------------------------------------------- |
| _(none)_       | Foreground: `cargo run -p api --release`, logs to terminal. |
| `--background` | Daemonize: redirect to `.rust-api.log`, write `.rust-api.pid`. Used by `up.sh`. |
| `-h`/`--help`  | Print flag reference.                                       |
