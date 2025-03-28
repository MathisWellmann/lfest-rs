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
      in
        with pkgs; {
          devShells.default = mkShell {
            buildInputs = [
              (lib.hiPrio rust-bin.nightly."2025-02-01".rustfmt)
              rust
              cargo-semver-checks
              cargo-mutants
              cargo-outdated
              cargo-tarpaulin # Code coverage
              taplo
            ];
          };
        }
    );
}
