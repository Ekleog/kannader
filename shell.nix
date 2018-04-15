with import <nixpkgs> {};
stdenv.mkDerivation {
  name = "smtp-server";
  buildInputs = [ cargo rustfmt ];
}
