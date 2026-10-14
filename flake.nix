{
  description = "Meals on the Move - SvelteKit + Rust dev shell + reproducible API build";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
    # crane: Rust-aware Nix builder. Splits dep-build from app-build so
    # CI gets cache granularity even when a single crate changes.
    crane.url = "github:ipetkov/crane";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, crane, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "clippy" "rustfmt" "rust-analyzer" ];
        };
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        # Build root: the Rust workspace staged ALONGSIDE the repo's
        # `static/` directory, because `crates/pdf/src/packing_slip.rs`
        # has `include_str!("../../../../static/mom-logo.svg")` — that's
        # 4 levels up from `crates/pdf/src/` back to the repo root, then
        # into `static/`. If we flattened `admin-operations-rust/` to
        # `$out/`, the 4-ups escapes the sandbox and compile fails. So
        # we rebuild the repo-layout: `admin-operations-rust/` is a
        # subdirectory under `$out/`, and `static/` is its sibling.
        # Also stage migrations under `admin-operations-rust/migrations/`
        # because `sqlx::migrate!("./../../migrations")` embeds SQL at
        # compile time by reading from a path relative to the crate's
        # `CARGO_MANIFEST_DIR`. The workspace keeps that directory
        # gitignored and populates it from `scripts/migrations/` via
        # the sync script. The Nix sandbox has no network and no state
        # between runs, so we have to stage it ourselves. Everything
        # else — the committed `.sqlx/` cache (103 queries), the
        # `.cargo/config.toml` with `SQLX_OFFLINE=true`, the lockfile
        # — comes along as part of `admin-operations-rust/`.
        buildRoot = pkgs.runCommand "api-build-root" { } ''
          mkdir -p $out/admin-operations-rust $out/static
          cp -r ${./admin-operations-rust}/. $out/admin-operations-rust/
          cp -r ${./static}/. $out/static/
          chmod -R u+w $out
          # The sync script copies to admin-operations-rust/migrations/
          # at the repo root; we reproduce that layout here.
          rm -rf $out/admin-operations-rust/migrations
          cp -r ${./scripts/migrations} $out/admin-operations-rust/migrations
        '';

        # Crane needs a cleaned source for its cargo-vendor pass; the
        # workspace already excludes target/ via .gitignore but we're
        # feeding it a nix store path, not a git tree. `cleanCargoSource`
        # keeps only Rust-relevant files (Cargo.{toml,lock}, *.rs, build
        # scripts) plus the explicit paths we add below.
        src = pkgs.lib.cleanSourceWith {
          src = buildRoot;
          filter = path: type:
            # Include SQL migrations + sqlx offline cache + .cargo config
            # + the static/ directory the pdf crate `include_str!`s —
            # crane's default filter drops everything that isn't a Rust
            # source file, which would strip exactly the state the
            # build depends on.
            (builtins.match ".*\\.sql$" path != null)
            || (builtins.match ".*/\\.sqlx/.*" path != null)
            || (builtins.match ".*/\\.cargo/.*" path != null)
            || (builtins.match ".*/static(/.*)?$" path != null)
            || (craneLib.filterCargoSources path type);
        };

        commonArgs = {
          inherit src;
          # The workspace Cargo.toml lives in admin-operations-rust/ ,
          # not at the root of `src`. `sourceRoot` tells the stdenv
          # unpacker where to land after unpacking the source tarball;
          # `cargoToml` + `cargoLock` give crane the explicit paths it
          # would otherwise look for at the top of src. The relative
          # `include_str!` paths in the pdf crate resolve correctly
          # because `static/` lives one level above `admin-operations-rust/`
          # in both the source tree and the build sandbox.
          sourceRoot = "source/admin-operations-rust";
          cargoToml = "${buildRoot}/admin-operations-rust/Cargo.toml";
          cargoLock = "${buildRoot}/admin-operations-rust/Cargo.lock";
          # pname/version can't be inferred from a workspace root
          # Cargo.toml (it has no [package]); pin them explicitly so
          # crane doesn't try.
          pname = "mom-rust-workspace";
          version = "0.1.0";
          strictDeps = true;
          # Enforced: no network inside the Nix sandbox, so query!
          # macros MUST resolve against admin-operations-rust/.sqlx/.
          SQLX_OFFLINE = "true";
          # pkg-config + openssl are compile-time only for native-dep
          # probes (e.g. zstd-sys's build.rs looks for libclang/pkgconfig
          # transitively). Runtime stays pure-rust: sqlx uses
          # `tls-rustls`, reqwest uses `rustls-tls`.
          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = [ pkgs.openssl ];
        };

        # Dep-build layer: produces the vendored target/ with every
        # dependency compiled. Reused across every app build until
        # Cargo.lock changes — this is where magic-nix-cache earns
        # its keep in CI.
        cargoArtifacts = craneLib.buildDepsOnly (commonArgs // {
          pname = "mom-rust-deps";
          # Build the whole workspace's deps so later per-crate builds
          # all hit the cache. `--locked` ensures Cargo.lock matches
          # what's committed; it fails loud on drift.
          cargoExtraArgs = "--workspace --locked";
        });

        # App-build layer: just the api crate on top of the dep cache.
        # `cargoExtraArgs = "-p api"` keeps the output small; the full
        # workspace's test binaries don't ship.
        api = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          pname = "api";
          cargoExtraArgs = "-p api --locked";
          # No tests under nix build — they'd need a live Postgres,
          # which is a CI-step concern, not a build-step concern.
          doCheck = false;
        });
      in
      {
        packages = {
          api = api;
          default = api;
        };

        apps.api = {
          type = "app";
          program = "${api}/bin/api";
        };

        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain
            pkgs.nodejs_22
            pkgs.pkg-config
            pkgs.openssl
            # sqlx-cli for offline-mode cache regeneration
            # (`pnpm rust:prepare`). The db crate uses `query_as!`
            # macros that type-check against `.sqlx/` in CI with
            # SQLX_OFFLINE=1; this is how we populate / refresh it.
            pkgs.sqlx-cli
            # Deploy CLI for Track 4a. `railway up` from GHA targets
            # the two-service Railway deploy; secrets live directly in
            # Railway's per-service dashboard (no vault — see
            # docs/ops.md § Secrets).
            pkgs.railway
          ];

          shellHook = ''
            echo "Meals on the Move dev shell"
            echo "Rust $(rustc --version | cut -d' ' -f2) 🦀"
            echo "Node $(node --version) 🟢"
            echo "sqlx-cli  $(sqlx --version 2>/dev/null | cut -d' ' -f2 || echo missing) 🗄️"
            echo "railway   $(railway --version 2>/dev/null | awk '{print $NF}' || echo missing) 🚂"
          '';
        };

        # `nix flake check` runs this — gives us a one-command sanity gate
        # that the api derivation at least evaluates. The actual build is
        # too heavy for `check`, so we just reference the derivation.
        checks = {
          api-builds = api;
        };
      });
}
