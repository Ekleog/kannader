extern crate bytes;
extern crate smtp_message;
extern crate tokio;

mod mail;
mod run;
mod send_queued_mail;
mod storage;
mod transport;

pub use mail::{InflightMail, Mail, QueuedMail};
pub use run::run;
pub use storage::Storage;
pub use transport::Transport;

// TODO: (B) make this not do bad things with multiple instances
// Use cases to take into account:
//  * By mistake, multiple instances have been started with the same queue
//    directory
//  * The user wants to modify by hand data in the queue for some reason, it's
//    better not to have to shut down the server in order to do that (esp. as
//    they may forget to do it)
// Idea:
//  * Use the `notify` crate to know when a mail has been added to the queue
//    directory
//  * Before sending mails, move them to an in-progress directory so that
//    multiple simultaneously-running instances don't send the same mail at the
//    same time
//  * If there is a crash, a mail may be stuck in this in-progress directory.
//    So, at startup:
//     * Also scan the in-progress directory
//     * If there is a mail there, it *could* be in the process of being sent,
//       so wait long enough (1 hour?) to be sure all timeouts are passed, and
//       check if it is still there.
//     * If it is still there, then it means that it was left here after a crash
//       while sending it, as the name in the in-progress directory is randomly
//       picked (so even if it was actually in-progress and had been
//       re-scheduled and put back in the in-progress directory, it would have a
//       new name)
