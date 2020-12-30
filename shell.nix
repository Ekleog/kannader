with import ./common.nix;

pkgs.stdenv.mkDerivation {
  name = "yuubind";
  buildInputs = (
    (with pkgs; [
      cacert
      capnproto
      cargo-fuzz
      gnuplot
      mdbook
      nodejs
      rust-analyzer
    ]) ++
    (with rustNightlyChannel; [
      cargo
      (rust.override { targets = ["wasm32-unknown-unknown"]; })
    ])
  );
}
