{ release ? false }:

with import ./common.nix;

naersk.buildPackage {
  pname = "yuubind";
  version = "dev";

  src = pkgs.lib.sourceFilesBySuffices ./. [".rs" ".toml" ".lock"];

  inherit release;
}
