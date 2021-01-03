with import ./common.nix;

pkgs.stdenv.mkDerivation {
  name = "kannader";
  buildInputs = (
    (with pkgs; [
      cacert
      capnproto
      cargo-fuzz
      cargo-udeps
      gnuplot
      mdbook
      nodejs
      rust-analyzer
    ]) ++
    (with rustNightlyChannel; [
      cargo
      rust
    ])
  );
}
