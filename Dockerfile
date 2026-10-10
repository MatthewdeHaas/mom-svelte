# Bundle the Svelte app into vanilla JavaScript for production
FROM node:22-alpine AS builder
WORKDIR /app
RUN npm install -g pnpm
COPY package.json pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile --ignore-scripts
COPY . .
# BETTER_AUTH_SECRET/URL are only needed so Better Auth doesn't throw during
# SvelteKit's post-build SSR analysis. The defaults are placeholders — the real
# values are injected by Railway at runtime and never enter the final image.
ARG BETTER_AUTH_SECRET=build-time-placeholder
ARG BETTER_AUTH_URL=http://localhost:3000
ENV BETTER_AUTH_SECRET=$BETTER_AUTH_SECRET
ENV BETTER_AUTH_URL=$BETTER_AUTH_URL
RUN pnpm run build

# Run the app
FROM node:22-alpine
WORKDIR /app
RUN npm install -g pnpm
COPY --from=builder /app/build build/
COPY --from=builder /app/package.json ./
COPY --from=builder /app/pnpm-lock.yaml ./
COPY --from=builder /app/scripts scripts/
COPY --from=builder /app/tsconfig.json ./
RUN pnpm install --frozen-lockfile --prod --ignore-scripts
EXPOSE 3000
# Post-Track-3-Step-6, Rust owns the data layer and runs its own
# `sqlx::migrate!` on startup against the shared Postgres — this
# container no longer runs the TS migrator in prod. The two-ledger
# bridge in `scripts/backfill-dev-sqlx.ts` is a dev-only tool.
#
# `seed:admin` stays behind SEED_ADMIN=1 so a fresh environment can
# opt in explicitly; the default no-ops to avoid a surprising write
# on every rollout.
CMD ["sh", "-c", "if [ \"${SEED_ADMIN:-0}\" = \"1\" ]; then pnpm run seed:admin; fi && node build"]
