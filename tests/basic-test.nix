with import ../common.nix;

import (pkgs.path + "/nixos/tests/make-test-python.nix") (_: {
  name = "basic-test";

  nodes = {
    # NOTE: THE ALPHABETIC ORDER OF THE NAMES IS IMPORTANT FOR THE IP
    # ADDRESS TO STAY CORRECT

    # 192.168.1.1
    srv1_py_smtplib = _: {
      imports = [ ./py-smtplib-client.nix ];
    };

    # 192.168.1.2
    srv2_yuubind = { pkgs, ... }: {
      imports = [ ./yuubind.nix ];

      # TODO: also test ipv6
      services.unbound = {
        enable = true;
        extraConfig = ''
          server:
            local-data: "a.opensmtpd.local. IN A 192.168.1.3"
            local-data: "mx.a.opensmtpd.local. IN A 111.111.111.111"
            local-data: "mx.a.opensmtpd.local. IN MX 10 a.opensmtpd.local."
        '';
      };

      networking.resolvconf.useLocalResolver = true;
    };

    # 192.168.1.3
    srv3_opensmtpd = _: {
      imports = [
        ./opensmtpd-server.nix
        (pkgs.path + "/nixos/tests/common/user-account.nix")
      ];
    };
  };

  testScript = ''
    start_all()

    srv1_py_smtplib.wait_for_unit("network-online.target")
    srv2_yuubind.wait_for_unit("unbound")
    srv2_yuubind.wait_for_open_port(2525)
    srv3_opensmtpd.wait_for_open_port(25)

    srv1_py_smtplib.succeed(
        "send-test-mail 192.168.1.2 2525 'alice@local' 'bob@[192.168.1.3]' 'Subject: hello\n\nwanna eat cookies?'"
    )
    srv1_py_smtplib.succeed(
        "send-test-mail 192.168.1.2 2525 'alice@local' 'bob@a.opensmtpd.local' 'Subject: hello\n\nwanna eat cookies?'"
    )
    srv1_py_smtplib.succeed(
        "send-test-mail 192.168.1.2 2525 'alice@local' 'bob@mx.a.opensmtpd.local' 'Subject: hello\n\nwanna eat cookies?'"
    )

    # For some reason opensmtpd is *slow* while I'm trying this (as in
    # close to 300s to answer connections), so let's just send a useless
    # email to it so we wait 'till it works

    srv1_py_smtplib.succeed(
        "send-test-mail 192.168.1.3 25 'alice@local' 'bob@useless' 'Subject: hello\n\nwanna eat cookies?'"
    )

    # TODO: replace this with waits on the proper commands that list the queues
    srv2_yuubind.succeed("sleep 10")

    srv3_opensmtpd.succeed("check-test-mail 'alice@local' 'bob@[192.168.1.3]' 'cookies'")
    srv3_opensmtpd.succeed(
        "check-test-mail 'alice@local' 'bob@a.opensmtpd.local' 'cookies'"
    )
    srv3_opensmtpd.succeed(
        "check-test-mail 'alice@local' 'bob@mx.a.opensmtpd.local' 'cookies'"
    )
  '';
})
