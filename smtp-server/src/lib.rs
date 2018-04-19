extern crate smtp_message;
extern crate tokio;

use smtp_message::*;
use std::io::BufRead;
use tokio::prelude::*;

pub type MailAddress = Vec<u8>;
pub type MailAddressRef<'a> = &'a [u8];

pub struct ConnectionMetadata {}

pub struct MailMetadata {
    from: MailAddress,
    to:   Vec<MailAddress>,
}

pub struct Refusal {
    code: ReplyCode,
    msg:  String,
}

pub enum Decision<T> {
    Accept(T),
    Reject(Refusal),
}

pub fn interact<
    ReaderError,
    Reader: Stream<Item = u8, Error = ReaderError>,
    WriterError,
    Writer: Sink<SinkItem = u8, SinkError = WriterError>,
    HandleReaderError: FnMut(ReaderError) -> (),
    HandleWriterError: FnMut(WriterError) -> (),
    State,
    FilterFrom: FnMut(MailAddressRef, &ConnectionMetadata) -> Decision<State>,
    FilterTo: FnMut(MailAddressRef, State, &ConnectionMetadata, &MailMetadata) -> Decision<State>,
    HandleMail: FnMut(MailMetadata, State, &AsyncRead) -> Decision<()>,
>(
    incoming: Reader,
    outgoing: &mut Writer,
    handle_reader_error: HandleReaderError,
    handle_writer_error: HandleWriterError,
    filter_from: FilterFrom,
    filter_to: FilterTo,
    handler: HandleMail,
) -> Box<Future<Item = (), Error = ()>> {
    // TODO: impl Future
    Box::new(future::empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Cursor;

    #[test]
    fn it_works() {
        let mut vec = Vec::new();
        interact(
            stream::iter_ok(b"foo bar".iter().cloned()),
            &mut vec,
            |()| (),
            |()| (),
            |_, _| Decision::Accept(()),
            |_, _, _, _| Decision::Accept(()),
            |_, _, _| {
                Decision::Reject(Refusal {
                    code: ReplyCode::POLICY_REASON,
                    msg:  "foo".to_owned(),
                })
            },
        );
    }
}
