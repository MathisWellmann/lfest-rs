{
  description = "Flake for lfest-rs";

  inputs = {
    nixpks.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
    hongdown.url = "github:dahlia/hongdown";
  };

  outputs = {
    nixpkgs,
    rust-overlay,
    flake-utils,
    hongdown,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        overlays = [(import rust-overlay)];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        rust = pkgs.rust-bin.selectLatestNightlyWith(toolchain: toolchain.default.override{
          extensions = [
            "rust-src"
            "rust-analyzer"
            "miri"
          ];
          targets = ["x86_64-unknown-linux-gnu"];
        });
        cargo_upgrades = pkgs.rustPlatform.buildRustPackage {
          name = "cargo-upgrades";
          src = builtins.fetchGit {
            url = "https://gitlab.com/kornelski/cargo-upgrades";
            rev = "95e1d282dd165c69f0eb4dc66a09db5265734f54";
          };
          useFetchCargoVendor = true;
          cargoHash = "sha256-yEUfWe4/kSvBPx3xneff45+K3Gix2QXDjUesm+psUxI=";
          doCheck = false; # Tests fail at the current revision.
        };
        creusot = pkgs.stdenv.mkDerivation{
          name = "creusot";
          src = builtins.fetchGit {
            url = "https://github.com/creusot-rs/creusot";
            rev = "879cd335441a1a126380c007d38cb499faa316dc";
          };
          buildInputs = with pkgs; [
            rust
            pkg-config
            openssl
            opam
            gcc
            autoconf
            gtk3
            gtksourceview
            cairo
            zeromq
            rsync
            git
          ];
          buildPhase = ''
            mkdir -p $out/bin
            ls -la .
            # opam init
            cargo run --bin creusot-install
          '';
          # installPhase = ''
          #   # Make the script executable
          #   chmod +x $out/hello.py
          # '';
        };
      in
        with pkgs; {
          devShells.default = mkShell {
            buildInputs = [
              (lib.hiPrio rust-bin.nightly."2025-02-01".rustfmt)
              rust
              cargo-nextest
              cargo-semver-checks
              cargo-mutants
              cargo_upgrades
              cargo-tarpaulin # Code coverage
              cargo-audit
              cargo-machete
              taplo
              mprocs # Run multiple commands in parallel from `mprocs.yml`, acting essentially as a local CI system.
              hongdown.packages.${system}.hongdown
              # creusot
            ];
            RUST_BACKTRACE = "1";
          };
        }
    );
}
