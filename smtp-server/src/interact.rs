use std::pin::Pin;

use bytes::{BufMut, Bytes, BytesMut};
use futures::prelude::*;

use smtp_message::{Command, DataStream, MailCommand, Prependable, RcptCommand, ReplyLine};

use crate::{
    config::Config,
    crlflines::next_crlf_line,
    decision::Decision,
    metadata::{ConnectionMetadata, MailMetadata},
    sendreply::send_reply,
};

// TODO: (B) allow Reader and Writer to return errors?
// TODO: (B) give up on having `Stream`s and `Sink`s until async streams and
// async sinks land
pub async fn interact<'a, Reader, Writer, UserProvidedMetadata, Cfg>(
    incoming: Reader,
    outgoing: Pin<&'a mut Writer>,
    metadata: UserProvidedMetadata,
    cfg: &'a mut Cfg,
) -> Result<(), Writer::Error>
where
    Reader: 'a + Stream<Item = BytesMut> + Unpin, // TODO: consider removing this unpin
    Writer: 'a + Sink<Bytes, Error = ()>,         // TODO: allow the user to set any error
    UserProvidedMetadata: 'static,
    Cfg: 'a + Config<UserProvidedMetadata>,
{
    let mut conn_meta = ConnectionMetadata { user: metadata };
    let mut mail_meta = None;
    let mut writer = outgoing.with(async move |c: ReplyLine| -> Result<Bytes, Writer::Error> {
        let mut w = BytesMut::with_capacity(c.byte_len()).writer();
        // TODO: (B) refactor Sendable to send to a sink instead of to a Write
        c.send_to(&mut w).unwrap();
        // By design of BytesMut::writer, this cannot fail so long as the buffer
        // has sufficient capacity. As if this is not respected it is a clear
        // programming error, there's no need to try and handle this cleanly.
        Ok(w.into_inner().freeze())
    });
    fn randomtest<Writer: Sink<Bytes>, S: Sink<ReplyLine, Error = Writer::Error>>(_: &S) {}
    randomtest::<Writer, _>(&writer);
    let mut writer = unsafe { Pin::new_unchecked(&mut writer) };
    let mut reader = Prependable::new(incoming);
    let mut reader = unsafe { Pin::new_unchecked(&mut reader) };

    send_reply(writer.as_mut(), cfg.welcome_banner()).await?;
    // TODO: (C) optimize by trying parsing directly and not buffering until crlf
    // Rationale: it allows to make parsing 1-pass in most cases, which is more
    // efficient
    while let Some(line) = next_crlf_line(reader.as_mut()).await {
        handle_line(
            reader.as_mut(),
            writer.as_mut(),
            line,
            cfg,
            &mut conn_meta,
            &mut mail_meta,
        )
        .await?;
    }
    // TODO: (B) warn of unfinished commands?

    Ok(())
}

// TODO: (A) allow for errors in sinks & streams
async fn handle_line<'a, U, W, R, Cfg>(
    reader: Pin<&'a mut Prependable<R>>,
    mut writer: Pin<&'a mut W>,
    line: BytesMut,
    cfg: &'a mut Cfg,
    conn_meta: &'a mut ConnectionMetadata<U>,
    mail_meta: &'a mut Option<MailMetadata>,
) -> Result<(), W::Error>
where
    U: 'static,
    W: 'a + Sink<ReplyLine>,
    R: 'a + Stream<Item = BytesMut> + Unpin,
    Cfg: Config<U>,
{
    let cmd = Command::parse(line.freeze());
    match cmd {
        Ok(Command::Mail(MailCommand {
            mut from,
            params: _params, // TODO: (C) this should be used
        })) => {
            if mail_meta.is_some() {
                send_reply(writer, cfg.already_in_mail()).await?;
            // TODO: (B) check we're not supposed to drop mail_meta
            } else {
                cfg.new_mail().await;
                match cfg.filter_from(&mut from, conn_meta).await {
                    Decision::Accept => {
                        let to = Vec::new();
                        send_reply(writer, cfg.mail_okay()).await?;
                        *mail_meta = Some(MailMetadata { from, to });
                    }
                    Decision::Reject(r) => {
                        send_reply(writer, (r.code, r.msg.into())).await?;
                    }
                }
            }
        }
        Ok(Command::Rcpt(RcptCommand {
            mut to,
            params: _params, // TODO: (C) this should be used
        })) => {
            if let Some(ref mut mail_meta_unw) = *mail_meta {
                match cfg.filter_to(&mut to, mail_meta_unw, conn_meta).await {
                    Decision::Accept => {
                        mail_meta_unw.to.push(to);
                        send_reply(writer, cfg.rcpt_okay()).await?;
                    }
                    Decision::Reject(r) => {
                        send_reply(writer, (r.code, r.msg)).await?;
                    }
                }
            } else {
                send_reply(writer, cfg.rcpt_before_mail()).await?;
            }
        }
        Ok(Command::Data(_)) => {
            if let Some(mut mail_meta_unw) = mail_meta.take() {
                if !mail_meta_unw.to.is_empty() {
                    match cfg.filter_data(&mut mail_meta_unw, conn_meta).await {
                        Decision::Accept => {
                            send_reply(writer.as_mut(), cfg.data_okay()).await?;
                            let mut data_stream = DataStream::new(reader);
                            let decision = cfg
                                .handle_mail(&mut data_stream, mail_meta_unw, conn_meta)
                                .await;
                            // TODO: (B) fail more elegantly on configuration error
                            assert!(data_stream.was_completed());
                            match decision {
                                Decision::Accept => {
                                    send_reply(writer, cfg.mail_accepted()).await?;
                                }
                                Decision::Reject(r) => {
                                    send_reply(writer, (r.code, r.msg.into())).await?;
                                    // Other mail systems (at least postfix,
                                    // OpenSMTPD and gmail)
                                    // appear to drop the state on an
                                    // unsuccessful DATA command
                                    // (eg. too long). Couldn't find the RFC
                                    // reference anywhere,
                                    // though.
                                }
                            }
                        }
                        Decision::Reject(r) => {
                            send_reply(writer, (r.code, r.msg.into())).await?;
                            *mail_meta = Some(mail_meta_unw);
                        }
                    }
                } else {
                    send_reply(writer, cfg.data_before_rcpt()).await?;
                    *mail_meta = Some(mail_meta_unw);
                }
            } else {
                send_reply(writer, cfg.data_before_mail()).await?;
            }
        }
        // TODO: (B) implement all the parsed commands and remove this case
        Ok(_) => {
            send_reply(writer, cfg.command_unimplemented()).await?;
        }
        Err(_) => {
            send_reply(writer, cfg.command_unrecognized()).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::{self, cell::RefCell, rc::Rc};

    use futures::executor;
    use itertools::Itertools;

    use smtp_message::{Email, ReplyCode, SmtpString};

    use crate::decision::Refusal;

    struct TestConfig {
        mails: Rc<RefCell<Vec<(Option<Email>, Vec<Email>, BytesMut)>>>,
    }

    impl Config<()> for TestConfig {
        fn hostname(&self) -> SmtpString {
            SmtpString::from_static(b"test.example.org")
        }

        fn filter_from<'a>(
            &'a mut self,
            addr: &'a mut Option<Email>,
            _conn_meta: &'a mut ConnectionMetadata<()>,
        ) -> Pin<Box<dyn 'a + Future<Output = Decision>>> {
            if *addr == Some(Email::parse_slice(b"bad@quux.example.org").unwrap()) {
                Box::pin(future::ready(Decision::Reject(Refusal {
                    code: ReplyCode::POLICY_REASON,
                    msg: "User 'bad' banned".into(),
                })))
            } else {
                Box::pin(future::ready(Decision::Accept))
            }
        }

        fn filter_to<'a>(
            &'a mut self,
            email: &'a mut Email,
            _meta: &'a mut MailMetadata,
            _conn_meta: &'a mut ConnectionMetadata<()>,
        ) -> Pin<Box<dyn 'a + Future<Output = Decision>>> {
            if email.localpart().bytes() == &b"baz"[..] {
                Box::pin(future::ready(Decision::Reject(Refusal {
                    code: ReplyCode::MAILBOX_UNAVAILABLE,
                    msg: "No user 'baz'".into(),
                })))
            } else {
                Box::pin(future::ready(Decision::Accept))
            }
        }

        fn handle_mail<'a, S: 'a + Stream<Item = BytesMut> + Unpin>(
            &'a mut self,
            reader: &'a mut DataStream<S>,
            meta: MailMetadata,
            _conn_meta: &'a mut ConnectionMetadata<()>,
        ) -> Pin<Box<dyn 'a + Future<Output = Decision>>> {
            Box::pin(async move {
                let mail_text = reader.concat().await;
                if let Err(_) = reader.complete() {
                    Decision::Reject(Refusal {
                        code: ReplyCode::BAD_SEQUENCE,
                        msg: "Closed the channel before end of message".into(),
                    })
                } else if mail_text.windows(5).position(|x| x == b"World").is_some() {
                    Decision::Reject(Refusal {
                        code: ReplyCode::POLICY_REASON,
                        msg: "Don't you dare say 'World'!".into(),
                    })
                } else {
                    self.mails
                        .borrow_mut()
                        .push((meta.from, meta.to, mail_text));
                    Decision::Accept
                }
            })
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
            let stream = stream::iter(inp.iter().map(|x| BytesMut::from(*x)));
            let resp_mail = Rc::new(RefCell::new(Vec::new()));
            let mut cfg = TestConfig {
                mails: resp_mail.clone(),
            };
            let mut resp = Vec::new();
            let mut resp_sink = Box::pin((&mut resp).sink_map_err(|_| ()));
            executor::block_on(interact(stream, resp_sink.as_mut(), (), &mut cfg)).unwrap();
            let resp = resp.into_iter().concat();
            println!("Expecting\n---\n{}---", std::str::from_utf8(out).unwrap());
            println!("Got\n---\n{}---", std::str::from_utf8(&resp).unwrap());
            assert_eq!(resp, out);
            println!("Checking mails:");
            drop(cfg);
            let resp_mail = Rc::try_unwrap(resp_mail).unwrap().into_inner();
            assert_eq!(resp_mail.len(), mail.len());
            for ((fr, tr, cr), &(fo, to, co)) in resp_mail.into_iter().zip(mail) {
                println!("Mail\n---");
                let fo = fo.map(SmtpString::from);
                let fr = fr.map(|x| SmtpString::from_sendable(&x).unwrap());
                println!("From: expected {:?}, got {:?}", fo, fr);
                assert_eq!(fo, fr);
                let to_smtp = to.iter().map(|x| SmtpString::from(*x)).collect::<Vec<_>>();
                let tr_smtp = tr
                    .into_iter()
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
        let stream = stream::iter(txt.iter().map(|x| BytesMut::from(*x)));
        let mut cfg = TestConfig {
            mails: Rc::new(RefCell::new(Vec::new())),
        };
        let mut resp = Box::pin(Vec::new().sink_map_err(|_| ()));
        executor::block_on(interact(stream, resp.as_mut(), (), &mut cfg)).unwrap();
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
        let stream = stream::iter(txt.iter().map(|x| BytesMut::from(*x)));
        let mut resp = Box::pin(Vec::new().sink_map_err(|_| ()));
        let mut cfg = TestConfig {
            mails: Rc::new(RefCell::new(Vec::new())),
        };
        executor::block_on(interact(stream, resp.as_mut(), (), &mut cfg)).unwrap();
    }
}
