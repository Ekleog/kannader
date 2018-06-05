use bytes::{BufMut, Bytes, BytesMut};
use smtp_message::{
    Command, DataStream, MailCommand, Prependable, RcptCommand, ReplyLine, StreamExt,
};
use tokio::prelude::{future::Either, *};

use config::Config;
use crlflines::CrlfLines;
use decision::Decision;
use metadata::{ConnectionMetadata, MailMetadata};
use sendreply::send_reply;
use stupidfut::FutIn9;

// TODO: (B) Allow Reader and Writer to return errors?
pub fn interact<
    'a,
    Reader: 'a + Stream<Item = BytesMut, Error = ()>,
    Writer: 'a + Sink<SinkItem = Bytes, SinkError = ()>,
    UserProvidedMetadata: 'static,
    Cfg: Config<UserProvidedMetadata>,
>(
    incoming: Reader,
    outgoing: Writer,
    metadata: UserProvidedMetadata,
    cfg: Cfg,
) -> impl Future<Item = (), Error = ()> + 'a {
    let conn_meta = ConnectionMetadata { user: metadata };
    let writer = outgoing.with(|c: ReplyLine| {
        let mut w = BytesMut::with_capacity(c.byte_len()).writer();
        // TODO: (B) refactor Sendable to send to a sink instead of to a Write
        c.send_to(&mut w).unwrap();
        // By design of BytesMut::writer, this cannot fail so long as the buffer
        // has sufficient capacity. As if this is not respected it is a clear
        // programming error, there's no need to try and handle this cleanly.
        future::ok(w.into_inner().freeze())
    });
    send_reply(writer, cfg.welcome_banner()).and_then(|writer| {
        CrlfLines::new(incoming.prependable())
            .fold_with_stream((cfg, writer, conn_meta, None), move |acc, line, reader| {
                handle_line(reader.into_inner(), acc, line).and_then(|(reader, acc)| {
                    future::result(reader.ok_or(()).map(|read| (CrlfLines::new(read), acc)))
                })
            })
            .map(|_acc| ()) // TODO: (B) warn of unfinished commands?
    })
}

// TODO: (B) use async/await here hide:async-await-in-rust-and-tokio
fn handle_line<
    'a,
    U: 'static,
    Writer: 'a + Sink<SinkItem = ReplyLine, SinkError = ()>,
    Reader: 'a + Stream<Item = BytesMut, Error = ()>,
    Cfg: Config<U>,
>(
    reader: Prependable<Reader>,
    (cfg, writer, conn_meta, mail_data): (Cfg, Writer, ConnectionMetadata<U>, Option<MailMetadata>),
    line: BytesMut,
) -> impl Future<
    Item = (
        Option<Prependable<Reader>>,
        (Cfg, Writer, ConnectionMetadata<U>, Option<MailMetadata>),
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
                FutIn9::Fut1(
                    send_reply(writer, cfg.already_in_mail()).and_then(|writer| {
                        future::ok((Some(reader), (cfg, writer, conn_meta, mail_data)))
                    }),
                )
            } else {
                FutIn9::Fut2(
                    cfg.new_mail()
                        .and_then(|cfg| cfg.filter_from(from, conn_meta))
                        .and_then(|(cfg, from, conn_meta, decision)| match decision {
                            Decision::Accept => {
                                let to = Vec::new();
                                Either::A(send_reply(writer, cfg.mail_okay()).and_then(|writer| {
                                    future::ok((
                                        Some(reader),
                                        (cfg, writer, conn_meta, Some(MailMetadata { from, to })),
                                    ))
                                }))
                            }
                            Decision::Reject(r) => Either::B(
                                send_reply(writer, (r.code, r.msg.into())).and_then(|writer| {
                                    future::ok((Some(reader), (cfg, writer, conn_meta, mail_data)))
                                }),
                            ),
                        }),
                )
            }
        }
        Ok(Command::Rcpt(RcptCommand {
            to: rcpt_to,
            params: _params,
        })) => {
            if let Some(mail_meta) = mail_data {
                FutIn9::Fut3(cfg.filter_to(rcpt_to, mail_meta, conn_meta).and_then(
                    |(cfg, rcpt_to, mail_meta, conn_meta, decision)| match decision {
                        Decision::Accept => {
                            let MailMetadata { from, mut to } = mail_meta;
                            to.push(rcpt_to);
                            Either::A(send_reply(writer, cfg.rcpt_okay()).and_then(|writer| {
                                future::ok((
                                    Some(reader),
                                    (cfg, writer, conn_meta, Some(MailMetadata { from, to })),
                                ))
                            }))
                        }
                        Decision::Reject(r) => {
                            Either::B(send_reply(writer, (r.code, r.msg)).and_then(|writer| {
                                future::ok((
                                    Some(reader),
                                    (cfg, writer, conn_meta, Some(mail_meta)),
                                ))
                            }))
                        }
                    },
                ))
            } else {
                FutIn9::Fut4(
                    send_reply(writer, cfg.rcpt_before_mail()).and_then(|writer| {
                        future::ok((Some(reader), (cfg, writer, conn_meta, mail_data)))
                    }),
                )
            }
        }
        Ok(Command::Data(_)) => {
            if let Some(mail_meta) = mail_data {
                if !mail_meta.to.is_empty() {
                    FutIn9::Fut5(cfg.filter_data(mail_meta, conn_meta).and_then(
                        |(cfg, mail_meta, conn_meta, decision)| match decision {
                            Decision::Accept => Either::A(
                                send_reply(writer, cfg.data_okay()).and_then(move |writer| {
                                    cfg.handle_mail(DataStream::new(reader), mail_meta, conn_meta)
                                        .and_then(|(cfg, reader, conn_meta, decision)| {
                                            match decision {
                                                Decision::Accept => Either::A(
                                                    send_reply(writer, cfg.mail_accepted())
                                                        .and_then(|writer| {
                                                            future::ok((
                                                                reader,
                                                                (cfg, writer, conn_meta, None),
                                                            ))
                                                        }),
                                                ),
                                                Decision::Reject(r) => Either::B(
                                                    send_reply(writer, (r.code, r.msg.into()))
                                                        .and_then(|writer| {
                                                            // Other mail systems (at least
                                                            // postfix, OpenSMTPD and gmail) appear
                                                            // to drop the state on an unsuccessful
                                                            // DATA command (eg. too long).
                                                            // Couldn't find the RFC reference
                                                            // anywhere, though.
                                                            future::ok((
                                                                reader,
                                                                (cfg, writer, conn_meta, None),
                                                            ))
                                                        }),
                                                ),
                                            }
                                        })
                                }),
                            ),
                            Decision::Reject(r) => Either::B(
                                send_reply(writer, (r.code, r.msg.into())).and_then(|writer| {
                                    future::ok((
                                        Some(reader),
                                        (cfg, writer, conn_meta, Some(mail_meta)),
                                    ))
                                }),
                            ),
                        },
                    ))
                } else {
                    FutIn9::Fut6(
                        send_reply(writer, cfg.data_before_rcpt()).and_then(|writer| {
                            future::ok((Some(reader), (cfg, writer, conn_meta, Some(mail_meta))))
                        }),
                    )
                }
            } else {
                FutIn9::Fut7(
                    send_reply(writer, cfg.data_before_mail()).and_then(|writer| {
                        future::ok((Some(reader), (cfg, writer, conn_meta, mail_data)))
                    }),
                )
            }
        }
        // TODO: (B) implement all the parsed commands and remove this case
        Ok(_) => FutIn9::Fut8(
            send_reply(writer, cfg.command_unimplemented())
                .and_then(|writer| future::ok((Some(reader), (cfg, writer, conn_meta, mail_data)))),
        ),
        Err(_) => FutIn9::Fut9(
            send_reply(writer, cfg.command_unrecognized())
                .and_then(|writer| future::ok((Some(reader), (cfg, writer, conn_meta, mail_data)))),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use itertools::Itertools;
    use smtp_message::{Email, ReplyCode, SmtpString};
    use std::{self, cell::RefCell, rc::Rc};

    use decision::Refusal;

    struct TestConfig {
        mails: Rc<RefCell<Vec<(Option<Email>, Vec<Email>, BytesMut)>>>,
    }

    impl Config<()> for TestConfig {
        fn hostname(&self) -> SmtpString {
            SmtpString::from_static(b"test.example.org")
        }

        fn filter_from(
            self,
            addr: Option<Email>,
            conn_meta: ConnectionMetadata<()>,
        ) -> Box<Future<Item = (Self, Option<Email>, ConnectionMetadata<()>, Decision), Error = ()>>
        {
            if addr == Some(Email::parse_slice(b"bad@quux.example.org").unwrap()) {
                Box::new(future::ok((
                    self,
                    addr,
                    conn_meta,
                    Decision::Reject(Refusal {
                        code: ReplyCode::POLICY_REASON,
                        msg:  "User 'bad' banned".into(),
                    }),
                )))
            } else {
                Box::new(future::ok((self, addr, conn_meta, Decision::Accept)))
            }
        }

        fn filter_to(
            self,
            email: Email,
            meta: MailMetadata,
            conn_meta: ConnectionMetadata<()>,
        ) -> Box<
            Future<
                Item = (Self, Email, MailMetadata, ConnectionMetadata<()>, Decision),
                Error = (),
            >,
        > {
            if email.localpart().bytes() == &b"baz"[..] {
                Box::new(future::ok((
                    self,
                    email,
                    meta,
                    conn_meta,
                    Decision::Reject(Refusal {
                        code: ReplyCode::MAILBOX_UNAVAILABLE,
                        msg:  "No user 'baz'".into(),
                    }),
                )))
            } else {
                Box::new(future::ok((self, email, meta, conn_meta, Decision::Accept)))
            }
        }

        fn handle_mail<'a, S: 'a + Stream<Item = BytesMut, Error = ()>>(
            self,
            reader: DataStream<S>,
            meta: MailMetadata,
            conn_meta: ConnectionMetadata<()>,
        ) -> Box<
            'a
                + Future<
                    Item = (
                        Self,
                        Option<Prependable<S>>,
                        ConnectionMetadata<()>,
                        Decision,
                    ),
                    Error = (),
                >,
        > {
            Box::new(reader.concat_and_recover().map_err(|_| ()).and_then(
                move |(mail_text, reader)| {
                    if mail_text.windows(5).position(|x| x == b"World").is_some() {
                        future::ok((
                            self,
                            Some(reader.into_inner()),
                            conn_meta,
                            Decision::Reject(Refusal {
                                code: ReplyCode::POLICY_REASON,
                                msg:  "Don't you dare say 'World'!".into(),
                            }),
                        ))
                    } else {
                        self.mails
                            .borrow_mut()
                            .push((meta.from, meta.to, mail_text));
                        future::ok((self, Some(reader.into_inner()), conn_meta, Decision::Accept))
                    }
                },
            ))
        }
    }

    #[test]
    fn interacts_ok() {
        let tests: &[(&[&[u8]], &[u8], &[(Option<&[u8]>, &[&[u8]], &[u8])])] = &[
            (
                &[b"MAIL FROM:<>\r\n\
                    RCPT TO:<baz@quux.example.org>\r\n\
                    RCPT TO:<foo2@bar.example.org>\r\n\
                    RCPT TO:<foo3@bar.example.org>\r\n\
                    DATA\r\n\
                    Hello world\r\n\
                    .\r\n\
                    QUIT\r\n"],
                b"220 test.example.org Service ready\r\n\
                  250 Okay\r\n\
                  550 No user 'baz'\r\n\
                  250 Okay\r\n\
                  250 Okay\r\n\
                  354 Start mail input; end with <CRLF>.<CRLF>\r\n\
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
                b"220 test.example.org Service ready\r\n\
                  250 Okay\r\n\
                  250 Okay\r\n\
                  354 Start mail input; end with <CRLF>.<CRLF>\r\n\
                  550 Don't you dare say 'World'!\r\n\
                  502 Command not implemented\r\n",
                &[],
            ),
            (
                &[b"HELP hello\r\n"],
                b"220 test.example.org Service ready\r\n\
                  502 Command not implemented\r\n",
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
                b"220 test.example.org Service ready\r\n\
                  550 User 'bad' banned\r\n\
                  250 Okay\r\n\
                  503 Bad sequence of commands\r\n\
                  250 Okay\r\n\
                  354 Start mail input; end with <CRLF>.<CRLF>\r\n\
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
                b"220 test.example.org Service ready\r\n\
                  250 Okay\r\n\
                  503 Bad sequence of commands\r\n\
                  502 Command not implemented\r\n",
                &[],
            ),
            (
                &[b"MAIL FROM:<foo@test.example.com>\r\n\
                    RCPT TO:<foo@bar.example.org>\r"],
                b"220 test.example.org Service ready\r\n\
                  250 Okay\r\n",
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
            let resp_mail = Rc::new(RefCell::new(Vec::new()));
            let mut cfg = TestConfig {
                mails: resp_mail.clone(),
            };
            let mut resp = Vec::new();
            interact(stream, &mut resp, (), cfg).wait().unwrap();
            let resp = resp.into_iter().concat();
            println!("Expecting\n---\n{}---", std::str::from_utf8(out).unwrap());
            println!("Got\n---\n{}---", std::str::from_utf8(&resp).unwrap());
            assert_eq!(resp, out);
            println!("Checking mails:");
            let resp_mail = Rc::try_unwrap(resp_mail).unwrap().into_inner();
            assert_eq!(resp_mail.len(), mail.len());
            for ((fr, tr, cr), &(fo, to, co)) in resp_mail.into_iter().zip(mail) {
                println!("Mail\n---");
                let fo = fo.map(SmtpString::from);
                let fr = fr.map(|x| SmtpString::from_sendable(&x).unwrap());
                println!("From: expected {:?}, got {:?}", fo, fr);
                assert_eq!(fo, fr);
                let to_smtp = to.iter().map(|x| SmtpString::from(*x)).collect::<Vec<_>>();
                let tr_smtp = tr.into_iter()
                    .map(|x| SmtpString::from_sendable(&x).unwrap())
                    .collect::<Vec<_>>();
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
        let cfg = TestConfig {
            mails: Rc::new(RefCell::new(Vec::new())),
        };
        let mut resp = Vec::new();
        let res = interact(stream, &mut resp, (), cfg).wait();
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
        let cfg = TestConfig {
            mails: Rc::new(RefCell::new(Vec::new())),
        };
        interact(stream, &mut resp, (), cfg).wait().unwrap();
    }
}
