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
// TODO: (B) give up on having `Stream`s and `Sink`s until async streams and async sinks land
pub async fn interact<
    'a,
    Reader: 'a + Stream<Item = BytesMut>,
    Writer: 'a + Sink<Bytes, SinkError = ()>,
    UserProvidedMetadata: 'static,
    Cfg: 'a + Config<UserProvidedMetadata>,
>(
    incoming: Reader,
    outgoing: Pin<&'a mut Writer>,
    metadata: UserProvidedMetadata,
    cfg: &'a mut Cfg,
) -> Result<(), Writer::SinkError> {
    let mut conn_meta = ConnectionMetadata { user: metadata };
    let mut mail_meta = None;
    let mut writer = outgoing.with(
        async move |c: ReplyLine| -> Result<Bytes, Writer::SinkError> {
            let mut w = BytesMut::with_capacity(c.byte_len()).writer();
            // TODO: (B) refactor Sendable to send to a sink instead of to a Write
            c.send_to(&mut w).unwrap();
            // By design of BytesMut::writer, this cannot fail so long as the buffer
            // has sufficient capacity. As if this is not respected it is a clear
            // programming error, there's no need to try and handle this cleanly.
            Ok(w.into_inner().freeze())
        },
    );
    fn randomtest<Writer: Sink<Bytes>, S: Sink<ReplyLine, SinkError = Writer::SinkError>>(_: &S) {}
    randomtest::<Writer, _>(&writer);
    let mut writer = unsafe { Pin::new_unchecked(&mut writer) };
    let mut reader = Prependable::new(incoming);
    let mut reader = unsafe { Pin::new_unchecked(&mut reader) };

    await!(send_reply(writer.as_mut(), cfg.welcome_banner()))?;
    // TODO: (C) optimize by trying parsing directly and not buffering until crlf
    // Rationale: it allows to make parsing 1-pass in most cases, which is more
    // efficient
    while let Some(line) = await!(next_crlf_line(reader.as_mut())) {
        await!(handle_line(
            reader.as_mut(),
            writer.as_mut(),
            line,
            cfg,
            &mut conn_meta,
            &mut mail_meta
        ));
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
) where
    U: 'static,
    W: 'a + Sink<ReplyLine>,
    R: 'a + Stream<Item = BytesMut>,
    Cfg: Config<U>,
{
    let cmd = Command::parse(line.freeze());
    match cmd {
        Ok(Command::Mail(MailCommand {
            mut from,
            params: _params, // TODO: (C) this should be used
        })) => {
            if mail_meta.is_some() {
                await!(send_reply(writer, cfg.already_in_mail()));
            // TODO: (B) check we're not supposed to drop mail_meta
            } else {
                await!(cfg.new_mail());
                match await!(cfg.filter_from(&mut from, conn_meta)) {
                    Decision::Accept => {
                        let to = Vec::new();
                        await!(send_reply(writer, cfg.mail_okay()));
                        *mail_meta = Some(MailMetadata { from, to });
                    }
                    Decision::Reject(r) => {
                        await!(send_reply(writer, (r.code, r.msg.into())));
                    }
                }
            }
        }
        Ok(Command::Rcpt(RcptCommand {
            mut to,
            params: _params, // TODO: (C) this should be used
        })) => {
            if let Some(ref mut mail_meta_unw) = *mail_meta {
                match await!(cfg.filter_to(&mut to, mail_meta_unw, conn_meta)) {
                    Decision::Accept => {
                        mail_meta_unw.to.push(to);
                        await!(send_reply(writer, cfg.rcpt_okay()));
                    }
                    Decision::Reject(r) => {
                        await!(send_reply(writer, (r.code, r.msg)));
                    }
                }
            } else {
                await!(send_reply(writer, cfg.rcpt_before_mail()));
            }
        }
        Ok(Command::Data(_)) => {
            if let Some(mut mail_meta_unw) = mail_meta.take() {
                if !mail_meta_unw.to.is_empty() {
                    match await!(cfg.filter_data(&mut mail_meta_unw, conn_meta)) {
                        Decision::Accept => {
                            await!(send_reply(writer.as_mut(), cfg.data_okay()));
                            let mut data_stream = DataStream::new(reader);
                            let decision =
                                await!(cfg.handle_mail(&mut data_stream, mail_meta_unw, conn_meta));
                            // TODO: (B) fail more elegantly on configuration error
                            assert!(data_stream.was_completed());
                            match decision {
                                Decision::Accept => {
                                    await!(send_reply(writer, cfg.mail_accepted()));
                                }
                                Decision::Reject(r) => {
                                    await!(send_reply(writer, (r.code, r.msg.into())));
                                    // Other mail systems (at least postfix, OpenSMTPD and gmail)
                                    // appear to drop the state on an unsuccessful DATA command
                                    // (eg. too long). Couldn't find the RFC reference anywhere,
                                    // though.
                                }
                            }
                        }
                        Decision::Reject(r) => {
                            await!(send_reply(writer, (r.code, r.msg.into())));
                            *mail_meta = Some(mail_meta_unw);
                        }
                    }
                } else {
                    await!(send_reply(writer, cfg.data_before_rcpt()));
                    *mail_meta = Some(mail_meta_unw);
                }
            } else {
                await!(send_reply(writer, cfg.data_before_mail()));
            }
        }
        // TODO: (B) implement all the parsed commands and remove this case
        Ok(_) => {
            await!(send_reply(writer, cfg.command_unimplemented()));
        }
        Err(_) => {
            await!(send_reply(writer, cfg.command_unrecognized()));
        }
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
