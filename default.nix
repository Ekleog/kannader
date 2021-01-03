{ release ? false }:

with import ./common.nix;

naersk.buildPackage {
  pname = "kannader";
  version = "dev";

  src = pkgs.lib.sourceFilesBySuffices ./. [".rs" ".toml" ".lock" ".capnp"];

  buildInputs = with pkgs; [
    capnproto
  ];

  copyLibs = true;

  inherit release;
}
