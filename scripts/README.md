# Local dev scripts

Two scripts form the local-dev loop:

- `./scripts/up.sh` — bring up Postgres + Rust API via docker-compose.
- `./scripts/down.sh` — tear down.

SvelteKit is **not** managed by compose. Run `pnpm dev` in a separate
terminal; it talks to the Rust API on `:8080` and Postgres on `:5432`.

## Everyday loop

```sh
./scripts/up.sh         # start stack, tail rust-api logs
# ... in another terminal:
pnpm dev                # SvelteKit on :5173
# ... when done:
./scripts/down.sh       # stop, data preserved
```

Re-running `./scripts/up.sh` is idempotent — containers already
running are left alone.

## Starting fresh

```sh
./scripts/up.sh --fresh         # wipe db, migrate, seed admin
./scripts/up.sh --fresh --seed  # wipe db, migrate, seed admin + dev data
```

`--fresh` nukes the `postgres_data` compose volume before starting.
Does **not** imply `--seed` — a fresh db just has migrations + the
admin upsert run against it. Add `--seed` explicitly if you want
`seed:dev`'s wipe-and-reseed on top (prompts before running).

## Who runs migrations

The Rust API runs `sqlx::migrate!` on its own startup — see
`admin-operations-rust/crates/api/src/main.rs`. These scripts never
invoke a migrator directly; bringing up `rust-api` applies any
pending migrations as a side effect.

## Flags

### `up.sh`

| Flag          | Effect                                                       |
| ------------- | ------------------------------------------------------------ |
| _(none)_      | Idempotent bring-up; tails `rust-api` logs at the end.       |
| `--fresh`     | `docker compose down -v` first (destroys db volume).         |
| `--seed`      | Runs `pnpm run seed:dev` after admin seed (destructive).     |
| `--no-follow` | Skip tailing logs; return the terminal.                      |
| `-h`/`--help` | Print flag reference.                                        |

### `down.sh`

| Flag          | Effect                                                       |
| ------------- | ------------------------------------------------------------ |
| _(none)_      | `docker compose down` — stops containers, keeps data.        |
| `--volumes`   | `docker compose down -v` — stops and wipes db volume.        |
| `-h`/`--help` | Print flag reference.                                        |
