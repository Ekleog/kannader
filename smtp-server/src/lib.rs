extern crate smtp_message;
extern crate tokio;

use smtp_message::*;
use std::mem;
use tokio::prelude::*;

pub type MailAddress = Vec<u8>;
pub type MailAddressRef<'a> = &'a [u8];

pub struct ConnectionMetadata<U> {
    user: U,
}

pub struct MailMetadata {
    from: MailAddress,
    to:   Vec<Email>,
}

pub struct Refusal {
    code: ReplyCode,
    msg:  String,
}

pub enum Decision<T> {
    Accept(T),
    Reject(Refusal),
}

// The streams will be read 1-by-1, so make sure they are buffered
pub fn interact<
    'a,
    ReaderError,
    Reader: 'a + Stream<Item = u8, Error = ReaderError>,
    WriterError,
    Writer: Sink<SinkItem = u8, SinkError = WriterError>,
    UserProvidedMetadata: 'a,
    HandleReaderError: 'a + FnMut(ReaderError) -> (),
    HandleWriterError: 'a + FnMut(WriterError) -> (),
    State: 'a,
    FilterFrom: 'a + Fn(MailAddressRef, &ConnectionMetadata<UserProvidedMetadata>) -> Decision<State>,
    FilterTo: 'a
        + Fn(&Email, &mut State, &ConnectionMetadata<UserProvidedMetadata>, &MailMetadata)
            -> Decision<()>,
    HandleMail: 'a + Fn(MailMetadata, State, &ConnectionMetadata<UserProvidedMetadata>, ()) -> Decision<()>,
>(
    incoming: Reader,
    outgoing: &'a mut Writer,
    metadata: UserProvidedMetadata,
    handle_reader_error: HandleReaderError,
    handle_writer_error: HandleWriterError,
    filter_from: &'a FilterFrom,
    filter_to: &'a FilterTo,
    handler: &'a HandleMail,
) -> Box<'a + Future<Item = (), Error = ()>> {
    // TODO: return `impl Future`
    let conn_meta = ConnectionMetadata { user: metadata };
    let writer = outgoing
        .sink_map_err(handle_writer_error)
        .with_flat_map(|c: Reply| {
            // TODO: actually make smtp-message's send_to work with sinks
            let mut v = Vec::new();
            c.send_to(&mut v).unwrap(); // and this is ugly
            stream::iter_ok(v)
        });
    Box::new(
        CrlfLines::new(incoming.map_err(handle_reader_error))
            .into_future()
            .map_err(|((), _)| ()) // Ignore the stream returned on error by into_future
            .and_then(move |x| {
                handle_lines(
                    x,
                    (writer, conn_meta, None),
                    filter_from,
                    filter_to,
                    handler,
                )
            }),
    )
}

fn handle_lines<
    'a,
    U: 'a,
    Writer: 'a + Sink<SinkItem = Reply, SinkError = ()>,
    Reader: 'a + Stream<Item = u8, Error = ()>,
    State: 'a,
    FilterFrom: 'a + Fn(MailAddressRef, &ConnectionMetadata<U>) -> Decision<State>,
    FilterTo: 'a + Fn(&Email, &mut State, &ConnectionMetadata<U>, &MailMetadata) -> Decision<()>,
    HandleMail: 'a + Fn(MailMetadata, State, &ConnectionMetadata<U>, ()) -> Decision<()>,
>(
    (line, reader): (Option<Vec<u8>>, CrlfLines<Reader>),
    add_data: (Writer, ConnectionMetadata<U>, Option<(MailMetadata, State)>),
    filter_from: &'a FilterFrom,
    filter_to: &'a FilterTo,
    handler: &'a HandleMail,
) -> Box<'a + Future<Item = (), Error = ()>> {
    if let Some(line) = line {
        // TODO: remove this allocation
        Box::new(
            reader
            .into_future()
            .map_err(|((), _)| ()) // Ignore the stream returned on error
            .join(handle_line(add_data, line, filter_from, filter_to, handler))
            .and_then(move |(read, add_data)| {
                handle_lines(read, add_data, filter_from, filter_to, handler)
            }),
        )
    } else {
        // TODO: warn of unfinished commands?
        Box::new(future::ok(()))
    }
}

fn handle_line<
    'a,
    U: 'a,
    W: 'a + Sink<SinkItem = Reply, SinkError = ()>,
    State: 'a,
    FilterFrom: 'a + Fn(MailAddressRef, &ConnectionMetadata<U>) -> Decision<State>,
    FilterTo: 'a + Fn(&Email, &mut State, &ConnectionMetadata<U>, &MailMetadata) -> Decision<()>,
    HandleMail: 'a + Fn(MailMetadata, State, &ConnectionMetadata<U>, ()) -> Decision<()>,
>(
    (writer, conn_meta, mail_data): (W, ConnectionMetadata<U>, Option<(MailMetadata, State)>),
    line: Vec<u8>,
    filter_from: &FilterFrom,
    filter_to: &FilterTo,
    handler: &HandleMail,
) -> Box<'a + Future<Item = (W, ConnectionMetadata<U>, Option<(MailMetadata, State)>), Error = ()>>
where
    W::SinkError: 'a,
{
    let cmd = Command::parse(&line);
    match cmd {
        Ok(Command::Mail(m)) => {
            if mail_data.is_some() {
                // TODO: make the message configurable
                Box::new(
                    send_reply(
                        writer,
                        ReplyCode::BAD_SEQUENCE,
                        SmtpString::copy_bytes(b"Bad sequence of commands"),
                    ).and_then(|writer| future::ok((writer, conn_meta, mail_data))),
                ) as Box<Future<Item = _, Error = _>>
            } else {
                match filter_from(m.raw_from(), &conn_meta) {
                    Decision::Accept(state) => {
                        let from = m.raw_from().to_vec();
                        let to = Vec::new();
                        // TODO: make this "Okay" configurable
                        Box::new(
                            send_reply(writer, ReplyCode::OKAY, SmtpString::copy_bytes(b"Okay"))
                                .and_then(|writer| {
                                    future::ok((
                                        writer,
                                        conn_meta,
                                        Some((MailMetadata { from, to }, state)),
                                    ))
                                }),
                        ) as Box<Future<Item = _, Error = _>>
                    }
                    Decision::Reject(r) => Box::new(
                        send_reply(writer, r.code, SmtpString::from_bytes(r.msg.into_bytes()))
                            .and_then(|writer| future::ok((writer, conn_meta, mail_data))),
                    ),
                }
            }
        }
        Ok(Command::Rcpt(r)) => {
            if let Some((mail_meta, mut state)) = mail_data {
                match filter_to(r.to(), &mut state, &conn_meta, &mail_meta) {
                    Decision::Accept(()) => {
                        let MailMetadata { from, mut to } = mail_meta;
                        to.push(r.to().clone());
                        // TODO: make this "Okay" configurable
                        Box::new(
                            send_reply(writer, ReplyCode::OKAY, SmtpString::copy_bytes(b"Okay"))
                                .and_then(|writer| {
                                    future::ok((
                                        writer,
                                        conn_meta,
                                        Some((MailMetadata { from, to }, state)),
                                    ))
                                }),
                        )
                    }
                    Decision::Reject(r) => Box::new(
                        send_reply(writer, r.code, SmtpString::from_bytes(r.msg.into_bytes()))
                            .and_then(|writer| {
                                future::ok((writer, conn_meta, Some((mail_meta, state))))
                            }),
                    ),
                }
            } else {
                // TODO: make the message configurable
                Box::new(
                    send_reply(
                        writer,
                        ReplyCode::BAD_SEQUENCE,
                        SmtpString::copy_bytes(b"Bad sequence of commands"),
                    ).and_then(|writer| future::ok((writer, conn_meta, mail_data))),
                )
            }
        }
        Ok(_) => Box::new(
            // TODO: look for a way to eliminate this alloc
            // TODO: make the message configurable
            send_reply(
                writer,
                ReplyCode::COMMAND_UNIMPLEMENTED,
                SmtpString::copy_bytes(b"Command not implemented"),
            ).and_then(|writer| future::ok((writer, conn_meta, mail_data))),
        ) as Box<Future<Item = _, Error = _>>,
        Err(_) => Box::new(
            // TODO: make the message configurable
            send_reply(
                writer,
                ReplyCode::COMMAND_UNRECOGNIZED,
                SmtpString::copy_bytes(b"Command not recognized"),
            ).and_then(|writer| future::ok((writer, conn_meta, mail_data))),
        ) as Box<Future<Item = _, Error = _>>,
    }
}

// Panics if `text` has a byte not in {9} \union [32; 126]
fn send_reply<'a, W>(
    writer: W,
    code: ReplyCode,
    text: SmtpString,
) -> Box<'a + Future<Item = W, Error = W::SinkError>>
where
    W: 'a + Sink<SinkItem = Reply>,
    W::SinkError: 'a,
{
    // TODO: figure out a way using fewer copies
    let replies = map_is_last(text.copy_chunks(Reply::MAX_LEN).into_iter(), move |t, l| {
        Reply::build(code, if l { IsLastLine::Yes } else { IsLastLine::No }, t).unwrap()
    });
    Box::new(writer.send_all(stream::iter_ok(replies)).map(|(w, _)| w))
}

// TODO: maybe it'd be possible to use upstream buffers instead of re-buffering
// here, for fewer copies
struct CrlfLines<S> {
    source:   S,
    cur_line: Vec<u8>,
}

impl<S: Stream<Item = u8>> CrlfLines<S> {
    pub fn new(s: S) -> CrlfLines<S> {
        CrlfLines {
            source:   s,
            cur_line: Self::initial_cur_line(),
        }
    }

    // Panics if a line was currently being read
    pub fn into_inner(self) -> S {
        assert!(self.cur_line.is_empty());
        self.source
    }

    fn initial_cur_line() -> Vec<u8> {
        Vec::with_capacity(1024)
    }

    fn next_line(&mut self) -> Vec<u8> {
        mem::replace(&mut self.cur_line, Self::initial_cur_line())
    }
}

impl<S: Stream<Item = u8>> Stream for CrlfLines<S> {
    type Item = Vec<u8>;
    type Error = S::Error;

    fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
        use Async::*;
        loop {
            match self.source.poll()? {
                NotReady => return Ok(NotReady),
                Ready(None) if self.cur_line.is_empty() => return Ok(Ready(None)),
                Ready(None) => return Ok(Ready(Some(self.next_line()))),
                Ready(Some(c)) => {
                    // TODO: implement line length limits
                    self.cur_line.push(c);
                    let l = self.cur_line.len();
                    if c == b'\n' && l >= 2 && self.cur_line[l - 2] == b'\r' {
                        return Ok(Ready(Some(self.next_line())));
                    }
                }
            }
        }
    }
}

struct MapIsLast<I: Iterator, F> {
    iter: std::iter::Peekable<I>,
    f:    F,
}

impl<R, I: Iterator, F: FnMut(I::Item, bool) -> R> Iterator for MapIsLast<I, F> {
    type Item = R;

    #[inline]
    fn next(&mut self) -> Option<R> {
        let res = self.iter.next();
        let is_last = self.iter.peek().is_none();
        res.map(|x| (self.f)(x, is_last))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

fn map_is_last<R, I: Iterator, F: FnMut(I::Item, bool) -> R>(iter: I, f: F) -> MapIsLast<I, F> {
    MapIsLast {
        iter: iter.peekable(),
        f,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crlflines_looks_good() {
        let stream = CrlfLines::new(
            stream::iter_ok(
                b"MAIL FROM:<foo@bar.example.org>\r\n\
                  RCPT TO:<baz@quux.example.org>\r\n\
                  RCPT TO:<foo2@bar.example.org>\r\n\
                  DATA\r\n\
                  Hello World\r\n\
                  .\r\n\
                  QUIT\r\n"
                    .iter()
                    .cloned(),
            ).map_err(|()| ()),
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

    #[test]
    fn interacts_ok() {
        let tests: &[(&[u8], &[u8])] = &[
            (
                b"MAIL FROM:<foo@bar.example.org>\r\n\
                  RCPT TO:<baz@quux.example.org>\r\n\
                  RCPT TO:<foo2@bar.example.org>\r\n\
                  RCPT TO:<foo3@bar.example.org>\r\n\
                  DATA\r\n\
                  Hello World\r\n\
                  .\r\n\
                  QUIT\r\n",
                b"250 Okay\r\n\
                  550 No user 'baz'\r\n\
                  250 Okay\r\n\
                  250 Okay\r\n\
                  502 Command not implemented\r\n\
                  500 Command not recognized\r\n\
                  500 Command not recognized\r\n\
                  502 Command not implemented\r\n",
            ),
            (b"HELP hello\r\n", b"502 Command not implemented\r\n"),
            (
                b"MAIL FROM:<bad@quux.example.org>\r\n\
                  MAIL FROM:<foo@bar.example.org>\r\n\
                  MAIL FROM:<baz@quux.example.org>\r\n\
                  RCPT TO:<foo2@bar.example.org>\r\n\
                  DATA\r\n\
                  Hello\r\n
                  .\r\n\
                  QUIT\r\n",
                b"550 User 'bad' banned\r\n\
                  250 Okay\r\n\
                  503 Bad sequence of commands\r\n\
                  250 Okay\r\n\
                  502 Command not implemented\r\n\
                  500 Command not recognized\r\n\
                  500 Command not recognized\r\n\
                  502 Command not implemented\r\n",
            ),
        ];
        for &(inp, out) in tests {
            let mut vec = Vec::new();
            let stream = stream::iter_ok(inp.iter().cloned());
            let mut filter_from = |addr: MailAddressRef, _: &ConnectionMetadata<()>| {
                if addr == b"bad@quux.example.org" {
                    Decision::Reject(Refusal {
                        code: ReplyCode::POLICY_REASON,
                        msg:  "User 'bad' banned".to_owned(),
                    })
                } else {
                    Decision::Accept(())
                }
            };
            let mut filter_to =
                |email: &Email, _: &mut (), _: &ConnectionMetadata<()>, _: &MailMetadata| {
                    if email.localpart().as_bytes() == b"baz" {
                        Decision::Reject(Refusal {
                            code: ReplyCode::MAILBOX_UNAVAILABLE,
                            msg:  "No user 'baz'".to_owned(),
                        })
                    } else {
                        Decision::Accept(())
                    }
                };
            let mut handler = |_: MailMetadata, (), _: &ConnectionMetadata<()>, _: ()| {
                Decision::Reject(Refusal {
                    code: ReplyCode::POLICY_REASON,
                    msg:  "foo".to_owned(),
                })
            };
            interact(
                stream,
                &mut vec,
                (),
                |()| (),
                |()| (),
                &mut filter_from,
                &mut filter_to,
                &mut handler,
            ).wait()
                .unwrap();
            assert_eq!(vec, out);
        }
    }
}
