# Bundle the Svelte app into vanilla JavaScript for production
FROM node:25-alpine AS builder
WORKDIR /app
RUN npm install -g pnpm
COPY package.json pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile --ignore-scripts
COPY . .
ARG BETTER_AUTH_SECRET
ARG BETTER_AUTH_URL
ENV BETTER_AUTH_SECRET=$BETTER_AUTH_SECRET
ENV BETTER_AUTH_URL=$BETTER_AUTH_URL
RUN pnpm run build

# Run the app
FROM node:25-alpine
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
