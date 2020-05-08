# Design Principles

## Security

### No high privilege - no root

For historical reasons smtpd servers required root privilege to run. This
necessity was explained by several things:

- the ability to use TCP port 25 on Unix-like systems requires root privilge.
  (on modern Linux at least `CAP_NET_BIND_SERVICE` capability is requred)

- the ability to ignore DAC and write mail to user's directory requries root
  (`CAP_CHOWN` and `CAP_DAC_OVERRIDE`)

- amusingly, the ability to drop privilege requies root too. 
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

This practially obsoletes the necessity to support mbox or Maildir in SMTP
server. Accepted mail should just be handed over to MDA via LMTP.

One might argue that privilege separation is still necessary to ensure security
and separation of concerns (even Rust might have volunerabilities discovered in
the future). But this also can be delegated to the OS's service managers - it 
the tool that is designed to orchestrate processes.

All this makes it possible to run SMTP server as a non-privileg user (or set of 
non-privileged users)

### Safe Programming Languate - Rust


### Deprecate legacy interfaces - no mbox, .forward and alike


## Simplicity

Do not re-invent the wheel.

- Do not orchestrate processes - offload this work to
  the OS service manager.
- Don't do MDA's work - let MDA deliver messages to users. Hand the mail to MDA
  via LMTP

## Configurability

Present discoverable, structured and flexible configuration. 

(ideas to expand: 
- don't be like postfix's mess
- don't force user into a limited config like OpenSMTPD to allow filters as
  escape hatch later.
- be more like awesomewm with lua
)



# High-Level Architecure

## Tier 1

`smtp-server` - exposes an `interact` function, that takes an input
stream, an output sink (defined by the `futures` crate) that are assumed to be
bound to an SMTP client, and handles the SMTP interaction with this single
client.

`smtp-message` - bytes-to-command parsing

`smtp-queue`  - queue

`smtp-client` - relaying 


## Tier 2 

`api` - expose API of Tier 1 crates

`config` - interact with API with a scripted language (yet to be determined which one)


## Tier 3

- binary targets

- library targets 






