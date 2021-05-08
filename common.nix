rec {
  pkgsSrc = builtins.fetchTarball {
    # The following is for nixos-unstable on 2021-04-10
    url = "https://github.com/NixOS/nixpkgs/archive/9e377a6ce42dccd9b624ae4ce8f978dc892ba0e2.tar.gz";
    sha256 = "1r3ll77hyqn28d9i4cf3vqd9v48fmaa1j8ps8c4fm4f8gqf4kpl1";
  };
  naerskSrc = builtins.fetchTarball {
    url = "https://github.com/nmattia/naersk/archive/e0fe990b478a66178a58c69cf53daec0478ca6f9.tar.gz";
    sha256 = "0qjyfmw5v7s6ynjns4a61vlyj9cghj7vbpgrp9147ngb1f8krz2c";
  };
  rustOverlaySrc = builtins.fetchTarball {
    # The following is the latest version as of 2021-04-10
    url = "https://github.com/mozilla/nixpkgs-mozilla/archive/8c007b60731c07dd7a052cce508de3bb1ae849b4.tar.gz";
    sha256 = "1zybp62zz0h077zm2zmqs2wcg3whg6jqaah9hcl1gv4x8af4zhs6";
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
    date = "2021-05-07";
    channel = "nightly";
    sha256 = "1l9aj3ig9yyxi5s41623wzvfinza656gnbrqay5ngl6yah5h0rs8";
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
