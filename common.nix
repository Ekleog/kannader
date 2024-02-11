rec {
  pkgsSrc = builtins.fetchTarball {
    # The following is for nixos-unstable on 2024-02-09
    url = "https://github.com/NixOS/nixpkgs/archive/8a3e1cf40a6eaeb122c8321b97a0518cfa6ed779.tar.gz";
    sha256 = "000k9dvgnhd6f6599w1pdxlj7f616p82hd12i3g013684873kcrh";
  };
  naerskSrc = builtins.fetchTarball {
    # The following is the latest version as of 2024-02-09
    url = "https://github.com/nmattia/naersk/archive/aeb58d5e8faead8980a807c840232697982d47b9.tar.gz";
    sha256 = "0qjyfmw5v7s6ynjns4a61vlyj9cghj7vbpgrp9147ngb1f8krz32";
  };
  rustOverlaySrc = builtins.fetchTarball {
    # The following is the latest version as of 2024-02-09
    url = "https://github.com/mozilla/nixpkgs-mozilla/archive/9b11a87c0cc54e308fa83aac5b4ee1816d5418a2.tar.gz";
    sha256 = "1f41psqw00mdcwm28y1frjhssybg6r8i7rpa8jq0jiannksbj27s";
  };
  rustOverlay = import rustOverlaySrc;
  pkgs = import pkgsSrc {
    overlays = [
      rustOverlay
      (self: super: {
        kannader = import ./. {};
      })
    ];
  };
  rustNightlyChannelRaw = pkgs.rustChannelOf {
    date = "2024-02-04";
    channel = "nightly";
    hash = "sha256-MR0rZ9wid6oc0sGwg4/MnOkPSQ06qVtYjV6X8a+BZA8=";
    #sha256 = "0wgn87di2bz901iv2gspg935qgyzc3c2fg5jszckxl4q47jzvd8c";
  };
  rustNightlyChannel = rustNightlyChannelRaw // {
    rust = rustNightlyChannelRaw.rust.override {
      targets = ["wasm32-wasi"];
    };
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
  naersk = pkgs.callPackage naerskSrc {
    rustc = rustNightlyChannel.rust;
    cargo = rustNightlyChannel.cargo;
  };
}
