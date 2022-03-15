rec {
  pkgsSrc = builtins.fetchTarball {
    # The following is for nixos-unstable on 2022-03-15
    url = "https://github.com/NixOS/nixpkgs/archive/73ad5f9e147c0d2a2061f1d4bd91e05078dc0b58.tar.gz";
    sha256 = "01j7nhxbb2kjw38yk4hkjkkbmz50g3br7fgvad6b1cjpdvfsllds";
  };
  naerskSrc = builtins.fetchTarball {
    # The following is the latest version as of 2022-03-15
    url = "https://github.com/nmattia/naersk/archive/2fc8ce9d3c025d59fee349c1f80be9785049d653.tar.gz";
    sha256 = "0qjyfmw5v7s6ynjns4a61vlyj9cghj7vbpgrp9147ngb1f8krz3c";
  };
  rustOverlaySrc = builtins.fetchTarball {
    # The following is the latest version as of 2022-03-15
    url = "https://github.com/mozilla/nixpkgs-mozilla/archive/15b7a05f20aab51c4ffbefddb1b448e862dccb7d.tar.gz";
    sha256 = "0admybxrjan9a04wq54c3zykpw81sc1z1nqclm74a7pgjdp7iqv1";
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
    date = "2022-03-15";
    channel = "nightly";
    sha256 = "0wgn87di2bz901iv2gspg935qgyzc3c2fg5jszckxl4q47jzvd8b";
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
