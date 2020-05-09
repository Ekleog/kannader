# High-Level Architecture Overview

`smtp-server` - exposes an `interact` function, that takes an input
stream, an output sink (defined by the `futures` crate) that are assumed to be
bound to an SMTP client, and handles the SMTP interaction with this single
client.

`smtp-message` - bytes-to-command parsing

`smtp-queue`  - queue

`smtp-client` - relaying 

`api` - expose API to consumers

`config` - interact with API with a scripted language (yet to be determined which one)

