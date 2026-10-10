{
  description = "Meals on the Move - SvelteKit + Rust dev shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "clippy" "rustfmt" "rust-analyzer" ];
        };
      in
      {
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
            # Deploy + secrets CLIs for Track 4a. `railway` targets
            # the two-service Railway deploy; `infisical` is the
            # single writer the GHA workflow authenticates against,
            # and devs can `infisical run -- pnpm dev` locally.
            pkgs.railway
            pkgs.infisical
          ];

          shellHook = ''
            echo "Meals on the Move dev shell"
            echo "Rust $(rustc --version | cut -d' ' -f2) 🦀"
            echo "Node $(node --version) 🟢"
            echo "sqlx-cli $(sqlx --version 2>/dev/null | cut -d' ' -f2 || echo missing)"
            echo "railway  $(railway --version 2>/dev/null | awk '{print $NF}' || echo missing) 🚂"
            echo "infisical $(infisical --version 2>/dev/null | head -1 || echo missing) 🔐"
          '';
        };
      });
}
