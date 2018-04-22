with import ../common.nix;
let
  # TODO: upstream to nixpkgs
  cargo-fuzz = pkgs.rustPlatform.buildRustPackage rec {
    name = "cargo-fuzz-${version}";
    version = "0.5.3";

    src =
      let
        source = pkgs.fetchFromGitHub {
          owner = "rust-fuzz";
          repo = "cargo-fuzz";
          rev = version;
          sha256 = "1l452fnjw7i10nrd4y4rssi5d457vgjp6rhdr9cnq32bjhdkprrs";
        };
        patch = pkgs.fetchurl {
          url = "https://gist.githubusercontent.com/Ekleog/7d5b62d13b7207aafa4c37d1bbdf2de7/raw/c6027fc1c531947f4d6836a3c4cba1b3e24df24c/Cargo.lock";
          sha256 = "0d7b6kxfbfvwksybzrihylamg2zv5fmsk9m6xshryhwipskzzvmd";
        };
      in
      pkgs.runCommand "cargo-fuzz-src" {} ''
        cp -R ${source} $out
        chmod +w $out
        cp ${patch} $out/Cargo.lock
      '';

    cargoSha256 = "0ajm8qp8hi7kn7199ywv26cmjv13phxv72lz8kcq97hxg17x0dkk";
  };
in
pkgs.stdenv.mkDerivation {
  name = "yuubind-fuzz";
  buildInputs = with rustNightlyChannel; [ rustfmt-preview rust cargo-fuzz ];
}
