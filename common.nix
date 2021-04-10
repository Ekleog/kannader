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
    date = "2021-03-25";
    channel = "nightly";
    sha256 = "0pd74f1wc5mf8psrq3mr3xdzwynqil7wizaqq8s7kqfgxx4c4l7w";
  };
  naerskRaw = pkgs.callPackage naerskSrc {
    rustc = rustNightlyChannelRaw.rust;
    cargo = rustNightlyChannelRaw.cargo;
  };
  rustNightlyChannel = rustNightlyChannelRaw // {
    rust = rustNightlyChannelRaw.rust.override {
      targets = ["wasm32-wasi"];
    };
    # TODO: remove override when https://github.com/rust-lang/cargo/pull/9030
    # lands
    cargo = naerskRaw.buildPackage {
      pname = "cargo";
      version = "dev";
      src = builtins.fetchTarball {
        url = "https://github.com/rust-lang/cargo/archive/f11237ac03d3d51b5320364fd9997e62abb50f62.tar.gz";
        sha256 = "1bk5vimjxr68v6c72q8zgkbq79wbd5a95d9cbw2linrivv5n8vjp";
      };
      buildInputs = with pkgs; [ openssl pkg-config ];
      singleStep = true;
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
