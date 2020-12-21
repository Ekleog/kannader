# How The Queue Works

## High-Level Overview

Yuubind, as a library design, supports any storage mechanism that can
implement the
[`Storage`](https://ekleog.github.io/yuubind/dev-doc/smtp_queue/trait.Storage.html)
trait.

The core idea of this trait is, that each function must either
succeed or return an error for future retry.

The storage must provide basically three sets of primitives:
 - For listing the emails that were left over from previous runs:
    - `list_queue`, `find_inflight` and `find_pending_cleanup`
 - For queuing emails and reading them from the storage:
    - `enqueue` and `read_inflight`
 - For handling state changes for each email:
    - `reschedule`
    - `send_start`, `send_done` and `send_cancel`
    - `drop` and `cleanup`

This being said, we do not expect system administrators to write their
own storage systems, unless they have very particular needs. As a
consequence, an implementation is provided with yuubind, and bundled
in yuubind the executable, that works with a local filesystem queue
like most other SMTP servers do by default.

## Provided implementation: queueing with the local filesystem

### File Structure

 - `<queue>/data`: location for the contents and metadata of the
   emails in the queue
 - `<queue>/queue`: folder for holding symlinks to the emails
 - `<queue>/inflight`: folder for holding symlinks to the emails that
   are currently in flight
 - `<queue>/cleanup`: folder for holding symlinks to the emails that
   are currently being deleted after being successfully sent

`<queue>/data` is the only directory that holds things that are not
symbolic links. All other folders only hold symbolic links that must
point into `<queue>/data` as relative links.

### Assumptions

 - Moving a symlink to another folder is atomic between
   `<queue>/queue`, `<queue>/inflight` and `<queue>/cleanup`
 - Moving a file is atomic between files in the same `<queue>/data/**`
   folder
 - Creating a symlink in the `<queue>/queue` folder is atomic
 - Once a write is flushed without error, it is guaranteed not to be
   changed by something other than a yuubind instance (or another
   system aware of yuubind's protocol and guarantees)

### `<queue>/data`

Each email in `<queue>/data` is a folder, that is constituted of:
 - `<mail>/contents`: the RFC5322 content of the email
 - `<mail>/<dest>/metadata`: the JSON-encoded `MailMetadata<U>`
 - `<mail>/<dest>/schedule`: the JSON-encoded `ScheduleInfo` couple

Both `<mail>/<dest>/metadata` and `<mail>/<dest>/schedule` could
change over time. In this case, the replacement gets written by
writing a `<filename>.{{random_uuid}}` then renaming it in-place.

### Enqueuing Process

When enqueuing, the process is:
 - Create `<queue>/data/<uuid>`, thereafter named `<mail>`
 - For each destination (ie. recipient email address):
   + Create `<mail>/<uuid>`, thereafter named `<mail>/<dest>`
   + Write `<mail>/<dest>/schedule` and `<mail>/<dest>/metadata`
 - Give out the Enqueuer to the user for writing `<mail>/contents`
 - Wait for the user to commit the Enqueuer
 - Create a symlink from `<queue>/queue/<uuid>` to `<mail>/<dest>` for
   each destination

### Starting and Cancelling Sends

When starting to send or cancelling a send, the process is:
 - Move `<queue>/queue/<id>` to `<queue>/inflight/<id>` (or back)

### Cleaning Up

When done with sending a mail and it thus needs to be removed from
disk, the process is:
 - Move `<queue>/inflight/<id>` to `<queue>/cleanup/<id>`
 - Remove `<queue>/cleanup/<id>/*` (which actually are in
   `<queue>/data/<mail>/<dest>/*`)
 - Remove the target of `<queue>/cleanup/<id>` (the folder in
   `<queue>/data/<mail>`)
 - Check whether only `<queue>/data/<mail>/contents` remains, and if
   so remove it as well as the `<queue>/data/<mail>` folder
 - Remove the `<queue>/cleanup/<id>` symlink
