# Runtime-only Dockerfile for the Rust API.
#
# No builder stage. The binary is produced by `nix build .#api` in CI
# (see `.github/workflows/ci.yaml::build-rust-image`) and copied into
# the build context before `docker build` runs. This file's job is to
# wrap that single binary in a minimal Debian base with the libs it
# links against, a non-root user, and the right env defaults.
#
# Why no builder stage: toolchain drift between Railway's Docker
# builder (previously `rust:1.80-slim-bookworm`) and the flake
# (`rust-bin.stable.latest`) silently produced different binaries,
# including sqlx offline-cache mismatches. GHCR-as-artifact-of-record
# means the exact binary CI verified is the one Railway runs.
#
# Expected build context: repo root. Expected pre-copied artifact:
# `./admin-operations-rust/target/release/api` (populated by CI from
# `result/bin/api`).

FROM debian:bookworm-slim

# ca-certificates for outbound HTTPS (Resend, S3, etc.); libgcc +
# libstdc++ come with the base image. The Nix-built binary is
# dynamically linked against glibc — bookworm-slim matches the
# nixpkgs-unstable glibc ABI the flake builds against.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Non-root user — Railway doesn't require it, but a stray `rm -rf /`
# in a bug is cheaper to shrug off as uid 1000.
RUN groupadd --system app && useradd --system --gid app --uid 1000 app

WORKDIR /app
COPY admin-operations-rust/target/release/api /usr/local/bin/api

USER app

EXPOSE 8080
# Railway sets $PORT; we default to 8080 for local docker-compose.
ENV RUST_LOG=info,sqlx::query=warn
CMD ["/usr/local/bin/api"]
