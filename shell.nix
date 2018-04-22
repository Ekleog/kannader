with import ./common.nix;

pkgs.stdenv.mkDerivation {
  name = "yuubind";
  buildInputs = (with rustNightlyChannel; [ rustfmt-preview ]) ++
                (with rustBetaChannel; [ rust ]);
}
