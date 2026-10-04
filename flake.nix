{
  description = "Meals on the Move - SvelteKit dev shell";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            nodejs_22
            postgresql
          ];
          shellHook = ''
            echo "Meals on the Move dev shell"
            echo "Node $(node --version)"
            echo "psql $(psql --version)"
          '';
        };
      });
}
