{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    systems.url = "github:nix-systems/default";
    devenv.url = "github:cachix/devenv";
    devenv.inputs.nixpkgs.follows = "nixpkgs";
    fenix.url = "github:nix-community/fenix";
    fenix.inputs.nixpkgs.follows = "nixpkgs";
    import-cargo.url = "github:edolstra/import-cargo";
  };

  outputs = {
    self,
    nixpkgs,
    devenv,
    systems,
    fenix,
    import-cargo,
    ...
  } @ inputs: let
    forEachSystem = nixpkgs.lib.genAttrs (import systems);
  in {
    packages = forEachSystem (system: {
      devenv-up = self.devShells.${system}.default.config.procfileScript;
      default = let
        overlays = [fenix.overlays.default];
        pkgs = import nixpkgs {inherit system overlays;};
        inherit (import-cargo.builders) importCargo;
      in
        pkgs.stdenv.mkDerivation {
          name = "memexpert";
          src = self;

          buildInputs = with pkgs; [openssl];

          nativeBuildInputs =
            [pkgs.pkg-config pkgs.fenix.default.toolchain]
            ++ [
              (importCargo {
                lockFile = ./Cargo.lock;
                inherit pkgs;
              })
              .cargoHome
            ];

          buildPhase = ''
            cargo build --release --offline
          '';

          installPhase = ''
            install -Dm775 ./target/release/memexpert $out/bin/memexpert
          '';
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

              dotenv.enable = true;

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

              services.meilisearch = {
                enable = true;
              };
            }
          ];
        };
      });
  };
}
