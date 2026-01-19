{
  description = "Flake for lfest-rs";

  inputs = {
    nixpks.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
    hongdown.url = "github:dahlia/hongdown";
    naersk = {
      url = "github:nix-community/naersk";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    nixpkgs,
    rust-overlay,
    flake-utils,
    hongdown,
    naersk,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        overlays = [(import rust-overlay)];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        rust = pkgs.rust-bin.selectLatestNightlyWith (toolchain:
          toolchain.default.override {
            extensions = [
              "rust-src"
              "rust-analyzer"
              "miri"
            ];
            targets = ["x86_64-unknown-linux-gnu"];
          });
        naersk' = let
          toolchain = pkgs.rust-bin.nightly."2025-11-13".default.override {
            extensions = [
              "rust-src"
              "rustc-dev"
              "llvm-tools"
            ];
            targets = ["x86_64-unknown-linux-gnu"];
          };
        in
          pkgs.callPackage naersk {
            cargo = toolchain;
            rustc = toolchain;
          };
        cargo-upgrades = pkgs.callPackage ./nix/cargo-upgrades.nix {};
        creusot = import ./nix/creusot.nix {inherit pkgs naersk';};
      in
        with pkgs; {
          devShells.default = mkShell {
            buildInputs = [
              (lib.hiPrio rust-bin.nightly."2025-02-01".rustfmt)
              rust
              cargo-nextest
              cargo-semver-checks
              cargo-mutants
              cargo-upgrades
              cargo-tarpaulin # Code coverage
              cargo-audit
              cargo-machete
              taplo
              mprocs # Run multiple commands in parallel from `mprocs.yml`, acting essentially as a local CI system.
              hongdown.packages.${system}.hongdown
              creusot # Execute with `cargo creusot`
            ];
            RUST_BACKTRACE = "1";
          };
        }
    );
}
