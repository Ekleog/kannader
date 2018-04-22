// TODO: add in deadlines
// TODO: refactor in multiple files
extern crate itertools;
extern crate smtp_message;
extern crate tokio;

mod helpers;

use itertools::Itertools;
use smtp_message::*;
use std::mem;
use tokio::prelude::*;

use helpers::*;

pub struct ConnectionMetadata<U> {
    pub user: U,
}

pub struct MailMetadata<'a> {
    from: Option<Email<'a>>,
    to:   Vec<Email<'a>>,
}

pub struct Refusal {
    code: ReplyCode,
    msg:  String, // TODO: drop in favor of SmtpString
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
    FilterFrom: 'a + Fn(&Option<Email>, &ConnectionMetadata<UserProvidedMetadata>) -> Decision<State>,
    FilterTo: 'a
        + Fn(&Email, &mut State, &ConnectionMetadata<UserProvidedMetadata>, &MailMetadata)
            -> Decision<()>,
    HandleMail: 'a
        + Fn(
            MailMetadata<'static>,
            State,
            &ConnectionMetadata<UserProvidedMetadata>,
            DataStream<stream::MapErr<Reader, HandleReaderError>>,
        ) -> (stream::MapErr<Reader, HandleReaderError>, Decision<()>),
>(
    incoming: Reader,
    outgoing: &'a mut Writer,
    metadata: UserProvidedMetadata,
    handle_reader_error: HandleReaderError,
    handle_writer_error: HandleWriterError,
    filter_from: &'a FilterFrom,
    filter_to: &'a FilterTo,
    handler: &'a HandleMail,
) -> impl Future<Item = (), Error = ()> + 'a {
    let conn_meta = ConnectionMetadata { user: metadata };
    let writer = outgoing
        .sink_map_err(handle_writer_error)
        .with_flat_map(|c: Reply| {
            // TODO: actually make smtp-message's send_to work with sinks
            let mut v = Vec::new();
            c.send_to(&mut v).unwrap(); // and this is ugly
            stream::iter_ok(v)
        });
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
        })
}

fn handle_lines<
    'a,
    U: 'a,
    Writer: 'a + Sink<SinkItem = Reply<'a>, SinkError = ()>,
    Reader: 'a + Stream<Item = u8, Error = ()>,
    State: 'a,
    FilterFrom: 'a + Fn(&Option<Email>, &ConnectionMetadata<U>) -> Decision<State>,
    FilterTo: 'a + Fn(&Email, &mut State, &ConnectionMetadata<U>, &MailMetadata) -> Decision<()>,
    HandleMail: 'a
        + Fn(MailMetadata<'static>, State, &ConnectionMetadata<U>, DataStream<Reader>)
            -> (Reader, Decision<()>),
>(
    (line, reader): (Option<Vec<u8>>, CrlfLines<Reader>),
    add_data: (
        Writer,
        ConnectionMetadata<U>,
        Option<(MailMetadata<'static>, State)>,
    ),
    filter_from: &'a FilterFrom,
    filter_to: &'a FilterTo,
    handler: &'a HandleMail,
) -> impl Future<Item = (), Error = ()> + 'a {
    if let Some(line) = line {
        // Cannot do without this allocation here, as the type of the thing inside the
        // box depends on the returned type of handle_lines
        Box::new(
            handle_line(
                reader.into_inner(),
                add_data,
                line,
                filter_from,
                filter_to,
                handler,
            ).and_then(|(reader, add_data)| {
                CrlfLines::new(reader)
                    .into_future()
                    .map_err(|((), _)| ()) // Discard the stream returned with errors
                    .map(|read| (read, add_data))
            })
                .and_then(move |(read, add_data)| {
                    handle_lines(read, add_data, filter_from, filter_to, handler)
                }),
        ) as Box<Future<Item = (), Error = ()>>
    } else {
        // TODO: warn of unfinished commands?
        Box::new(future::ok(()))
    }
}

fn handle_line<
    'a,
    U: 'a,
    Writer: 'a + Sink<SinkItem = Reply<'a>, SinkError = ()>,
    Reader: 'a + Stream<Item = u8, Error = ()>,
    State: 'a,
    FilterFrom: 'a + Fn(&Option<Email>, &ConnectionMetadata<U>) -> Decision<State>,
    FilterTo: 'a + Fn(&Email, &mut State, &ConnectionMetadata<U>, &MailMetadata) -> Decision<()>,
    HandleMail: 'a
        + Fn(MailMetadata<'static>, State, &ConnectionMetadata<U>, DataStream<Reader>)
            -> (Reader, Decision<()>),
>(
    reader: Reader,
    (writer, conn_meta, mail_data): (
        Writer,
        ConnectionMetadata<U>,
        Option<(MailMetadata<'static>, State)>,
    ),
    line: Vec<u8>,
    filter_from: &FilterFrom,
    filter_to: &FilterTo,
    handler: &HandleMail,
) -> impl Future<
    Item = (
        Reader,
        (
            Writer,
            ConnectionMetadata<U>,
            Option<(MailMetadata<'static>, State)>,
        ),
    ),
    Error = (),
>
         + 'a {
    // TODO: is this take_ownership actually required?
    let cmd = Command::parse(&line).map(|x| x.take_ownership());
    match cmd {
        Ok(Command::Mail(m)) => {
            if mail_data.is_some() {
                // TODO: make the message configurable
                FutIn12::Fut1(
                    send_reply(
                        writer,
                        ReplyCode::BAD_SEQUENCE,
                        (&b"Bad sequence of commands"[..]).into(),
                    ).and_then(|writer| future::ok((reader, (writer, conn_meta, mail_data)))),
                )
            } else {
                match filter_from(m.from(), &conn_meta) {
                    Decision::Accept(state) => {
                        let from = m.into_from();
                        let to = Vec::new();
                        // TODO: make this "Okay" configurable
                        FutIn12::Fut2(
                            send_reply(writer, ReplyCode::OKAY, (&b"Okay"[..]).into()).and_then(
                                |writer| {
                                    future::ok((
                                        reader,
                                        (
                                            writer,
                                            conn_meta,
                                            Some((MailMetadata { from, to }, state)),
                                        ),
                                    ))
                                },
                            ),
                        )
                    }
                    Decision::Reject(r) => {
                        FutIn12::Fut3(send_reply(writer, r.code, r.msg.into()).and_then(|writer| {
                            future::ok((reader, (writer, conn_meta, mail_data)))
                        }))
                    }
                }
            }
        }
        Ok(Command::Rcpt(r)) => {
            if let Some((mail_meta, mut state)) = mail_data {
                match filter_to(r.to(), &mut state, &conn_meta, &mail_meta) {
                    Decision::Accept(()) => {
                        let MailMetadata { from, mut to } = mail_meta;
                        to.push(r.into_to());
                        // TODO: make this "Okay" configurable
                        FutIn12::Fut4(
                            send_reply(writer, ReplyCode::OKAY, (&b"Okay"[..]).into()).and_then(
                                |writer| {
                                    future::ok((
                                        reader,
                                        (
                                            writer,
                                            conn_meta,
                                            Some((MailMetadata { from, to }, state)),
                                        ),
                                    ))
                                },
                            ),
                        )
                    }
                    Decision::Reject(r) => {
                        FutIn12::Fut5(send_reply(writer, r.code, r.msg.into()).and_then(|writer| {
                            future::ok((reader, (writer, conn_meta, Some((mail_meta, state)))))
                        }))
                    }
                }
            } else {
                // TODO: make the message configurable
                FutIn12::Fut6(
                    send_reply(
                        writer,
                        ReplyCode::BAD_SEQUENCE,
                        (&b"Bad sequence of commands"[..]).into(),
                    ).and_then(|writer| future::ok((reader, (writer, conn_meta, mail_data)))),
                )
            }
        }
        Ok(Command::Data(_)) => {
            if let Some((mail_meta, state)) = mail_data {
                if !mail_meta.to.is_empty() {
                    match handler(mail_meta, state, &conn_meta, DataStream::new(reader)) {
                        (reader, Decision::Accept(())) => FutIn12::Fut7(
                            send_reply(writer, ReplyCode::OKAY, (&b"Okay"[..]).into())
                                .and_then(|writer| future::ok((reader, (writer, conn_meta, None)))),
                        ),
                        (reader, Decision::Reject(r)) => FutIn12::Fut8(
                            send_reply(writer, r.code, r.msg.into()).and_then(|writer| {
                                // Other mail systems (at least postfix, OpenSMTPD and gmail)
                                // appear to drop the state on an unsuccessful DATA command
                                // (eg. too long). Couldn't find the RFC reference anywhere,
                                // though.
                                future::ok((reader, (writer, conn_meta, None)))
                            }),
                        ),
                    }
                } else {
                    FutIn12::Fut9(
                        send_reply(
                            writer,
                            ReplyCode::BAD_SEQUENCE,
                            (&b"Bad sequence of commands"[..]).into(),
                        ).and_then(|writer| {
                            future::ok((reader, (writer, conn_meta, Some((mail_meta, state)))))
                        }),
                    )
                }
            } else {
                FutIn12::Fut10(
                    send_reply(
                        writer,
                        ReplyCode::BAD_SEQUENCE,
                        (&b"Bad sequence of commands"[..]).into(),
                    ).and_then(|writer| future::ok((reader, (writer, conn_meta, mail_data)))),
                )
            }
        }
        // TODO: this case should just no longer be needed
        Ok(_) => FutIn12::Fut11(
            // TODO: make the message configurable
            send_reply(
                writer,
                ReplyCode::COMMAND_UNIMPLEMENTED,
                (&b"Command not implemented"[..]).into(),
            ).and_then(|writer| future::ok((reader, (writer, conn_meta, mail_data)))),
        ),
        Err(_) => FutIn12::Fut12(
            // TODO: make the message configurable
            send_reply(
                writer,
                ReplyCode::COMMAND_UNRECOGNIZED,
                (&b"Command not recognized"[..]).into(),
            ).and_then(|writer| future::ok((reader, (writer, conn_meta, mail_data)))),
        ),
    }
}

// Panics if `text` has a byte not in {9} \union [32; 126]
fn send_reply<'a, W>(
    writer: W,
    code: ReplyCode,
    text: SmtpString,
) -> impl Future<Item = W, Error = W::SinkError> + 'a
where
    W: 'a + Sink<SinkItem = Reply<'a>>,
    W::SinkError: 'a,
{
    // TODO: figure out a way using fewer copies
    let replies = text.copy_chunks(Reply::MAX_LEN)
        .into_iter()
        .with_position()
        .map(move |t| {
            use itertools::Position::*;
            match t {
                First(t) | Middle(t) => Reply::build(code, IsLastLine::No, t).unwrap(),
                Last(t) | Only(t) => Reply::build(code, IsLastLine::Yes, t).unwrap(),
            }
        });
    writer.send_all(stream::iter_ok(replies)).map(|(w, _)| w)
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

#[cfg(test)]
mod tests {
    use super::*;

    use std::cell::Cell;

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

    fn filter_from(addr: &Option<Email>, _: &ConnectionMetadata<()>) -> Decision<()> {
        if opt_email_repr(addr) == (&b"bad@quux.example.org"[..]).into() {
            Decision::Reject(Refusal {
                code: ReplyCode::POLICY_REASON,
                msg:  "User 'bad' banned".to_owned(),
            })
        } else {
            Decision::Accept(())
        }
    }

    fn filter_to(
        email: &Email,
        _: &mut (),
        _: &ConnectionMetadata<()>,
        _: &MailMetadata,
    ) -> Decision<()> {
        if email.localpart().as_bytes() == b"baz" {
            Decision::Reject(Refusal {
                code: ReplyCode::MAILBOX_UNAVAILABLE,
                msg:  "No user 'baz'".to_owned(),
            })
        } else {
            Decision::Accept(())
        }
    }

    fn handler<R: Stream<Item = u8>>(
        meta: MailMetadata<'static>,
        (): (),
        _: &ConnectionMetadata<()>,
        mut reader: DataStream<R>,
        mails: &Cell<Vec<(Option<Email>, Vec<Email>, Vec<u8>)>>,
    ) -> (R, Decision<()>) {
        // TODO: this API should be asynchronous!!!!!
        let mail_text = reader.by_ref().collect().wait().map_err(|_| ()).unwrap();
        if mail_text.windows(5).position(|x| x == b"World").is_some() {
            (
                // TODO: rename `continue` and panic instead of auto-consuming the remaining stuff
                reader.consume_and_continue(),
                Decision::Reject(Refusal {
                    code: ReplyCode::POLICY_REASON,
                    msg:  "Don't you dare say 'World'!".to_owned(),
                }),
            )
        } else {
            let mut m = mails.take();
            m.push((meta.from, meta.to, mail_text));
            mails.set(m);
            (reader.consume_and_continue(), Decision::Accept(()))
        }
    }

    #[test]
    fn interacts_ok() {
        let tests: &[(&[u8], &[u8], &[(Option<&[u8]>, &[&[u8]], &[u8])])] = &[
            // TODO: send banner before EHLO
            // TODO: send please go on after DATA
            (
                b"MAIL FROM:<>\r\n\
                  RCPT TO:<baz@quux.example.org>\r\n\
                  RCPT TO:<foo2@bar.example.org>\r\n\
                  RCPT TO:<foo3@bar.example.org>\r\n\
                  DATA\r\n\
                  Hello world\r\n\
                  .\r\n\
                  QUIT\r\n",
                b"250 Okay\r\n\
                  550 No user 'baz'\r\n\
                  250 Okay\r\n\
                  250 Okay\r\n\
                  250 Okay\r\n\
                  502 Command not implemented\r\n",
                &[(
                    None,
                    &[b"foo2@bar.example.org", b"foo3@bar.example.org"],
                    b"Hello world\r\n",
                )],
            ),
            (
                b"MAIL FROM:<test@example.org>\r\n\
                  RCPT TO:<foo@example.org>\r\n\
                  DATA\r\n\
                  Hello World\r\n\
                  .\r\n\
                  QUIT\r\n",
                b"250 Okay\r\n\
                  250 Okay\r\n\
                  550 Don't you dare say 'World'!\r\n\
                  502 Command not implemented\r\n",
                &[],
            ),
            (b"HELP hello\r\n", b"502 Command not implemented\r\n", &[]),
            (
                b"MAIL FROM:<bad@quux.example.org>\r\n\
                  MAIL FROM:<foo@bar.example.org>\r\n\
                  MAIL FROM:<baz@quux.example.org>\r\n\
                  RCPT TO:<foo2@bar.example.org>\r\n\
                  DATA\r\n\
                  Hello\r\n\
                  .\r\n\
                  QUIT\r\n",
                b"550 User 'bad' banned\r\n\
                  250 Okay\r\n\
                  503 Bad sequence of commands\r\n\
                  250 Okay\r\n\
                  250 Okay\r\n\
                  502 Command not implemented\r\n",
                &[(
                    Some(b"foo@bar.example.org"),
                    &[b"foo2@bar.example.org"],
                    b"Hello\r\n",
                )],
            ),
            (
                b"MAIL FROM:<foo@test.example.com>\r\n\
                  DATA\r\n\
                  QUIT\r\n",
                b"250 Okay\r\n\
                  503 Bad sequence of commands\r\n\
                  502 Command not implemented\r\n",
                &[],
            ),
        ];
        for &(inp, out, mail) in tests {
            println!("\nSending\n---\n{}---", std::str::from_utf8(inp).unwrap());
            let stream = stream::iter_ok(inp.iter().cloned());
            let mut resp = Vec::new();
            let mut resp_mail = Cell::new(Vec::new());
            let handler_closure = |a, b, c: &_, d| handler(a, b, c, d, &resp_mail);
            interact(
                stream,
                &mut resp,
                (),
                |()| (),
                |()| (),
                &filter_from,
                &filter_to,
                &handler_closure,
            ).wait()
                .unwrap();
            println!("Expecting\n---\n{}---", std::str::from_utf8(out).unwrap());
            println!("Got\n---\n{}---", std::str::from_utf8(&resp).unwrap());
            assert_eq!(resp, out);
            println!("Checking mails:");
            let resp_mail = resp_mail.take();
            assert_eq!(resp_mail.len(), mail.len());
            for ((fr, tr, cr), &(fo, to, co)) in resp_mail.into_iter().zip(mail) {
                println!("Mail\n---");
                let fo = fo.map(SmtpString::from);
                let fr = fr.map(|x| x.as_smtp_string());
                println!("From: expected {:?}, got {:?}", fo, fr);
                assert_eq!(fo, fr);
                let to_smtp = to.iter().map(|x| SmtpString::from(*x)).collect::<Vec<_>>();
                let tr_smtp = tr.into_iter()
                    .map(|x| x.into_smtp_string())
                    .collect::<Vec<_>>();
                println!("To: expected {:?}, got {:?}", to_smtp, tr_smtp);
                assert_eq!(to_smtp, tr_smtp);
                println!("Expected text\n--\n{}--", std::str::from_utf8(co).unwrap());
                println!("Got text\n--\n{}--", std::str::from_utf8(&cr).unwrap());
                assert_eq!(co, &cr[..]);
            }
        }
    }
}
