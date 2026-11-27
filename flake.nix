{
  description = "tql, a database client for the terminal";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs =
    { self, nixpkgs }:
    let
      # The release this flake installs. Bump it, and the hashes below with it,
      # when a new version is out: the checksums are in the release's
      # SHA256SUMS.
      version = "0.17.1";

      # The release assets, one per platform. The Linux ones are static musl
      # builds, so they run on NixOS with nothing beside them, and the macOS
      # one is for Apple Silicon. There is no build for Intel Macs, so that
      # platform builds from source instead.
      releaseAssets = {
        "x86_64-linux" = {
          asset = "tql-linux-x86_64";
          hash = "sha256-1nmJ5CZ1r1ZzwEBTYj3aP9acOFS6RhDlOkIuUvIDh4M=";
        };
        "aarch64-linux" = {
          asset = "tql-linux-aarch64";
          hash = "sha256-Z4p/Z2hP47J4DKzq1XFXbPMLXR8fVkTZEC82VAIsvPs=";
        };
        "aarch64-darwin" = {
          asset = "tql-macos-aarch64";
          hash = "sha256-vhN5zeiw2JJngX0e+CH4jaX6lLLCnRkJixTC4jpaITo=";
        };
      };

      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});

      # The release binary, for the platforms upstream builds one for.
      prebuilt =
        pkgs:
        {
          asset,
          hash,
        }:
        pkgs.stdenv.mkDerivation {
          pname = "tql";
          inherit version;

          src = pkgs.fetchurl {
            url = "https://github.com/VheissuLabs/tql/releases/download/v${version}/${asset}";
            inherit hash;
            name = "tql-${version}-${asset}";
          };

          # The asset is the binary itself, so there is nothing to unpack, and
          # no source to configure.
          dontUnpack = true;

          installPhase = ''
            install -Dm755 $src $out/bin/tql
          '';

          meta = {
            description = "A database client for the terminal";
            homepage = "https://github.com/VheissuLabs/tql";
            license = pkgs.lib.licenses.mit;
            mainProgram = "tql";
            platforms = [
              "x86_64-linux"
              "aarch64-linux"
              "aarch64-darwin"
            ];
          };
        };

      # Built from the sources in this repository, for the platforms upstream
      # releases no binary for, and for anyone who wants to patch it.
      fromSource =
        pkgs:
        pkgs.rustPlatform.buildRustPackage {
          pname = "tql";
          inherit version;
          src = ./.;

          cargoLock.lockFile = ./Cargo.lock;

          # DuckDB is C++ and is built from its amalgamation, which wants a
          # C++ compiler, CMake and Python.
          nativeBuildInputs = [
            pkgs.cmake
            pkgs.python3
          ];

          # The crates carry their own copies of the system libraries, which
          # is what the release build does too.
          dontCargoUseSystemLibs = true;

          # The test suite talks to databases that are not here.
          doCheck = false;

          meta = {
            description = "A database client for the terminal";
            homepage = "https://github.com/VheissuLabs/tql";
            license = pkgs.lib.licenses.mit;
            mainProgram = "tql";
          };
        };

      # The package, on whatever nixpkgs it is asked for.
      tqlFor =
        pkgs:
        let
          system = pkgs.stdenv.hostPlatform.system;
        in
        if releaseAssets ? ${system} then prebuilt pkgs releaseAssets.${system} else fromSource pkgs;
    in
    {
      packages = forAllSystems (
        pkgs:
        let
          tql = tqlFor pkgs;
        in
        {
          inherit tql;
          default = tql;

          # The same program, built here rather than fetched from a release.
          tql-source = fromSource pkgs;
        }
      );

      apps = forAllSystems (pkgs: {
        default = {
          type = "app";
          program = "${tqlFor pkgs}/bin/tql";
          meta.description = "tql, a database client for the terminal";
        };
      });

      # The binary says which version it is, and it says it the way the release
      # workflow checks it.
      checks = forAllSystems (pkgs: {
        version = pkgs.runCommand "tql-version-check" { nativeBuildInputs = [ (tqlFor pkgs) ]; } ''
          tql --version | grep -qx 'Tql v${version}'
          touch "$out"
        '';
      });

      # tql in a nixpkgs, as `pkgs.tql`.
      overlays.default = final: _prev: { tql = tqlFor final; };

      # tql in a NixOS configuration, as `tql.enable`.
      nixosModules.default =
        {
          config,
          lib,
          pkgs,
          ...
        }:
        {
          options.tql = {
            enable = lib.mkEnableOption "tql, a database client for the terminal";

            package = lib.mkOption {
              type = lib.types.package;
              default = tqlFor pkgs;
              defaultText = lib.literalExpression "the tql package built with this configuration's nixpkgs";
              description = "The tql package to install.";
            };
          };

          config = lib.mkIf config.tql.enable {
            environment.systemPackages = [ config.tql.package ];
          };
        };

      # A shell with the tools for working on tql itself.
      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = [
            pkgs.cargo
            pkgs.clippy
            pkgs.rust-analyzer
            pkgs.rustc
            pkgs.rustfmt
          ];

          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
        };
      });

      # The formatter for this file, for `nix fmt`.
      formatter = forAllSystems (pkgs: pkgs.nixfmt);
    };
}
