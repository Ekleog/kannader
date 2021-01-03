# Simplicity

Do not re-invent the wheel.

- Do one job - transfer mail. Transfer to MDAs and other SMTP servers
- Do not orchestrate processes - offload this work to the OS service manager.
- Don't do MDA's work - let MDA deliver messages to users. Hand the mail to MDA
  via LMTP

On the other hand, simplicity does not mean that things should be hard
to get to work -- quite the contrary. For example with the OS service
manager, with simple things built, it is possible to either use the OS
service manager to handle orchestration (the preferred solution), but
also to run a minimal wrapper that kannader provides. This is required
for the cases where the OS service manager has different abilities
than the one for which kannader was designed.
