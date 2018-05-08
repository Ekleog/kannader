use bytes::{BufMut, Bytes, BytesMut};
use smtp_message::{
    Command, DataStream, MailCommand, Prependable, RcptCommand, ReplyLine, StreamExt,
};
use tokio::prelude::*;

use config::Config;
use crlflines::CrlfLines;
use decision::Decision;
use metadata::{ConnectionMetadata, MailMetadata};
use sendreply::send_reply;
use stupidfut::FutIn11;

// TODO: try removing as much lifetimes as possible from the whole mess

// TODO: remove Handle{Reader,Writer}Error that just make no sense (the user
// could just as well map before passing us Reader and Writer)
pub fn interact<
    'a,
    ReaderError,
    Reader: 'a + Stream<Item = BytesMut, Error = ReaderError>,
    WriterError,
    Writer: Sink<SinkItem = Bytes, SinkError = WriterError>,
    UserProvidedMetadata: 'a,
    HandleReaderError: 'a + FnMut(ReaderError) -> (),
    HandleWriterError: 'a + FnMut(WriterError) -> (),
    Cfg: 'a + Config<UserProvidedMetadata>,
>(
    incoming: Reader,
    outgoing: &'a mut Writer,
    metadata: UserProvidedMetadata,
    handle_reader_error: HandleReaderError,
    handle_writer_error: HandleWriterError,
    cfg: &'a mut Cfg,
) -> impl Future<Item = (), Error = ()> + 'a {
    let conn_meta = ConnectionMetadata { user: metadata };
    let writer = outgoing
        .sink_map_err(handle_writer_error)
        .with(|c: ReplyLine| {
            let mut w = BytesMut::with_capacity(c.byte_len()).writer();
            c.send_to(&mut w).unwrap();
            // By design of BytesMut::writer, this cannot fail so long as the buffer
            // has sufficient capacity. As if this is not respected it is a clear
            // programming error, there's no need to try and handle this cleanly.
            future::ok(w.into_inner().freeze())
        });
    CrlfLines::new(incoming.map_err(handle_reader_error).prependable())
        .fold_with_stream((cfg, writer, conn_meta, None), move |acc, line, reader| {
            handle_line(reader.into_inner(), acc, line).and_then(|(reader, acc)| {
                future::result(reader.ok_or(()).map(|read| (CrlfLines::new(read), acc)))
            })
        })
        .map(|_acc| ()) // TODO(low): warn of unfinished commands?
}

fn handle_line<
    'a,
    U: 'a,
    Writer: 'a + Sink<SinkItem = ReplyLine, SinkError = ()>,
    Reader: 'a + Stream<Item = BytesMut, Error = ()>,
    Cfg: 'a + Config<U>,
>(
    reader: Prependable<Reader>,
    (cfg, writer, conn_meta, mail_data): (
        &'a mut Cfg,
        Writer,
        ConnectionMetadata<U>,
        Option<MailMetadata>,
    ),
    line: BytesMut,
) -> impl Future<
    Item = (
        Option<Prependable<Reader>>,
        (
            &'a mut Cfg,
            Writer,
            ConnectionMetadata<U>,
            Option<MailMetadata>,
        ),
    ),
    Error = (),
>
         + 'a {
    let cmd = Command::parse(line.freeze());
    match cmd {
        Ok(Command::Mail(MailCommand {
            from,
            params: _params,
        })) => {
            if mail_data.is_some() {
                FutIn11::Fut1(
                    send_reply(writer, cfg.already_in_mail()).and_then(|writer| {
                        future::ok((Some(reader), (cfg, writer, conn_meta, mail_data)))
                    }),
                )
            } else {
                match cfg.filter_from(&from, &conn_meta) {
                    Decision::Accept => {
                        let to = Vec::new();
                        FutIn11::Fut2(send_reply(writer, cfg.mail_okay()).and_then(|writer| {
                            future::ok((
                                Some(reader),
                                (cfg, writer, conn_meta, Some(MailMetadata { from, to })),
                            ))
                        }))
                    }
                    Decision::Reject(r) => FutIn11::Fut3(
                        send_reply(writer, (r.code, r.msg.into())).and_then(|writer| {
                            future::ok((Some(reader), (cfg, writer, conn_meta, mail_data)))
                        }),
                    ),
                }
            }
        }
        Ok(Command::Rcpt(RcptCommand { to: rcpt_to })) => {
            if let Some(mail_meta) = mail_data {
                match cfg.filter_to(&rcpt_to, &mail_meta, &conn_meta) {
                    Decision::Accept => {
                        let MailMetadata { from, mut to } = mail_meta;
                        to.push(rcpt_to);
                        FutIn11::Fut4(send_reply(writer, cfg.rcpt_okay()).and_then(|writer| {
                            future::ok((
                                Some(reader),
                                (cfg, writer, conn_meta, Some(MailMetadata { from, to })),
                            ))
                        }))
                    }
                    Decision::Reject(r) => {
                        FutIn11::Fut5(send_reply(writer, (r.code, r.msg)).and_then(|writer| {
                            future::ok((Some(reader), (cfg, writer, conn_meta, Some(mail_meta))))
                        }))
                    }
                }
            } else {
                FutIn11::Fut6(
                    send_reply(writer, cfg.rcpt_before_mail()).and_then(|writer| {
                        future::ok((Some(reader), (cfg, writer, conn_meta, mail_data)))
                    }),
                )
            }
        }
        Ok(Command::Data(_)) => {
            if let Some(mail_meta) = mail_data {
                if !mail_meta.to.is_empty() {
                    FutIn11::Fut7(
                        cfg.handle_mail(DataStream::new(reader), mail_meta, &conn_meta)
                            .and_then(|(cfg, reader, decision)| match decision {
                                Decision::Accept => future::Either::A(
                                    send_reply(writer, cfg.mail_accepted()).and_then(|writer| {
                                        future::ok((reader, (cfg, writer, conn_meta, None)))
                                    }),
                                ),
                                Decision::Reject(r) => future::Either::B(
                                    send_reply(writer, (r.code, r.msg.into())).and_then(|writer| {
                                        // Other mail systems (at least postfix, OpenSMTPD and
                                        // gmail) appear to drop the state on an unsuccessful
                                        // DATA command (eg. too long). Couldn't find the RFC
                                        // reference anywhere, though.
                                        future::ok((reader, (cfg, writer, conn_meta, None)))
                                    }),
                                ),
                            }),
                    )
                } else {
                    FutIn11::Fut8(
                        send_reply(writer, cfg.data_before_rcpt()).and_then(|writer| {
                            future::ok((Some(reader), (cfg, writer, conn_meta, Some(mail_meta))))
                        }),
                    )
                }
            } else {
                FutIn11::Fut9(
                    send_reply(writer, cfg.data_before_mail()).and_then(|writer| {
                        future::ok((Some(reader), (cfg, writer, conn_meta, mail_data)))
                    }),
                )
            }
        }
        // TODO(low): this case should just no longer be needed
        Ok(_) => FutIn11::Fut10(
            send_reply(writer, cfg.command_unimplemented())
                .and_then(|writer| future::ok((Some(reader), (cfg, writer, conn_meta, mail_data)))),
        ),
        Err(_) => FutIn11::Fut11(
            send_reply(writer, cfg.command_unrecognized())
                .and_then(|writer| future::ok((Some(reader), (cfg, writer, conn_meta, mail_data)))),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use itertools::Itertools;
    use smtp_message::{opt_email_repr, Email, ReplyCode, SmtpString};
    use std;

    use decision::Refusal;

    struct TestConfig {
        mails: Vec<(Option<Email>, Vec<Email>, BytesMut)>,
    }

    impl Config<()> for TestConfig {
        fn filter_from(&mut self, addr: &Option<Email>, _: &ConnectionMetadata<()>) -> Decision {
            if opt_email_repr(addr) == (&b"bad@quux.example.org"[..]).into() {
                Decision::Reject(Refusal {
                    code: ReplyCode::POLICY_REASON,
                    msg:  "User 'bad' banned".into(),
                })
            } else {
                Decision::Accept
            }
        }

        fn filter_to(
            &mut self,
            email: &Email,
            _: &MailMetadata,
            _: &ConnectionMetadata<()>,
        ) -> Decision {
            if email.localpart().bytes() == &b"baz"[..] {
                Decision::Reject(Refusal {
                    code: ReplyCode::MAILBOX_UNAVAILABLE,
                    msg:  "No user 'baz'".into(),
                })
            } else {
                Decision::Accept
            }
        }

        fn handle_mail<'a, S: 'a + Stream<Item = BytesMut, Error = ()>>(
            &'a mut self,
            reader: DataStream<S>,
            meta: MailMetadata,
            _: &ConnectionMetadata<()>,
        ) -> Box<'a + Future<Item = (&'a mut Self, Option<Prependable<S>>, Decision), Error = ()>>
        where
            Self: 'a,
            S: 'a + Stream<Item = BytesMut, Error = ()>,
        {
            Box::new(reader.concat_and_recover().map_err(|_| ()).and_then(
                move |(mail_text, reader)| {
                    if mail_text.windows(5).position(|x| x == b"World").is_some() {
                        future::ok((
                            self,
                            Some(reader.into_inner()),
                            Decision::Reject(Refusal {
                                code: ReplyCode::POLICY_REASON,
                                msg:  "Don't you dare say 'World'!".into(),
                            }),
                        ))
                    } else {
                        self.mails.push((meta.from, meta.to, mail_text));
                        future::ok((self, Some(reader.into_inner()), Decision::Accept))
                    }
                },
            ))
        }
    }

    #[test]
    fn interacts_ok() {
        let tests: &[(&[&[u8]], &[u8], &[(Option<&[u8]>, &[&[u8]], &[u8])])] = &[
            // TODO: send banner before EHLO
            // TODO: send please go on after DATA
            (
                &[b"MAIL FROM:<>\r\n\
                    RCPT TO:<baz@quux.example.org>\r\n\
                    RCPT TO:<foo2@bar.example.org>\r\n\
                    RCPT TO:<foo3@bar.example.org>\r\n\
                    DATA\r\n\
                    Hello world\r\n\
                    .\r\n\
                    QUIT\r\n"],
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
                &[
                    b"MAIL FROM:<test@example.org>\r\n",
                    b"RCPT TO:<foo@example.org>\r\n",
                    b"DATA\r\n",
                    b"Hello World\r\n",
                    b".\r\n",
                    b"QUIT\r\n",
                ],
                b"250 Okay\r\n\
                  250 Okay\r\n\
                  550 Don't you dare say 'World'!\r\n\
                  502 Command not implemented\r\n",
                &[],
            ),
            (
                &[b"HELP hello\r\n"],
                b"502 Command not implemented\r\n",
                &[],
            ),
            (
                &[
                    b"MAIL FROM:<bad@quux.example.org>\r\n\
                      MAIL FROM:<foo@bar.example.org>\r\n\
                      MAIL FROM:<baz@quux.example.org>\r\n",
                    b"RCPT TO:<foo2@bar.example.org>\r\n\
                      DATA\r\n\
                      Hello\r\n",
                    b".\r\n\
                      QUIT\r\n",
                ],
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
                &[b"MAIL FROM:<foo@test.example.com>\r\n\
                    DATA\r\n\
                    QUIT\r\n"],
                b"250 Okay\r\n\
                  503 Bad sequence of commands\r\n\
                  502 Command not implemented\r\n",
                &[],
            ),
            (
                &[b"MAIL FROM:<foo@test.example.com>\r\n\
                    RCPT TO:<foo@bar.example.org>\r"],
                b"250 Okay\r\n",
                &[],
            ),
        ];
        for &(inp, out, mail) in tests {
            println!(
                "\nSending\n---\n{:?}---",
                inp.iter()
                    .map(|x| std::str::from_utf8(x).unwrap())
                    .collect::<Vec<&str>>()
            );
            let stream = stream::iter_ok(inp.iter().map(|x| BytesMut::from(*x)));
            let mut cfg = TestConfig { mails: Vec::new() };
            let mut resp = Vec::new();
            interact(stream, &mut resp, (), |()| (), |()| (), &mut cfg)
                .wait()
                .unwrap();
            let resp = resp.into_iter().concat();
            println!("Expecting\n---\n{}---", std::str::from_utf8(out).unwrap());
            println!("Got\n---\n{}---", std::str::from_utf8(&resp).unwrap());
            assert_eq!(resp, out);
            println!("Checking mails:");
            let resp_mail = cfg.mails;
            assert_eq!(resp_mail.len(), mail.len());
            for ((fr, tr, cr), &(fo, to, co)) in resp_mail.into_iter().zip(mail) {
                println!("Mail\n---");
                let fo = fo.map(SmtpString::from);
                let fr = fr.map(|x| x.as_string());
                println!("From: expected {:?}, got {:?}", fo, fr);
                assert_eq!(fo, fr);
                let to_smtp = to.iter().map(|x| SmtpString::from(*x)).collect::<Vec<_>>();
                let tr_smtp = tr.into_iter().map(|x| x.as_string()).collect::<Vec<_>>();
                println!("To: expected {:?}, got {:?}", to_smtp, tr_smtp);
                assert_eq!(to_smtp, tr_smtp);
                println!("Expected text\n--\n{}--", std::str::from_utf8(co).unwrap());
                println!("Got text\n--\n{}--", std::str::from_utf8(&cr).unwrap());
                assert_eq!(co, &cr[..]);
            }
        }
    }

    // Fuzzer-found
    #[test]
    fn interrupted_data() {
        let txt: &[&[u8]] = &[b"MAIL FROM:foo\r\n\
                                RCPT TO:bar\r\n\
                                DATA\r\n\
                                hello"];
        let stream = stream::iter_ok(txt.iter().map(|x| BytesMut::from(*x)));
        let mut cfg = TestConfig { mails: Vec::new() };
        let mut resp = Vec::new();
        let res = interact(stream, &mut resp, (), |()| (), |()| (), &mut cfg).wait();
        assert!(res.is_err());
    }

    // Fuzzer-found
    #[test]
    fn no_stack_overflow() {
        let txt: &[&[u8]] = &[
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r",
        ];
        let stream = stream::iter_ok(txt.iter().map(|x| BytesMut::from(*x)));
        let mut resp = Vec::new();
        let mut cfg = TestConfig { mails: Vec::new() };
        interact(stream, &mut resp, (), |()| (), |()| (), &mut cfg)
            .wait()
            .unwrap();
    }
}
