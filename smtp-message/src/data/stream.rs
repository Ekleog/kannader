use std::{
    pin::Pin,
    task::{Context, Poll},
};

use bytes::BytesMut;
use futures::prelude::*;

use crate::streamext::Prependable;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DataStreamState {
    Running,
    CrPassed,
    CrLfPassed,
    Finished,
    EarlyFin,
    Completed,
}

// Stream adapter that takes as input a stream that yields ByteMut elements,
// and outputs data unescaped as per RFC5321.
//
// The input stream should start giving elements since just after DATA\r\n, and
// will be consumed until the \r\n.\r\n sequence is found (or .\r\n if these
// are the first three bytes). The DataStream will output the unescaped data
// (ie. replacing \r\n. with \r\n when not in the \r\n.\r\n sequence) up to and
// including the first \r\n in \r\n.\r\n.
//
// Once the \r\n.\r\n sequence will have been reached, this stream will be
// terminated. At this point (and *not* before, as that would panic), please
// call `into_inner` to recover the original stream, advanced until after the
// \r\n.\r\n sequence.
//
// In order to handle the case of a packet received that doesn't end exactly
// after the \r\n.\r\n, the received stream must be Prependable, so that the
// additional data can be pushed back into it if need be.
pub struct DataStream<'a, S: Stream<Item = BytesMut>> {
    source: Pin<&'a mut Prependable<S>>,
    // state is the state of the state machine at the BEGINNING of `buf`
    state: DataStreamState,
    buf:   BytesMut,
}

impl<'a, S: Stream<Item = BytesMut>> DataStream<'a, S> {
    pub fn new(source: Pin<&mut Prependable<S>>) -> DataStream<S> {
        DataStream {
            source,
            state: DataStreamState::CrLfPassed,
            buf: BytesMut::new(),
        }
    }

    // Beware: this will panic if it hasn't been fully consumed.
    // If there has been an early EOF in the incoming stream, return Err(()).
    pub fn complete(&mut self) -> Result<(), ()> {
        if self.state == DataStreamState::EarlyFin {
            // TODO: (B) distinguish from successful completion?
            self.state = DataStreamState::Completed;
            return Err(());
        }
        assert_eq!(self.state, DataStreamState::Finished);
        if !self.buf.is_empty() {
            // If this `unwrap` fails, this means that somehow:
            //  1. The stream passed to `new` was already prepended
            //  2. Somehow the state managed to go into `Finished` and the buffer has been
            //     filled without ever pulling a single element from the stream
            // So, quite obviously, that'd be a programming error from here, so let's just
            // unwrap
            self.source.as_mut().prepend(self.buf.split_off(0)).unwrap();
        }
        self.state = DataStreamState::Completed;
        Ok(())
    }

    pub fn was_completed(&self) -> bool {
        self.state == DataStreamState::Completed
    }
}

// TODO: (B) remove unpin marker hide:https://github.com/rust-lang-nursery/futures-rs/issues/1547
impl<'a, S: Stream<Item = BytesMut> + Unpin> Stream for DataStream<'a, S> {
    type Item = BytesMut;

    fn poll_next(mut self: Pin<&mut Self>, ctxt: &mut Context) -> Poll<Option<Self::Item>> {
        use self::DataStreamState::*;
        use Poll::*;
        // First, handle the case when we're done
        if self.state == Finished {
            return Ready(None);
        }
        loop {
            // Figure out what to send from the current buf
            #[derive(Debug, Eq, PartialEq)]
            enum BufSplit {
                Nowhere,        // Should send the whole buffer as a result
                Eof(usize),     // Should send [arg] bytes as a result, then skip .\r\n and EOF
                Escape(usize),  // Should send [arg] bytes as a result, then skip a dot
                Unknown(usize), // Should send [arg] bytes as a result, then wait for more data
            }
            let mut split = BufSplit::Nowhere;

            // First, look at all that's in the buffe rexcept for the last 2 characters
            let mut state = self.state; // Temporary variable to please the borrow checker
            for (idx, w) in self.buf.windows(3).enumerate() {
                match (state, w[0]) {
                    // Move forward in the \r\n state machine
                    (_, b'\r') => state = CrPassed,
                    (CrPassed, b'\n') => state = CrLfPassed,

                    // If there is a \r\n., what should we do?
                    (CrLfPassed, b'.') if w == b".\r\n" => {
                        split = BufSplit::Eof(idx);
                        break;
                    }
                    (CrLfPassed, b'.') => {
                        split = BufSplit::Escape(idx);
                        break;
                    }

                    // If we can't do either of the above, just continue reading stuff
                    (_, _) => state = Running,
                }
            }
            self.state = state;

            // Then, look at the last 2 characters
            let l = self.buf.len();
            if split == BufSplit::Nowhere {
                if l >= 2 {
                    match (self.state, self.buf[l - 2], self.buf[l - 1]) {
                        // If we may be stopping the buffer somewhere in \r\n.\r\n
                        (CrLfPassed, b'.', b'\r') => split = BufSplit::Unknown(l - 2),
                        (CrPassed, b'\n', b'.') => {
                            self.state = CrLfPassed;
                            split = BufSplit::Unknown(l - 1);
                        }

                        // If there is a \r\n.
                        (CrLfPassed, b'.', _) => split = BufSplit::Escape(l - 2),

                        // Move forward in the \r\n state machine
                        (_, b'\r', b'\n') => self.state = CrLfPassed,
                        (_, _, b'\r') => self.state = CrPassed,

                        // Or just continue reading stuff
                        (_, _, _) => self.state = Running,
                    }
                } else if l == 1 {
                    match (self.state, self.buf[l - 1]) {
                        // If we may be stopping the buffer somewhere in \r\n.\r\n
                        (CrLfPassed, b'.') => split = BufSplit::Unknown(l - 1),

                        // Move forward in the \r\n state machine
                        (_, b'\r') => self.state = CrPassed,
                        (CrPassed, b'\n') => self.state = CrLfPassed,

                        // Or just continue reading stuff
                        (_, _) => self.state = Running,
                    }
                } // Ignore the case l == 0, as it wouldn't send anything anyway
            }

            // Send the buffer if we have something to send
            match split {
                BufSplit::Nowhere if self.buf.len() > 0 => return Ready(Some(self.buf.take())),
                BufSplit::Nowhere => (), // Continue to read more data if nothing to send
                BufSplit::Eof(x) => {
                    let res = self.buf.split_to(x);
                    self.buf.advance(3);
                    self.state = Finished;
                    if res.len() > 0 {
                        return Ready(Some(res));
                    } else {
                        return Ready(None);
                    }
                }
                BufSplit::Escape(x) => {
                    let res = self.buf.split_to(x);
                    self.buf.advance(1);
                    self.state = Running;
                    if res.len() > 0 {
                        return Ready(Some(res));
                    } else {
                        // Continue to read more data if nothing is to be sent before the escape
                        // point
                        continue;
                    }
                }
                BufSplit::Unknown(x) if x > 0 => return Ready(Some(self.buf.split_to(x))),
                BufSplit::Unknown(_) => (), // Continue to read more data if nothing to send
            }

            // Didn't find anything to send, so let's just gather more data from the network
            match self.source.as_mut().poll_next(ctxt) {
                Pending => return Pending,
                // If the stream ends there, it means that we received a FIN during the stream of
                // DATA. This is an error according to the specification, so returning an error.
                // Now, the receive end of the pipe isn't necessarily closed, so it may be a good
                // idea to send a message. However, RFC5321 doesn't appear to make this sort of
                // things possible, and both OpenSMTPD and gmail appear to just answer with closing
                // the stream in the other direction. So here we do, doing nothing in case of
                // unexpected connection closing.
                // However, we definitely want to not considered the DATA as having completed
                // correctly, so we do record that there was an early FIN.
                Ready(None) => {
                    self.state = DataStreamState::EarlyFin;
                    return Ready(None);
                }
                Ready(Some(b)) => self.buf.unsplit(b), // TODO: (B) optimize with `self.buf = b`?
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use futures::{executor::block_on, StreamExt};

    use crate::streamext::StreamExt as SmtpStreamExt;

    #[test]
    fn valid_data_stream() {
        let tests: &[(&[&[u8]], &[u8], &[u8])] = &[
            (
                &[b"foo", b" bar", b"\r\n", b".\r", b"\n"],
                b"foo bar\r\n",
                b"",
            ),
            (&[b"\r\n.\r\n", b"\r\n"], b"\r\n", b"\r\n"),
            (&[b".baz\r\n", b".\r\n", b"foo"], b"baz\r\n", b"foo"),
            (&[b" .baz", b"\r\n.", b"\r\nfoo"], b" .baz\r\n", b"foo"),
            (&[b".\r\n", b"MAIL FROM"], b"", b"MAIL FROM"),
            (&[b"..\r\n.\r\n"], b".\r\n", b""),
            (&[b"foo\r\n. ", b"bar\r\n.\r\n"], b"foo\r\n bar\r\n", b""),
        ];
        for &(inp, out, rem) in tests {
            use crate::smtpstring::SmtpString;
            println!(
                "\nTrying to parse {:?} into {:?} with {:?} remaining",
                inp.iter().map(|x| SmtpString::from(*x)).collect::<Vec<_>>(),
                SmtpString::from(out),
                SmtpString::from(rem),
            );
            let mut stream =
                DataStream::new(stream::iter(inp.iter().map(|x| BytesMut::from(*x))).prependable());
            let output = block_on(async {
                let mut res = BytesMut::new();
                while let Some(i) = await!(stream.by_ref().next()) {
                    res.unsplit(i);
                }
                res
            });
            println!("Now computing remaining stuff");
            let remaining = block_on(async {
                let mut res = BytesMut::new();
                let mut stream = stream.into_inner().unwrap();
                while let Some(i) = await!(stream.next()) {
                    res.unsplit(i);
                }
                res
            });
            println!(
                " -> Got {:?} with {:?} remaining",
                SmtpString::from(&output[..]),
                SmtpString::from(&remaining[..])
            );
            assert_eq!(output, BytesMut::from(out));
            assert_eq!(remaining, BytesMut::from(rem));
        }
    }

    quickcheck! {
        fn all_leading_dots_are_escaped(v: Vec<Vec<u8>>) -> bool {
            let mut v = v;
            v.extend_from_slice(&[vec![b'\r', b'\n', b'.', b'\r', b'\n']]);
            let r = block_on(async {
                let mut stream = DataStream::new(stream::iter(v.into_iter().map(BytesMut::from)).prependable());
                let mut res = BytesMut::new();
                while let Some(i) = await!(stream.next()) {
                    res.unsplit(i);
                }
                res
            });
            r.windows(5).position(|x| x == b"\r\n.\r\n").is_none()
        }
    }
}
