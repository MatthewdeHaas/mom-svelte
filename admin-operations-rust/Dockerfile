# Multi-stage build for the Rust API service.
#
# Stage 1 builds a release binary against the full workspace. Stage 2
# ships a thin Debian base with just the binary and the shared libs it
# needs (openssl is replaced by rustls, so we only need libgcc/libssl's
# tls transitive — which Debian slim already has).
#
# Migrations are COPIED (not symlinked) into the crate's `migrations/`
# directory *before* `cargo build` so `sqlx::migrate!` embeds them at
# compile time. A symlink pointing out of the build context would make
# the macro see an empty directory and emit a no-op migrator.

# ---------- Builder ----------
FROM rust:1.80-slim-bookworm AS builder

WORKDIR /app

# Native deps the crates need to build:
# - pkg-config for calamine/printpdf transitive feature probes
# - libssl-dev is unused at runtime (we pick rustls) but a few build
#   scripts still probe for it; keeping it avoids future surprises
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        pkg-config \
        libssl-dev \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests first so changing a .rs file doesn't bust the
# dependency-build layer.
COPY admin-operations-rust/Cargo.toml admin-operations-rust/rust-toolchain.toml ./
COPY admin-operations-rust/crates ./crates

# Real copy of migrations into the crate so `sqlx::migrate!` finds them.
# `scripts/migrations/` is the source of truth; `.gitignore` excludes
# the destination so the two never two-way-diff.
COPY scripts/migrations ./migrations

# Build the release binary. `--locked` would be ideal but we don't
# commit Cargo.lock during scaffold; add once the lockfile stabilises.
RUN cargo build --release --bin api

# ---------- Runtime ----------
FROM debian:bookworm-slim AS runtime

# Non-root user — Railway doesn't require it, but a stray `rm -rf /`
# in a bug is cheaper to shrug off as uid 1000.
RUN groupadd --system app && useradd --system --gid app --uid 1000 app

# ca-certificates for outbound HTTPS (if we ever call Resend / S3);
# libgcc / libstdc++ come with the base image.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/api /usr/local/bin/api

USER app

EXPOSE 8080
# Railway sets $PORT; we default to 8080 for local docker-compose.
ENV RUST_LOG=info,sqlx::query=warn
CMD ["/usr/local/bin/api"]
