with import <nixpkgs> {};
stdenv.mkDerivation {
  name = "yuubind";
  buildInputs = [ cargo rustfmt ];
}
