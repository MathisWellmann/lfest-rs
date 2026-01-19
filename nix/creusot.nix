{
  pkgs,
  naersk',
  ...
}:
naersk'.buildPackage rec {
  name = "creusot";
  version = "v0.9.0";
  src = pkgs.fetchFromGitHub {
    owner = "creusot-rs";
    repo = "creusot";
    tag = version;
    hash = "sha256-YlRRCOM0q1vXFAMFaZwU1TjV2TNSYHTnCz124mBxETA=";
  };
  cargoHash = "sha256-nBK5cFv4golbtgUx+MlBEINX8KMaJUpMBPsgy911AhU=";
  buildInputs = with pkgs; [
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
}
