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
          ];

          shellHook = ''
            echo "Meals on the Move dev shell"
            echo "Rust $(rustc --version | cut -d' ' -f2) 🦀"
            echo "Node $(node --version) 🟢"
          '';
        };
      });
}
