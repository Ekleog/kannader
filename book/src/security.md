# Security

## No high privilege - no root

For historical reasons smtpd servers required root privilege to run. This
necessity was explained by several things:

- the ability to use TCP port 25 on Unix-like systems requires root privilege.
  (on modern Linux at least `CAP_NET_BIND_SERVICE` capability is required)

- the ability to ignore DAC and write mail to user's directory requries root
  (`CAP_CHOWN` and `CAP_DAC_OVERRIDE`)

- amusingly, the ability to drop privilege requires root too. 
  (to employ privilege separation techniques, e.g. spawn child
  processes and set their UID and GID to a non-privileged user root or `CAP_SETUID`
  and `CAP_SETGID` capabilities are needed)


But all of the above are rudiments of the past.

> It is no longer a catastrophe if an unprivileged process binds to transport
> layer ports less than 1024. Everyone should consider reading and writing the
> network medium as unlimited due to hardware no longer costing a million
> dollars, regardless of what an operating system does.

[Rise and Fall of the Operating System](http://www.fixup.fi/misc/usenix-login-2015/login_oct15_02_kantee.pdf)

The very action of opening TCP port 25 can be delegated to a very small
privileged program or OS service manager. OS service manager runs with root
privilege anyway. SMTP server can accept the passed descriptor and use it
without ever having to escalate privilege.

Nowadays people are not logging in to the mail server via ssh to check mail.
Modern mail servers are running MDAs like Dovecot to present an IMAP interface
to the user. Very likely the mail itself is stored in a virtual mailboxes, owned 
by one user (usually `vmail`)

This practically obsoletes the necessity to support mbox or Maildir in the SMTP
server. Accepted mail can be handed over to MDA via LMTP or piped into another
executable.

One might argue that privilege separation is still necessary to ensure security
and separation of concerns (even Rust might have vulnerabilities discovered in
the future). But this also can be delegated to the OS's service managers - it 
is a tool that is designed to orchestrate processes.

All this makes it possible to run the SMTP server as a non-privileged user (or set of 
non-privileged users)

## Safe Programming Language - Rust

TODO: (C) expand why Rust

## Deprecate legacy interfaces - no mbox, .forward and alike

TODO: (C) expand why lagacy interfaces negaively impact security

> I think several of these errata help demonstrate that principles like
> eliminating legacy interfaces and reducing complexity are vital to
> maintaining security. 

[rethinking openbsd security](https://flak.tedunangst.com/post/rethinking-openbsd-security) 
by Ted Unangst
