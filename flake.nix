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
        # kani-tarball = builtins.fetchTarball{
        #   url = "https://github.com/model-checking/kani/releases/download/kani-0.62.0/kani-0.62.0-x86_64-unknown-linux-gnu.tar.gz";
        #   sha256 = "sha256:01rlj2kwjg81ak15pvbgcx6rfzmkabdk7xgqjq01a6l1xi1kf2vl";
        # };
        # kani-home = pkgs.stdenv.mkDerivation {
        #   name = "kani-home";
        #   src = kani-tarball;
        #   buildInputs = [
        #     pkgs.stdenv.cc.cc.lib #libs needed by patchelf
        #   ];
        #   runtimeDependencies = [
        #     pkgs.glibc #not detected as missing by patchelf for some reason
        #   ];
        #   nativeBuildInputs = [ pkgs.autoPatchelfHook ];
        #   installPhase = ''
        #   runHook preInstall
        #   ${pkgs.rsync}/bin/rsync -av $src/ $out --exclude kani-compiler
        #   runHook postInstall
        #   '';
        # };
        # kani = pkgs.rustPlatform.buildRustPackage rec {
        #   pname = "kani";
        #   version = "kani-0.62.0";
        #   src = pkgs.fetchFromGitHub {
        #     owner = "model-checking";
        #     repo = "kani";
        #     rev = "kani-${version}";
        #     hash = "";
        #   };
        #   nativeBuildInputs = [ pkgs.makeWrapper ];
        #   postInstall = ''
        #   mkdir -p $out/lib/
        #   ${pkgs.rsync}/bin/rsync -av ${kani-home}/ $out/lib/${version} --perms --chmod=D+rw,F+rw
        #   cp $out/bin/* $out/lib/${version}/bin/
        #   ln -s ${rust} $out/lib/${version}/toolchain
        #   '';

        #   postFixup = ''
        #   wrapProgram $out/bin/kani --set KANI_HOME $out/lib/
        #   wrapProgram $out/bin/cargo-kani --set KANI_HOME $out/lib/
        #   '';

        #   cargoHash = "sha256-WbMkcWvAnoRCB5CYhf3KR4obMB2GW42Rc3unRstafGU=";

        #   env = {
        #     RUSTUP_HOME = "${rust}";
        #     RUSTUP_TOOLCHAIN = "..";
        #   };
        # };

        # kani = pkgs.rustPlatform.buildRustPackage rec {
        #   pname = "kani";
        #   version = "0.61.0";

        #   src = pkgs.fetchFromGitHub {
        #     owner = "model-checking";
        #     repo = "kani";
        #     rev = "kani-${version}";
        #     hash = "sha256-NfOXyWGqvjgUFGjZhox8J0Bg+M0rUENjRLfLC1Kon/A=";
        #   };

        #   cargoHash = "sha256-yhWMj9y85YiowLwaZJKfh6jhkq9c0ClbsJHVSrPofg4=";

        #   # env = lib.traceValSeq {
        #   #   # kani-compiler expects Nightly compiler installed through rustup.
        #   #   RUSTUP_HOME = "${rust}";
        #   #   RUSTUP_TOOLCHAIN = "..";
        #   #   # Allow nightly features on stable compiler.
        #   #   RUSTC_BOOTSTRAP = 1;

        #   #   # RUST_SRC_PATH = "${rust-src}/lib/rustlib/src/rust/library";
        #   #   # RUST_SRC_PATH = "${rustc-dev}/lib/rustlib/rustc-src/rust/compiler";
        #   # };
        # };
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
              # kani
            ];
            RUST_BACKTRACE = "1";
          };
        }
    );
}
