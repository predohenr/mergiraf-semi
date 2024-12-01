{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs";
    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
    git-hooks = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      ...
    }@inputs:
    inputs.flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import inputs.rust-overlay) ];
        };

        craneLib = inputs.crane.mkLib nixpkgs.legacyPackages.${system};

        # When filtering sources, we want to allow assets other than .rs files
        unfilteredRoot = ./.; # The original, unfiltered source
        src = pkgs.lib.fileset.toSource {
          root = unfilteredRoot;
          fileset = pkgs.lib.fileset.unions [
            # Default files from crane (Rust and cargo files)
            (craneLib.fileset.commonCargoSources unfilteredRoot)
            ./examples
            ./helpers
          ];
        };

        nativeBuildInputs = pkgs.lib.optionals pkgs.stdenv.isDarwin [
          # Additional darwin specific inputs can be set here
          pkgs.libiconv
        ];

        # Dependencies needed for tests.
        nativeCheckInputs = with pkgs; [ git ];

        # Build just the cargo dependencies for reuse when running in CI.
        cargoArtifacts = craneLib.buildDepsOnly { inherit src nativeBuildInputs; };

        # Build the actual crate itself, reusing the dependency
        # artifacts from above.
        mergiraf = craneLib.buildPackage {
          inherit
            cargoArtifacts
            src
            nativeBuildInputs
            nativeCheckInputs
            ;
        };
      in
      {
        # `nix flake check`
        checks = {
          # Build the crate as part of `nix flake check` for convenience
          inherit mergiraf;

          # Check formatting
          mergirafFmt = craneLib.cargoFmt { inherit src; };

          # Audit dependencies
          mergirafAudit = craneLib.cargoAudit {
            inherit src;
            inherit (inputs) advisory-db;
          };

          # Run clippy (and deny all warnings) on the crate source,
          # again, reusing the dependency artifacts from above.
          #
          # Note that this is done as a separate derivation so that
          # we can block the CI if there are issues here, but not
          # prevent downstream consumers from building our crate by itself.
          mergirafClippy = craneLib.cargoClippy {
            inherit cargoArtifacts src;
            cargoClippyExtraArgs = "--all-targets -- --deny warnings";
          };

          mergirafDoc = craneLib.cargoDoc { inherit cargoArtifacts src; };

          # Run tests with cargo-nextest.
          mergirafNextest = craneLib.cargoNextest {
            inherit
              cargoArtifacts
              src
              nativeBuildInputs
              nativeCheckInputs
              ;
            partitions = 1;
            partitionType = "count";
          };

          # Git pre-commit hooks
          gitPreCommit = inputs.git-hooks.lib.${system}.run {
            src = ./.;
            hooks = {
              nixfmt-rfc-style.enable = true;
              rustfmt.enable = true;
              typos = {
                enable = true;
                settings.exclude = "{doc/src/asciinema,examples}/*";
              };
            };
          };
        };

        # `nix build`
        packages = {
          inherit mergiraf;
          default = mergiraf; # `nix build`
        };

        # `nix run`
        apps.default = flake-utils.lib.mkApp { drv = mergiraf; };

        # `nix develop`
        devShells.default = craneLib.devShell {
          inherit (self.checks.${system}.gitPreCommit) shellHook;
          buildInputs = self.checks.${system}.gitPreCommit.enabledPackages;
        };
      }
    );
}
