extern crate smtp_message;
extern crate tokio;

use smtp_message::*;
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
    'a,
    ReaderError,
    Reader: 'a + Stream<Item = u8, Error = ReaderError>,
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
) -> Box<'a + Future<Item = (), Error = ()>> {
    // TODO: return `impl Future`
    Box::new(
        CrlfLines::new(incoming)
            .map_err(|_| ())
            .fold((), |_, _| future::ok(())),
    )
}

struct CrlfLines<S> {
    source: S,
    buf:    Vec<u8>,
}

impl<S: Stream<Item = u8>> CrlfLines<S> {
    fn new(s: S) -> CrlfLines<S> {
        CrlfLines {
            source: s,
            buf:    Vec::with_capacity(1024),
        }
    }
}

impl<S: Stream<Item = u8>> Stream for CrlfLines<S> {
    type Item = Vec<u8>;
    type Error = S::Error;

    fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
        unimplemented!() // TODO
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
