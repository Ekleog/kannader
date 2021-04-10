{ pkgs, ... }:

let
  # TODO: should have pkgs.kannader instead of this import, after
  # having figured out how to make sure pkgs is the overlayed pkgs
  kannader = import ../../default.nix {};
in
{
  networking.firewall.allowedTCPPorts = [ 2525 ];

  systemd.services.kannader = {
    wantedBy = ["multi-user.target"];
    after = ["network.target"];
    script = ''
      RUST_LOG="debug,cranelift=info" RUST_BACKTRACE=1 \
        ${kannader}/bin/kannader \
          -b ${kannader}/bin/forwarder
    '';
    preStart = ''
      #!/bin/sh
      set -ex

      mkdir /tmp/kannader

      cp ${pkgs.writeText "cert.pem" ''
        -----BEGIN CERTIFICATE-----
        MIIEpzCCAo8CAgKaMA0GCSqGSIb3DQEBCwUAMBYxFDASBgNVBAMMC1NuYWtlb2ls
        IENBMCAXDTE4MDcxMjAwMjIxOVoYDzIxMTgwNjE4MDAyMjE5WjAaMRgwFgYDVQQD
        DA9sZXRzZW5jcnlwdC5vcmcwggIiMA0GCSqGSIb3DQEBAQUAA4ICDwAwggIKAoIC
        AQDA++GXB6aA+Lr3X2xIPs/PqXoJF9TUb98NzQC+ww3+dOaIY0Omqf15/UID5G0u
        0649zUsvISi9x+7vn9X7opKA7iTX0TYKsUIXCMQ5YMYXgOByVPfvnVYaYUmDM8R9
        l+Fs6uZB9Bq7zorEC01lWn0XFwpu4vJD+w03F80wH7sgdtWHC/NVyd0elhkQ/qR2
        fC80s/bZui5fmrdq/FZK2WfTW5nQF9SbhfDFpQ0hTu3QO/noUr0L2Fb0Blp7R2jQ
        EkFZEXvFLAt4Mmqf25nP6xfRf+0hppJBIG5nJKPQd7lkrN1Q7S2nHbFNLYGuXdjY
        E4lo4T3we5ta90qiyUu3hfatWszEZrzpKYXNzIMvVFKjBw/+u7LLnnmXul6kvmzh
        yYyBEEFePm1eGpRNU6flY74kKUuT6u2+0BPiuiAmYyoQqYzDa3ss3LVb2uL/fvxA
        b+LcAZosgv/saw9u5E7nVJ22LhAxV7bM0XN7vabM2aPVJeiRbLv5m+NQynnmgSNF
        GxM9Rl004QclbLo6zbOe6ovtmkgcLEEbwUqeQJWJdEmoADxvNwSFH6ktqYsg4eW/
        iR3MnqmEeRK8rryq9tFF5SY6MmBK6lUEj2OEr58drL9RZHjN8lB5330fxk6iJeWs
        sJWz5U8PKdRQqvMXWdjKiU3OL81Lh7gBud9aDILkkpGmNQIDAQABMA0GCSqGSIb3
        DQEBCwUAA4ICAQAkx3jcryukAuYP7PQxMy3LElOl65ZFVqxDtTDlr7DvAkWJzVCb
        g08L6Tu+K0rKh2RbG/PqS0+8/jBgc4IwSOPfDDAX+sinfj0kwXG34WMzB0G3fQzU
        2BMplJDOaBcNqHG8pLP1BG+9HAtR/RHe9p2Jw8LG2qmZs6uemPT/nCTNoyIL4oxh
        UncjETV4ayCHDKD1XA7/icgddYsnfLQHWuIMuCrmQCHo0uQAd7qVHfUWZ+gcsZx0
        jTNCcaI8OTS2S65Bjaq2HaM7GMcUYNUD2vSyNQeQbha4ZeyZ9bPyFzznPMmrPXQe
        MJdkbJ009RQIG9As79En4m+l+/6zrdx4DNdROqaL6YNiSebWMnuFHpMW/rCnhrT/
        HYadijHOiJJGj9tWSdC4XJs7fvZW3crMPUYxpOvl01xW2ZlgaekILi1FAjSMQVoV
        NhWstdGCKJdthJqLL5MtNdfgihKcmgkJqKFXTkPv7sgAQCopu6X+S+srCgn856Lv
        21haRWZa8Ml+E0L/ticT8Fd8Luysc6K9TJ4mT8ENC5ywvgDlEkwBD3yvINXm5lg1
        xOIxv/Ye5gFk1knuM7OzpUFBrXUHdVVxflCUqNAhFPbcXwjgEQ+A+S5B0vI6Ohue
        ZnR/wuiou6Y+Yzh8XfqL/3H18mGDdjyMXI1B6l4Judk000UVyr46cnI7mw==
        -----END CERTIFICATE-----
      ''} /tmp/kannader/cert.pem

      cp ${pkgs.writeText "key.pem" ''
        -----BEGIN PRIVATE KEY-----
        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
        Gvfq54FbDuB10XP8JdLrsy9Z6GEsmoE=
        -----END PRIVATE KEY-----
      ''} /tmp/kannader/key.pem
    '';
  };
}
