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
CMD ["sh", "-c", "pnpm run migrate && pnpm run seed:admin && node build"]
