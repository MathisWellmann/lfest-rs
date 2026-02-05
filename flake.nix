{
  description = "Flake for lfest-rs";

  inputs = {
    nixpks.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
    hongdown.url = "github:dahlia/hongdown";
    bencher.url = "github:MathisWellmann/bencher";
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
    bencher,
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
        buildInputs = [
          rust
        ];
        rust_tools = with pkgs; [
          cargo-nextest
          cargo-semver-checks
          cargo-mutants
          cargo-upgrades
          cargo-tarpaulin # Code coverage
          cargo-audit
          cargo-machete
          creusot # Execute with `cargo creusot`
          taplo # Format `.toml` files.
        ];
        nix_tools = with pkgs; [
          alejandra # Nix code formatter.
          deadnix # Nix dead code checker
          statix # Nix static code checker.
        ];
        tools = with pkgs; [
          mprocs # Run multiple commands in parallel from `mprocs.yml`, acting essentially as a local CI system.
          hongdown.packages.${system}.hongdown
          bencher.packages.${system}.bencher
        ];
      in
        with pkgs; {
          devShells.default = mkShell {
            buildInputs = buildInputs ++ rust_tools ++ nix_tools ++ tools;
            RUST_BACKTRACE = "1";
          };
        }
    );
}
