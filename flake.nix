{
  description = "Flake for lfest-rs";

  inputs = {
    nixpks.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    nixpkgs,
    rust-overlay,
    flake-utils,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        overlays = [(import rust-overlay)];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        rust = (
          pkgs.rust-bin.stable.latest.default.override {
            extensions = [
              "rust-src"
              "rust-analyzer"
            ];
            targets = ["x86_64-unknown-linux-gnu"];
          }
        );
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
              taplo
            ];
            RUST_BACKTRACE = "1";
          };
        }
    );
}
