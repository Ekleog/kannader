with import <nixpkgs> {};
stdenv.mkDerivation {
  name = "smtp-message";
  buildInputs = [ rustc rustfmt ];
}
