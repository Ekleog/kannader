with import ./common.nix;

pkgs.stdenv.mkDerivation {
  name = "yuubind";
  buildInputs = (
    (with rustNightlyChannel; [ rust rustfmt-preview ])
    ++ (with pkgs; [ cargo-fuzz mdbook cacert ])
  );
}
