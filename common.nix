rec {
  hostPkgs = import <nixpkgs> {};
  pkgsSrc = hostPkgs.fetchFromGitHub {
    owner = "NixOS";
    repo = "nixpkgs";
    # The following is for nixos-unstable on 2020-12-13
    rev = "e9158eca70ae59e73fae23be5d13d3fa0cfc78b4";
    sha256 = "0cnmvnvin9ixzl98fmlm3g17l6w95gifqfb3rfxs55c0wj2ddy53";
  };
  rustOverlaySrc = hostPkgs.fetchFromGitHub {
    owner = "mozilla";
    repo = "nixpkgs-mozilla";
    # The following is the latest version as of 2020-12-13
    rev = "8c007b60731c07dd7a052cce508de3bb1ae849b4";
    sha256 = "1zybp62zz0h077zm2zmqs2wcg3whg6jqaah9hcl1gv4x8af4zhs6";
  };
  rustOverlay = import rustOverlaySrc;
  pkgs = import pkgsSrc {
    overlays = [ rustOverlay ];
  };
  rustNightlyChannel = pkgs.rustChannelOf {
    date = "2020-12-13";
    channel = "nightly";
    sha256 = "1xd1560g3nbc6s535i9pzry3livkkccrjz970qrn5rb6y4xqkzfj";
  };
  #rustBetaChannel = pkgs.rustChannelOf {
  #  date = "2018-04-20";
  #  channel = "beta";
  #};
  #rustStableChannel = pkgs.rustChannelOf {
  #  date = "2020-03-12";
  #  channel = "stable";
  #  sha256 = "0pddwpkpwnihw37r8s92wamls8v0mgya67g9m8h6p5zwgh4il1z6";
  #};
}
