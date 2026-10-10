# Handoff — Track 4b: GHCR Docker image pipeline

Fresh-session doc. 4a's rewrite (Infisical removed, Railway owns
secrets directly) landed on `main`. 4b is the tactical optimization
that was deferred during 4a: build images in GHA, push to GHCR,
switch Railway to pull from the image registry instead of building
from source on every deploy.

4a shipped a real deploy at least once before you start 4b.

## Why 4b exists

Today, every push triggers `deploy.yaml` → `railway up` → Railway
clones the repo and runs the Dockerfile build on its own build
infrastructure. Three downsides:

- **Build minutes burned twice.** CI already builds equivalent
  artifacts (typecheck, test, Rust compile); Railway re-runs the
  same work during image build. ~5 min of duplicate work per deploy.
- **No image audit trail.** You can't `docker pull` the exact bytes
  that are serving prod to inspect them locally. Railway's build
  logs are the only record.
- **No rollback by sha.** To revert, you push a revert commit; you
  can't say "ship the image that was green 20 minutes ago."

GHCR pipeline fixes all three: GHA builds the image once, pushes to
`ghcr.io/<owner>/admin-operations-{svelte,rust}:<sha>`, Railway's
"Deploy from Docker Image" source pulls it. Deploy becomes "point
Railway at this image tag."

## Preconditions

Verify ALL of these before starting. If any is false, STOP.

- `origin/main` has `deploy.yaml` running `railway up` only (no
  Infisical, no allowlist). Grep: `grep -i infisical
  .github/workflows/*.yaml` returns nothing.
- At least one green `workflow_run → deploy.yaml` has fired on
  `origin/main` and both services are healthy at:
  - `https://mom-svelte-production.up.railway.app/healthz` → 200
    with `"status":"ok"`
  - `https://mom-svelte-production.up.railway.app/healthz/deep` →
    200 with `"rust":"ok"`
- `RAILWAY_TOKEN` GH repo secret exists and is project-scoped to
  `striking-encouragement`.
- Railway dashboard has all current prod secrets populated per
  service. (You edited them directly in Railway during 4a close-out
  — Railway is now the source of truth.)

If `/healthz/deep` is red or you haven't seen a real deploy succeed,
finish 4a first.

## Standing user directives (verbatim — do NOT deviate)

- *"I would like everything to be in Rust. If it requires business
  logic or data layer interaction, it should be in Rust. Typescript
  should be for client side interaction and handling http
  interactions, and should not have any logic at all."*
- *"Please don't say nix develop. Just run cargo ... as it is on the
  repo's path with the flake."*
- *"no cleartext stored anywhere"*
- *"please use the convention *.yaml instead of *.yml please"*
- Commits stay local until the user explicitly asks to push.
- Agent runs verification gates itself (user granted permission
  2026-10-09; read `pnpm`/`cargo` output carefully, don't declare
  green on red).

## Flake / cargo invocation

```sh
bash -c '. .direnv/flake-profile-*.rc && cd admin-operations-rust && cargo ...'
```

Postgres via `docker compose up -d db`.

## Scope

Four changes:

1. New GHA workflow `.github/workflows/build-images.yaml` that
   builds both Docker images and pushes to GHCR on green CI.
2. Rewrite `.github/workflows/deploy.yaml` to depend on
   `build-images` (not `CI`) and to tell Railway which image tag to
   pull (via `railway redeploy` or by setting the service's image
   source via API).
3. Switch both Railway services from "Deploy from Repo" to "Deploy
   from Docker Image" pointing at the `:latest` tag.
4. Update `docs/ops.md` to document the image pipeline.

## Step-by-step

### 1. New workflow: `.github/workflows/build-images.yaml`

Triggered after CI succeeds on `main` (same `workflow_run` pattern
`deploy.yaml` uses today). Builds both images in parallel, pushes
to GHCR tagged with both `:latest` and `:<sha>`.

```yaml
name: Build images

on:
  workflow_run:
    workflows: ["CI"]
    types: [completed]
    branches: [main]
  workflow_dispatch:

concurrency:
  group: build-images
  cancel-in-progress: false

jobs:
  build:
    name: Build ${{ matrix.service }}
    runs-on: ubuntu-latest
    if: >
      github.event_name == 'workflow_dispatch' ||
      github.event.workflow_run.conclusion == 'success'
    permissions:
      contents: read
      packages: write

    strategy:
      fail-fast: false
      matrix:
        include:
          - service: admin-operations-svelte
            dockerfile: Dockerfile
            context: .
          - service: admin-operations-rust
            dockerfile: admin-operations-rust/Dockerfile
            context: .

    steps:
      - uses: actions/checkout@v6

      - uses: docker/setup-buildx-action@v3

      - uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - name: Resolve lowercase owner
        id: owner
        run: echo "name=$(echo '${{ github.repository_owner }}' | tr '[:upper:]' '[:lower:]')" >> "$GITHUB_OUTPUT"

      - uses: docker/build-push-action@v6
        with:
          context: ${{ matrix.context }}
          file: ${{ matrix.dockerfile }}
          push: true
          tags: |
            ghcr.io/${{ steps.owner.outputs.name }}/${{ matrix.service }}:latest
            ghcr.io/${{ steps.owner.outputs.name }}/${{ matrix.service }}:${{ github.sha }}
          cache-from: type=gha
          cache-to: type=gha,mode=max
```

Notes:
- `GITHUB_TOKEN` with `packages: write` is enough; no PAT needed
  for same-repo GHCR pushes.
- `cache-from: type=gha` plus `cache-to: type=gha,mode=max` reuses
  layers across runs — turns 5-minute builds into 30-second ones
  on no-dep changes.
- Owner-name must be lowercased for GHCR path. `github.repository_owner`
  can be mixed case; the shell step normalizes it.
- Both images get `:<sha>` so rollback is possible by sha.

### 2. Rewrite `deploy.yaml`

The new flow chains `build-images → deploy`. Deploy's job becomes
telling Railway to redeploy from the new `:latest` tag.

```yaml
name: Deploy

on:
  workflow_run:
    workflows: ["Build images"]
    types: [completed]
    branches: [main]
  workflow_dispatch:

concurrency:
  group: deploy-production
  cancel-in-progress: false

jobs:
  deploy:
    name: Deploy ${{ matrix.service }}
    runs-on: ubuntu-latest
    if: >
      github.event_name == 'workflow_dispatch' ||
      github.event.workflow_run.conclusion == 'success'

    strategy:
      fail-fast: false
      matrix:
        service: [mom-svelte, Rust-API]

    steps:
      - uses: actions/checkout@v6

      - name: Install Railway CLI
        run: npm install -g @railway/cli@3

      - name: Trigger Railway redeploy
        env:
          RAILWAY_TOKEN: ${{ secrets.RAILWAY_TOKEN }}
        run: |
          railway redeploy \
            --service "${{ matrix.service }}" \
            --yes
```

`railway redeploy` tells Railway to pull the latest image for the
service and swap traffic. The service's image source is configured
once in Railway's UI (step 3 below) and never changes afterwards —
`:latest` always points at the newest green build.

Watch out: `railway redeploy` has historically been named
`railway service redeploy` in some CLI versions. Pin `@railway/cli@3`
and check `railway redeploy --help` before relying on the exact
subcommand name.

### 3. Flip Railway service image source

In Railway's dashboard, for each service (`mom-svelte` and
`Rust-API`):

1. Settings → Source. Change from "GitHub Repo" to "Docker Image".
2. Image: `ghcr.io/<lowercase-owner>/admin-operations-svelte:latest`
   for `mom-svelte`; `admin-operations-rust:latest` for `Rust-API`.
3. If the image is private (default for new GHCR packages — the
   repo's visibility doesn't automatically apply to its packages),
   add a GitHub Personal Access Token with `read:packages` scope as
   the registry credential. Set the username to your GitHub handle,
   password to the PAT. Alternatively, make the package public via
   GHCR's UI (`Package settings → Change visibility`); then no
   credential is needed. Public is fine for most non-sensitive
   service images, but if the images bake any secrets at build time
   (they shouldn't — audit the Dockerfiles first), keep them
   private.
4. Keep the Variables tab (secrets) exactly as it is. Image source
   change does NOT touch variables.
5. Keep `healthcheckPath` as `/healthz`. Confirm via
   `railway.toml` — the file still ships with the repo and Railway
   reads it on each deploy even for image-source services (it reads
   `[deploy]` config, ignores `[build]` config since Docker builds
   are now in GHA).

### 4. Update `docs/ops.md`

Add a "Container images" section between "Services" and
"Env-var inventory":

```markdown
## Container images

Images build in GHA and ship via GHCR; Railway pulls from the
registry rather than building from source.

| Service | Image |
|---|---|
| `mom-svelte` | `ghcr.io/<owner>/admin-operations-svelte:latest` |
| `Rust-API`   | `ghcr.io/<owner>/admin-operations-rust:latest`   |

Every green CI run also pushes a sha-tagged image
(`:<commit-sha>`) alongside `:latest`. Rollback: in Railway's
service → Deployments → pick an older deploy → Redeploy. Or
override the image tag to a known-good sha.

Build workflow: `.github/workflows/build-images.yaml`. Deploy
chain: `CI → Build images → Deploy`. Deploy is a `railway
redeploy` only — no secret pushing.
```

And update the "Deploys" section's chain description:

```markdown
GitHub Actions → GHCR → Railway. CI runs on every push to `main`;
on a green run, `build-images.yaml` pushes new `:latest` + `:<sha>`
tags to GHCR; on its success, `deploy.yaml` fires and tells Railway
to redeploy both services from `:latest`.
```

## Verification gates — run yourself

**After each step:**

```sh
# Step 1 (build workflow added):
gh workflow run build-images.yaml
gh run watch
# Expect: both matrix jobs green; two new package versions in
# https://github.com/users/<owner>/packages

# Step 2 (deploy rewritten):
pnpm check                       # 0/0/0 (no code changes)
# Workflow syntax is lint-checked only by GHA itself; the gate is
# the next real deploy.

# Step 3 (Railway source flipped):
# In Railway dashboard, confirm each service's Source shows
# "Docker Image" and points at the right GHCR URL. No deploy yet.

# Step 4 (docs):
pnpm check                       # 0/0/0

# End-to-end gate (do this last):
git push origin main             # triggers CI
# Watch: gh run watch -R <owner>/<repo>
# CI green → Build images runs → both images push → Deploy fires
# → railway redeploy on each service → /healthz returns 200
curl -sSf https://mom-svelte-production.up.railway.app/healthz
curl -sSf https://mom-svelte-production.up.railway.app/healthz/deep
# Both should return 200 with "status":"ok".
```

## Standing concerns

1. **First `railway redeploy` may fail silently** if the Railway
   service's image source still points at the repo. Flip source
   FIRST (step 3), THEN trigger a deploy. Order matters.

2. **GHCR package visibility defaults to private.** First build
   succeeds; first Railway pull fails with "unauthorized" because
   no credential is configured. Either add the PAT credential
   (step 3.3) or flip visibility to public before the first deploy.
   Decide which during execution.

3. **Build cache can get stale.** `type=gha` cache is scoped to the
   repo. If a layer's inputs don't change but you need a rebuild
   (e.g. a sub-dependency upgraded transitively), the cache can
   serve old layers. Workaround: push a trivial change that
   invalidates the cache (bump a comment in the Dockerfile).

4. **`railway redeploy` CLI name.** Confirm the exact subcommand
   on `@railway/cli@3` before pushing. If it's `railway service
   redeploy` or similar, update the workflow.

5. **Rollback story.** Document in `docs/ops.md` how to roll back
   via Railway's Deployments tab (pick older sha, redeploy). This
   is the first time the project has a real rollback path — don't
   leave it undocumented.

6. **GHCR image retention.** 1-2 image pushes per weekday means
   ~500 images/year. GHCR charges for private storage over 500MB.
   Track 8 Item 5 handles this (`docs/handoff-track8.md`). Not a
   blocker; just know it accumulates.

## Decisions locked in — do NOT re-ask

- **Images build in GHA, not Railway.** Railway is a deploy
  surface, not a build surface.
- **GHCR, not Docker Hub.** Free tier, same auth as the repo,
  package tied to repo lifecycle.
- **`:latest` + `:<sha>` double-tagging.** `:latest` for the
  deploy workflow; `:<sha>` for rollback.
- **`railway redeploy`, not `railway up`.** `up` deploys from
  source; `redeploy` pulls the current image source. Only
  `redeploy` is compatible with image-source services.
- **No secret changes.** Variables stay in Railway's dashboard
  from the 4a close-out. 4b is image pipeline only.

## Pointers

- 4a summary: `docs/track-4a-summary.md`
- Operations guide: `docs/ops.md`
- Current deploy workflow: `.github/workflows/deploy.yaml`
- Current CI: `.github/workflows/ci.yaml`
- SvelteKit Dockerfile: `Dockerfile`
- Rust Dockerfile: `admin-operations-rust/Dockerfile`
- Railway configs: `railway.toml` (web),
  `admin-operations-rust/railway.toml` (Rust)

## After Track 4b

- Track 8 (`docs/handoff-track8.md`) — 5 operational hardening
  items. Item 5 (GHCR retention cron) becomes relevant once 4b has
  been running for ~a month and image count passes ~30.
- Nothing else from the original plan is open.

Stay local. Do NOT push until gates pass.
