use bytes::BytesMut;
use smtp_message::Prependable;
use tokio::prelude::*;

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

#[cfg(test)]
mod tests {
    use super::*;
    use smtp_message::StreamExt;

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
