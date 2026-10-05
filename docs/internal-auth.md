# Internal auth: SvelteKit ↔ Rust

Both services sit on the same Railway private network. We don't expose
the Rust service to the public internet. Even so, the SvelteKit image
could leak or an egress rule could regress — so every request between
them is HMAC-signed and timestamp-bound. Belt and braces.

## Protocol

Six headers, all required:

| Header | Value |
|---|---|
| `X-Internal-User` | Better Auth user id (opaque string) |
| `X-Internal-Role` | `admin` or `client` (lowercase) |
| `X-Internal-Company` | UUID or empty string when unscoped |
| `X-Internal-Timestamp` | Unix seconds (integer, as string) |
| `X-Internal-Nonce` | ≥16 random bytes, hex-encoded (so ≥32 chars) |
| `X-Internal-Auth` | `hex(HMAC-SHA256(secret, payload))` |

**Canonical payload** (pipe-delimited, no trailing newline):

```
<user_id>|<role>|<company_id_or_empty>|<timestamp>|<nonce>
```

Any change to the field order, delimiter, or encoding breaks every
request. The signer (`src/lib/server/api-client.ts`) and the verifier
(`admin-operations-rust/crates/api/src/auth.rs`) must land such a
change in the same commit.

## Verification rules (Rust side)

1. All six headers present and ASCII.
2. `|timestamp - now| ≤ 60s`.
3. `len(nonce) ≥ 32`.
4. HMAC matches (constant-time compare).
5. Nonce not seen in the last 60s (in-memory `DashMap`, sweep every 5s).

Failures all return `401 Unauthorized` with no body — the reason is
logged on the Rust side at `warn`.

### Why both a timestamp window *and* a nonce cache?

The timestamp alone leaves a 60-second replay window where a captured
request can be re-sent verbatim. The nonce cache closes that window:
first use succeeds, every subsequent use inside the cache's TTL (also
60s) is rejected as `nonce replay`. The two TTLs match deliberately —
there is no window where a nonce's signature is still valid but the
cache has forgotten it.

Order matters. The Rust verifier checks HMAC *before* consulting the
nonce cache. If we cached nonces first, an attacker could poison the
cache with garbage nonces and signatures, forcing legitimate requests
to fail. HMAC-first means only a well-formed request even touches the
cache.

## Secret management

- `INTERNAL_AUTH_SECRET` — ≥32 bytes, generated with `openssl rand -hex 32`.
- Set on both services in Railway (and in `.env` locally).
- Rotation: deploy both services with the new secret in the same
  Railway rollout. In-flight requests signed with the old secret will
  401 for the duration of the rollover — this is an accepted ~seconds
  of blips, not a correctness problem (SvelteKit surfaces internal
  errors to the user as a toast and the client retries manually).

## Local development

`docker-compose up` brings both services up together with a shared
secret from `.env`. The Rust container builds its own image per
`admin-operations-rust/Dockerfile`.

For bare-metal Rust dev (faster incremental builds):

```sh
pnpm rust:sync-migrations   # populate admin-operations-rust/migrations/
cargo run --manifest-path admin-operations-rust/Cargo.toml --bin api
```

Then run SvelteKit against it:

```sh
RUST_API_URL=http://localhost:8080 pnpm dev
```

## Testing

- The Rust verifier has unit tests covering: valid request, tampered
  payload, old timestamp, replay, UUID parse, missing header. Run via
  `pnpm rust:test`.
- Integration between the two services lands with the first ported
  domain (Phase 3 of the Rust migration).
