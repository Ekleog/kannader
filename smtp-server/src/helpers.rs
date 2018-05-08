use bytes::BytesMut;
use itertools::Itertools;
use tokio::prelude::*;

use smtp_message::*;

// TODO: find a better home for all this stuff

pub struct ConnectionMetadata<U> {
    pub user: U,
}

// TODO(low): make pub fields private?
pub struct MailMetadata {
    pub from: Option<Email>,
    pub to:   Vec<Email>,
}

// TODO(low): make pub fields private?
// TODO(low): merge into Decision<T> once Reply is a thing
pub struct Refusal {
    pub code: ReplyCode,
    pub msg:  SmtpString,
}

pub enum Decision {
    Accept,
    Reject(Refusal),
}

// TODO: try removing as much lifetimes as possible from the whole mess

// Panics if `text` has a byte not in {9} \union [32; 126]
pub fn send_reply<'a, W>(
    writer: W,
    code: ReplyCode,
    text: SmtpString,
) -> impl Future<Item = W, Error = W::SinkError> + 'a
where
    W: 'a + Sink<SinkItem = ReplyLine>,
    W::SinkError: 'a,
{
    let replies = text.byte_chunks(ReplyLine::MAX_LEN)
        .with_position()
        .map(move |t| {
            use itertools::Position::*;
            match t {
                First(t) | Middle(t) => ReplyLine::build(code, IsLastLine::No, t).unwrap(),
                Last(t) | Only(t) => ReplyLine::build(code, IsLastLine::Yes, t).unwrap(),
            }
        });
    // TODO: do not use send_all as it closes the writer, use start_send and
    // poll_complete instead (or even refactor to move this logic into
    // smtp_message::ReplyLine?)
    writer.send_all(stream::iter_ok(replies)).map(|(w, _)| w)
}

pub struct CrlfLines<S: Stream<Item = BytesMut>> {
    source: Prependable<S>,
    buf:    BytesMut,
}

impl<S: Stream<Item = BytesMut>> CrlfLines<S> {
    pub fn new(s: Prependable<S>) -> CrlfLines<S> {
        CrlfLines {
            source: s,
            buf:    BytesMut::new(),
        }
    }

    pub fn into_inner(mut self) -> Prependable<S> {
        if !self.buf.is_empty() {
            // If this `unwrap` fails, this means that somehow:
            //  1. The stream passed to `new` was already prepended
            //  2. Somehow the buffer has been filled without ever pulling a single element
            //     from the stream
            // So, quite obviously, that'd be a programming error from here, so let's just
            // unwrap
            self.source.prepend(self.buf).unwrap();
        }
        self.source
    }
}

impl<S: Stream<Item = BytesMut>> Stream for CrlfLines<S> {
    type Item = BytesMut;
    type Error = S::Error;

    fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
        use self::Async::*;

        // First, empty the current buffer
        if let Some(pos) = self.buf.windows(2).position(|x| x == b"\r\n") {
            return Ok(Ready(Some(self.buf.split_to(pos + 2))));
        }

        // Then ask for more until a complete line is found
        loop {
            match self.source.poll()? {
                NotReady => return Ok(NotReady),
                Ready(None) => return Ok(Ready(None)), // Drop self.buf
                Ready(Some(b)) => {
                    // TODO(low): implement line length limits
                    // TODO(low): can do with much fewer allocations and searches through the
                    // buffer (by not extending the buffers straightaway but storing them in a vec
                    // until the CRLF is found, and then extending with the right size)
                    self.buf.unsplit(b);
                    if let Some(pos) = self.buf.windows(2).position(|x| x == b"\r\n") {
                        return Ok(Ready(Some(self.buf.split_to(pos + 2))));
                    }
                }
            }
        }
    }
}

pub enum FutIn11<T, E, F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11>
where
    F1: Future<Item = T, Error = E>,
    F2: Future<Item = T, Error = E>,
    F3: Future<Item = T, Error = E>,
    F4: Future<Item = T, Error = E>,
    F5: Future<Item = T, Error = E>,
    F6: Future<Item = T, Error = E>,
    F7: Future<Item = T, Error = E>,
    F8: Future<Item = T, Error = E>,
    F9: Future<Item = T, Error = E>,
    F10: Future<Item = T, Error = E>,
    F11: Future<Item = T, Error = E>,
{
    Fut1(F1),
    Fut2(F2),
    Fut3(F3),
    Fut4(F4),
    Fut5(F5),
    Fut6(F6),
    Fut7(F7),
    Fut8(F8),
    Fut9(F9),
    Fut10(F10),
    Fut11(F11),
}

impl<T, E, F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11> Future
    for FutIn11<T, E, F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11>
where
    F1: Future<Item = T, Error = E>,
    F2: Future<Item = T, Error = E>,
    F3: Future<Item = T, Error = E>,
    F4: Future<Item = T, Error = E>,
    F5: Future<Item = T, Error = E>,
    F6: Future<Item = T, Error = E>,
    F7: Future<Item = T, Error = E>,
    F8: Future<Item = T, Error = E>,
    F9: Future<Item = T, Error = E>,
    F10: Future<Item = T, Error = E>,
    F11: Future<Item = T, Error = E>,
{
    type Item = T;
    type Error = E;

    fn poll(&mut self) -> Result<Async<Self::Item>, Self::Error> {
        use self::FutIn11::*;
        match *self {
            Fut1(ref mut f) => f.poll(),
            Fut2(ref mut f) => f.poll(),
            Fut3(ref mut f) => f.poll(),
            Fut4(ref mut f) => f.poll(),
            Fut5(ref mut f) => f.poll(),
            Fut6(ref mut f) => f.poll(),
            Fut7(ref mut f) => f.poll(),
            Fut8(ref mut f) => f.poll(),
            Fut9(ref mut f) => f.poll(),
            Fut10(ref mut f) => f.poll(),
            Fut11(ref mut f) => f.poll(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crlflines_looks_good() {
        let stream = CrlfLines::new(
            stream::iter_ok(
                vec![
                    &b"MAIL FROM:<foo@bar.example.org>\r\n"[..],
                    b"RCPT TO:<baz@quux.example.org>\r\n",
                    b"RCPT TO:<foo2@bar.example.org>\r\n",
                    b"DATA\r\n",
                    b"Hello World\r\n",
                    b".\r\n",
                    b"QUIT\r\n",
                ].into_iter()
                    .map(BytesMut::from),
            ).map_err(|()| ())
                .prependable(),
        );

        assert_eq!(
            stream.collect().wait().unwrap(),
            vec![
                b"MAIL FROM:<foo@bar.example.org>\r\n".to_vec(),
                b"RCPT TO:<baz@quux.example.org>\r\n".to_vec(),
                b"RCPT TO:<foo2@bar.example.org>\r\n".to_vec(),
                b"DATA\r\n".to_vec(),
                b"Hello World\r\n".to_vec(),
                b".\r\n".to_vec(),
                b"QUIT\r\n".to_vec(),
            ]
        );
    }
}
