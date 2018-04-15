extern crate tokio;

use tokio::prelude::*;

pub enum Decision {
    Accept,
    Reject,
}

pub struct Metadata {
    from: String,
    to: Vec<String>,
}

pub enum Error {}

pub fn interact<
    R: AsyncRead,
    W: AsyncWrite,
    F: FnMut(Metadata, &Stream<Item = String, Error = Error>) -> Decision,
>(incoming: R, outgoing: W, handler: F) {
}
