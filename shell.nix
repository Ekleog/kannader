with import ./common.nix;

pkgs.stdenv.mkDerivation {
  name = "yuubind";
  buildInputs = (
    (with pkgs; [
      cacert
      cargo-fuzz
      gnuplot
      mdbook
      nodejs
      rust-analyzer
    ]) ++
    (with rustNightlyChannel; [
      cargo
      rust
      rustfmt-preview
    ])
  );
}
