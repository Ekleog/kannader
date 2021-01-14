{ pkgs, ... }:

let
  send_test_mail = pkgs.writeScriptBin "send-test-mail" ''
    #!${pkgs.python3.interpreter}
    import smtplib, sys

    server = sys.argv[1]
    port = int(sys.argv[2])
    orig = sys.argv[3]
    to = sys.argv[4]
    contents = sys.argv[5]

    print("server =", server, "; port =", port, "; orig =", orig, "; to =", to, "; contents =", contents)

    with smtplib.SMTP(server, port) as smtp:
      smtp.set_debuglevel(2)
      smtp.sendmail(orig, to, contents)
  '';
in
{
  environment.systemPackages = [ send_test_mail ];
}
