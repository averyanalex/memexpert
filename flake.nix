{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    systems.url = "github:nix-systems/default";
    devenv.url = "github:cachix/devenv";
    devenv.inputs.nixpkgs.follows = "nixpkgs";
    fenix.url = "github:nix-community/fenix";
    fenix.inputs.nixpkgs.follows = "nixpkgs";
    crane.url = "github:ipetkov/crane";
    crane.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = {
    self,
    nixpkgs,
    devenv,
    systems,
    fenix,
    crane,
    ...
  } @ inputs: let
    forEachSystem = nixpkgs.lib.genAttrs (import systems);
  in {
    packages = forEachSystem (system: {
      devenv-up = self.devShells.${system}.default.config.procfileScript;
      default = let
        overlays = [fenix.overlays.default];
        pkgs = import nixpkgs {inherit system overlays;};
        craneLib = (crane.mkLib pkgs).overrideToolchain (p: pkgs.fenix.minimal.toolchain);
      in
        craneLib.buildPackage {
          src = ./.;
          strictDeps = true;

          nativeBuildInputs = with pkgs; [autoPatchelfHook pkg-config];
          buildInputs = with pkgs; [openssl stdenv.cc.cc.lib];
        };
    });

    devShells =
      forEachSystem
      (system: let
        pkgs = nixpkgs.legacyPackages.${system};
      in {
        default = devenv.lib.mkShell {
          inherit inputs pkgs;
          modules = [
            {
              packages = with pkgs; [sea-orm-cli openssl alejandra];

              languages.rust = {
                enable = true;
                channel = "nightly";
              };

              languages.javascript.enable = true;
              languages.javascript.pnpm.enable = true;

              services.postgres = {
                enable = true;
                package = pkgs.postgresql_16;
                initialDatabases = [{name = "memexpert";}];
              };
              env.PGDATABASE = "memexpert";
              enterShell = "export DATABASE_URL=postgresql:///$PGDATABASE?host=$PGHOST";

              processes.qdrant.exec = "podman run --network host -e QDRANT__SERVICE__GRPC_PORT=\"6334\" qdrant/qdrant";

              services.meilisearch = {
                enable = true;
              };
            }
          ];
        };
      });
  };
}
