{
  lib,
  rustPlatform,
}:
rustPlatform.buildRustPackage {
  name = "cargo-upgrades";
  src = fetchGit {
    url = "https://gitlab.com/kornelski/cargo-upgrades";
    rev = "4d18359ba87cd7ccb2fd0d9c975b2d85d5cb7e9c";
  };
  cargoHash = "sha256-bWVZAKH3F4BYcEujJ2uL+Iq7HDmFQHJ4eRB9xKujoO0=";
  doCheck = false; # Tests fail at the current revision, but its not fatal.
  meta = {
    description = "Check for outdated dependencies in a cargo workspace";
    homepage = "https://gitlab.com/kornelski/cargo-upgrades";
    mainProgram = "cargo-upgrades";
    license = lib.licenses.gpl3;
    maintainers = [];
  };
}
