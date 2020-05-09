# High-Level Architecture Overview

## Code layout

- [`smtp-message`](https://ekleog.github.io/yuubind/dev-doc/smtp_message/index.html)
handles the SMTP protocol, at the parsing and serialization level.

- [`smtp-server`](https://ekleog.github.io/yuubind/dev-doc/smtp_server/index.html)
exposes an `interact` function, that semantically takes in a
configuration and a client with which it's going to interact, and
handles the SMTP interaction with this single client.

- [`smtp-queue`](https://ekleog.github.io/yuubind/dev-doc/smtp_queue/index.html)
runs a queue for use by SMTP servers, delegating to a storage handler
and a transport for sending messages that have reached their scheduled
time for sending.

### Not yet implemented

- `smtp-queue-fs` implements a storage handler for `smtp-queue` that
relies on the filesystem.

- `smtp-client` relays emails to external email servers.

- `yuubind` exposes the API of all above crates to consumers an an
more opinionated way.

- `yuubind-bin` interacts with API with hooks, so that changing the
configuration does not require rebuilding the whole server — see [the
configuration format chapter](./configuration_format.md) for more
details
