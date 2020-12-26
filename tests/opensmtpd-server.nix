{ pkgs, ... }:

let
  check_test_mail = pkgs.writeScriptBin "check-test-mail" ''
    #!/bin/sh

    echo "listing /tmp/mail-from-*"
    echo "===> " /tmp/mail-from-*

    echo "---"
    echo "contents of the file:"
    cat "/tmp/mail-from-$1-to-$2.mail"
    echo "---"

    exec grep "$3" "/tmp/mail-from-$1-to-$2.mail"
  '';
in
{
  networking.firewall.allowedTCPPorts = [ 25 ];

  environment.systemPackages = [ check_test_mail ];

  # TODO: figure out a way to test STARTTLS
  services.opensmtpd = {
    enable = true;

    extraServerArgs = [ "-v" "-T all" ];

    serverConfiguration = ''
      listen on 0.0.0.0

      action "send-to-tmp" mda "${pkgs.coreutils}/bin/tee '/tmp/mail-from-%{sender}-to-%{rcpt}.mail'"

      match from any for any action "send-to-tmp"
    '';
  };
}
