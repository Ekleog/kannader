let
  hostPkgs = import <nixpkgs> {};
  pkgsSrc = hostPkgs.fetchFromGitHub {
    owner = "NixOS";
    repo = "nixpkgs";
    # The following is for nixos-unstable on 2018-04-19
    rev = "6c064e6b1f34a8416f990db0cc617a7195f71588";
    sha256 = "1rqzh475xn43phagrr30lb0fd292c1s8as53irihsnd5wcksnbyd";
  };
  rustOverlaySrc = hostPkgs.fetchFromGitHub {
    owner = "mozilla";
    repo = "nixpkgs-mozilla";
    # The following is the latest version as of 2018-04-19
    rev = "327eccf80d64ac26244aff22d0d2f4060580568a";
    sha256 = "1r14c21x7x2h3v8gmng1g8g6n0c7hr46s5p60plqfh18sf2kp845";
  };
  rustOverlay = import rustOverlaySrc;
  pkgs = import pkgsSrc {
    overlays = [ rustOverlay ];
  };
  rustChannel = pkgs.rustChannelOf {
    date = "2018-04-19";
    channel = "nightly";
  };
in

pkgs.stdenv.mkDerivation {
  name = "yuubind";
  buildInputs = (with rustChannel; [ rustfmt-preview ]) ++
                (with pkgs; [ cargo rustc ]);
}
