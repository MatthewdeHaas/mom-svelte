# Meals on the Move — Railway Deployment Guide

## Prerequisites
- Railway account + project created at railway.app
- `railway` CLI installed: `npm i -g @railway/cli`
- GitHub repo connected to Railway project
- Resend API key ready

---

## Step 1 — Add the Node adapter (if not already done)

```bash
npm install @sveltejs/adapter-node
```

Update `svelte.config.ts`:

```ts
import adapter from '@sveltejs/adapter-node';
// ...
kit: {
  adapter: adapter()
}
```

Commit and push this change before continuing.

---

## Step 2 — Copy config files into your project root

From this package, copy to your repo root:

```
nixpacks.toml        → <repo-root>/nixpacks.toml
railway.toml         → <repo-root>/railway.toml
```

Copy the health route:

```
src/routes/health/+server.ts  →  <repo-root>/src/routes/health/+server.ts
```

Copy the workflow:

```
.github/workflows/ci.yml  →  <repo-root>/.github/workflows/ci.yml
```

---

## Step 3 — Set up Railway Postgres

1. In your Railway project, click **+ New** → **Database** → **PostgreSQL**
2. Once provisioned, click the Postgres service → **Variables** tab
3. Note the values for: `PGUSER`, `PGPASSWORD`, `PGDATABASE`, `PGHOST`

---

## Step 4 — Set Railway environment variables

In your Railway **web service** → **Variables** tab, add:

| Variable            | Value                                      |
|---------------------|--------------------------------------------|
| `POSTGRES_USER`     | from Railway Postgres (`PGUSER`)           |
| `POSTGRES_PASSWORD` | from Railway Postgres (`PGPASSWORD`)       |
| `POSTGRES_DB`       | from Railway Postgres (`PGDATABASE`)       |
| `POSTGRES_HOST`     | from Railway Postgres (`PGHOST`)           |
| `BETTER_AUTH_SECRET`| any long random string (use `openssl rand -base64 32`) |
| `BETTER_AUTH_URL`   | your Railway public URL (e.g. `https://meals.up.railway.app`) |
| `ADMIN_EMAIL`       | your admin email address                   |
| `RESEND_API_KEY`    | your Resend API key                        |

> **Tip:** In Railway you can use "Reference Variables" to pull Postgres values
> directly from the Postgres service — avoids copy-paste errors.
> Set them as: `${{Postgres.PGUSER}}` etc.

---

## Step 5 — Run migrations against production DB

Do this once before the first deploy. From your local machine with the
production DB credentials in your shell:

```bash
POSTGRES_HOST=<railway-host> \
POSTGRES_USER=<railway-user> \
POSTGRES_PASSWORD=<railway-password> \
POSTGRES_DB=<railway-db> \
npm run migrate
```

Or use the Railway CLI to open a shell:

```bash
railway run npm run migrate
```

---

## Step 6 — Seed the admin user against production DB

```bash
railway run npm run seed:admin
```

This only needs to run once. The seed script should be idempotent (safe to
re-run), but double-check if you're not sure.

---

## Step 7 — First deploy

```bash
railway login
railway link          # link to your existing project
railway up --detach   # triggers a build + deploy
```

Watch the build logs in the Railway dashboard. The build runs:
1. `npm ci`
2. `npm run migrate`   ← runs migrations on every deploy (safe, hash-verified)
3. `npm run build`     ← SvelteKit build

Then starts with `node build`.

---

## Step 8 — Verify

1. Visit `https://<your-app>.up.railway.app/health` → should return `{"ok":true}`
2. Visit the app root → should load
3. Trigger a magic link login to confirm Resend is working

---

## Step 9 — GitHub Actions CI/CD

### Secrets & Variables to add in GitHub

Go to your repo → **Settings** → **Secrets and variables** → **Actions**:

**Secrets** (encrypted):
| Name             | Value                                      |
|------------------|--------------------------------------------|
| `RAILWAY_TOKEN`  | From Railway → Account Settings → Tokens  |

**Variables** (plain text):
| Name                   | Value                                |
|------------------------|--------------------------------------|
| `RAILWAY_SERVICE_NAME` | Your Railway service name (e.g. `web`) |

### What the workflow does

- **On every PR to main:** runs typecheck (`npm run check`) + tests (`npm run test`) against a fresh Postgres instance
- **On merge to main:** runs CI then deploys to Railway automatically

### Getting your Railway token

```
railway.app → Account (top-right avatar) → Account Settings → Tokens → Create Token
```

---

## Ongoing deploy flow

```
Push to feature branch
  → open PR
    → CI runs (typecheck + tests)
      → merge to main
        → CI runs again + auto-deploys to Railway
```

No manual deploys needed after this is set up.

---

## Troubleshooting

**Build fails on `npm run migrate`**
- Check that all `POSTGRES_*` env vars are set correctly in Railway
- Make sure the Postgres service is in the same Railway project/environment

**`node build` crashes on start**
- Check Railway logs for the error
- Confirm `@sveltejs/adapter-node` is installed and `svelte.config.ts` uses it
- Make sure `BETTER_AUTH_URL` matches your actual public URL exactly

**Magic links not arriving**
- Verify `RESEND_API_KEY` is correct
- Check Resend dashboard for send logs / bounces
- Confirm the sender domain is verified in Resend (or use `onboarding@resend.dev` for testing)

**Health check failing**
- Confirm `src/routes/health/+server.ts` is in your repo
- Check that the route isn't behind an auth guard
