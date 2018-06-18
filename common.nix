rec {
  hostPkgs = import <nixpkgs> {};
  pkgsSrc = hostPkgs.fetchFromGitHub {
    owner = "NixOS";
    repo = "nixpkgs";
    # The following is for nixos-unstable on 2018-06-17
    rev = "4b649a99d8461c980e7028a693387dc48033c1f7";
    sha256 = "0iy2gllj457052wkp20baigb2bnal9nhyai0z9hvjr3x25ngck4y";
  };
  rustOverlaySrc = hostPkgs.fetchFromGitHub {
    owner = "mozilla";
    repo = "nixpkgs-mozilla";
    # The following is the latest version as of 2018-06-17
    rev = "11cf06f0550a022d8bc4850768edecc3beef9f40";
    sha256 = "00fwvvs8qa2g17q4bpwskp3bmis5vac4jp1wsgzcyn64arkxnmys";
  };
  rustOverlay = import rustOverlaySrc;
  pkgs = import pkgsSrc {
    overlays = [ rustOverlay ];
  };
  rustNightlyChannel = pkgs.rustChannelOf {
    date = "2018-06-17";
    channel = "nightly";
  };
  #rustBetaChannel = pkgs.rustChannelOf {
  #  date = "2018-04-20";
  #  channel = "beta";
  #};
  rustStableChannel = pkgs.rustChannelOf {
    date = "2018-06-05";
    channel = "stable";
  };
}
