# GHCR as artifact of record — handoff

## What changed

Prod images are now built in CI (not on Railway), pushed to GHCR, and
Railway pulls them on redeploy. Toolchain drift between local, CI, and
Railway's Docker builder is eliminated: the exact binary CI verified
is the one that runs.

**New flow on push to main:**

1. CI's `ci`, `rust-ci`, `e2e` jobs run as before.
2. On green, `build-rust-image` + `build-svelte-image` push two tags
   per service to GHCR:
   - `ghcr.io/matthewdehaas/mom-rust-api:latest`
   - `ghcr.io/matthewdehaas/mom-rust-api:<sha>`
   - `ghcr.io/matthewdehaas/mom-svelte:latest`
   - `ghcr.io/matthewdehaas/mom-svelte:<sha>`
3. GHCR retention step prunes to the newest 10 versions per package.
4. `deploy.yaml` fires on CI completion, calls `railway redeploy -s
   <service> -e production -y` for each service.
5. Railway re-pulls `:latest` from GHCR and swaps the running
   container.

**No source upload to Railway.** No `railway up`. No `docker build`
outside CI.

## One-time dashboard actions (required for first rollout)

Both services, in the Railway dashboard:

1. **Switch Source to Docker Image.**
   - Service → Settings → Source → "Docker Image".
   - Image: `ghcr.io/matthewdehaas/mom-rust-api:latest` (for Rust-API)
     or `ghcr.io/matthewdehaas/mom-svelte:latest` (for mom-svelte).
   - Credentials: none required once the package is public (step 2).

2. **Make the GHCR package public** (after the first CI push succeeds):
   - https://github.com/users/MatthewdeHaas/packages/container/mom-rust-api/settings
   - https://github.com/users/MatthewdeHaas/packages/container/mom-svelte/settings
   - Danger Zone → "Change visibility" → Public.
   - Why public: avoids plumbing a GH PAT into Railway's image-pull
     credentials. The images contain no secrets (secrets live in
     Railway's per-service env). Admin app only, no sensitive binary
     content.

3. **Delete legacy env/vars** on each service (optional cleanup):
   - Any `NIXPACKS_*` or `RAILWAY_DOCKERFILE_PATH` overrides.

## Rollback

Fast path, from the Railway dashboard:
- Service → Settings → Source → change image tag from `:latest` to
  `:<previous-sha>` → Redeploy.
- Up to 10 previous `:<sha>` tags are retained (see CI's
  `Prune old GHCR versions` step).

Slow path:
- `git revert <bad-sha> && git push origin main` — CI builds and
  pushes a new `:latest`, `deploy.yaml` redeploys.

## Why this layout

- **Public GHCR, free tier.** `delete-package-versions@v5` keeps the
  newest 10 per package; `:latest` is always in that set because it's
  always the most recent push.
- **Nix for the Rust binary.** `nix build .#api` is already green in
  the `e2e` job; `build-rust-image` reuses the same derivation so the
  binary that runs in prod is the one `e2e` tested against.
- **Alpine for SvelteKit.** `node:22-alpine` matches the previous
  Dockerfile's runtime base — same libc, same shell.
- **Workflow-scoped buildx cache.** Each image gets its own `scope=`
  so a Rust change doesn't bust the SvelteKit layer cache and vice
  versa.
- **`railway redeploy`, not `railway up`.** `up` uploads source and
  rebuilds on Railway. `redeploy` just re-pulls the image — the whole
  point of this migration.
