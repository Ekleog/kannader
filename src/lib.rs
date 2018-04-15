extern crate tokio;

use std::io::BufRead;
use tokio::prelude::*;

pub type MailAddress = Vec<u8>;
pub type MailAddressRef<'a> = &'a [u8];

pub struct ConnectionMetadata {}

pub struct MailMetadata {
    from: MailAddress,
    to: Vec<MailAddress>,
}

pub struct Refusal {
    code: u16,
    msg: String,
}

pub enum Decision {
    Accept,
    Reject(Refusal),
}

pub fn interact<
    Reader: AsyncRead + BufRead,
    Writer: AsyncWrite,
    State,
    FilterFrom: FnMut(MailAddressRef, &ConnectionMetadata) -> Result<State, Refusal>,
    FilterTo: FnMut(MailAddressRef, State, &ConnectionMetadata, &MailMetadata)
                    -> Result<State, Refusal>,
    HandleMail: FnMut(MailMetadata, State, &AsyncRead) -> Decision,
>(incoming: Reader, outgoing: Writer, filter_from: FilterFrom, filter_to: FilterTo, handler: HandleMail){
}
