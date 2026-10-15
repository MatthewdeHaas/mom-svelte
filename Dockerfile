# Two-stage build for the SvelteKit app. Built in CI only — the
# resulting image is pushed to GHCR and Railway pulls it from there.
# See `.github/workflows/ci.yaml::build-svelte-image`.
#
# Stage 1 compiles the Svelte app to vanilla JS under `build/`.
# Stage 2 ships a minimal Alpine + Node + pnpm with only prod deps.
#
# BETTER_AUTH_SECRET / BETTER_AUTH_URL are build-time placeholders so
# Better Auth doesn't throw during SvelteKit's post-build SSR analysis.
# The real values are injected by Railway at runtime and never enter
# the pushed image.

# ---------- Builder ----------
FROM node:22-alpine AS builder
WORKDIR /app
RUN npm install -g pnpm@10
COPY package.json pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile --ignore-scripts
COPY . .
ARG BETTER_AUTH_SECRET=build-time-placeholder
ARG BETTER_AUTH_URL=http://localhost:3000
ENV BETTER_AUTH_SECRET=$BETTER_AUTH_SECRET
ENV BETTER_AUTH_URL=$BETTER_AUTH_URL
RUN pnpm run build

# ---------- Runtime ----------
FROM node:22-alpine AS runtime
WORKDIR /app
RUN npm install -g pnpm@10
COPY --from=builder /app/build build/
COPY --from=builder /app/package.json ./
COPY --from=builder /app/pnpm-lock.yaml ./
COPY --from=builder /app/scripts scripts/
COPY --from=builder /app/tsconfig.json ./
RUN pnpm install --frozen-lockfile --prod --ignore-scripts

EXPOSE 3000
# Post-Track-3-Step-6, Rust owns the data layer and runs its own
# `sqlx::migrate!` on startup against the shared Postgres — this
# container no longer runs the TS migrator in prod.
#
# `seed:admin` stays behind SEED_ADMIN=1 so a fresh environment can
# opt in explicitly; the default no-ops to avoid a surprising write
# on every rollout.
CMD ["sh", "-c", "if [ \"${SEED_ADMIN:-0}\" = \"1\" ]; then pnpm run seed:admin; fi && node build"]
