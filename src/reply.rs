use std::{fmt, io, str};
use std::str::FromStr;
use nom::IResult;

use helpers::*;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Clone)]
pub struct Reply<'a> {
    num: u16,
    is_last: bool,
    line: &'a [u8],
}

macro_rules! reply_builder_function {
    ($code:tt, $fun:ident) => {
        pub fn $fun<'b>(is_last: bool, line: &'b [u8]) -> Result<Reply<'b>, BuildError> {
            if line.len() > 506 {
                Err(BuildError::LineTooLong { length: line.len(), limit: 506 })
            } else if let Some(p) = line.iter().position(|&x| !(x == 9 || (x >= 32 && x <= 126))) {
                Err(BuildError::DisallowedByte { b: line[p], pos: p })
            } else {
                Ok(Reply { num: $code, is_last, line })
            }
        }
    }
}

impl<'a> Reply<'a> {
    // Parse one line of SMTP reply
    pub fn parse(arg: &[u8]) -> Result<(Reply, &[u8]), ParseError> {
        match reply(arg) {
            IResult::Done(rem, res) => Ok((res, rem)),
            IResult::Error(e) => Err(ParseError::ParseError(e)),
            IResult::Incomplete(n) => Err(ParseError::IncompleteString(n)),
        }
    }

    pub fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        let code = &[
            ((self.num % 1000) / 100) as u8 + b'0',
            ((self.num % 100) / 10) as u8 + b'0',
            ((self.num % 10)) as u8 + b'0',
        ];
        w.write_all(code)?;
        w.write_all(if self.is_last { b" " } else { b"-" })?;
        w.write_all(self.line)?;
        w.write_all(b"\r\n")
    }

    reply_builder_function!(211, r211_system_status);
    reply_builder_function!(214, r214_help_message);
    reply_builder_function!(220, r220_service_ready);
    reply_builder_function!(221, r221_closing_channel);
    reply_builder_function!(250, r250_okay);
    reply_builder_function!(251, r251_user_not_local_will_forward);
    reply_builder_function!(252, r252_cannot_vrfy_but_please_try);
    reply_builder_function!(354, r354_start_mail_input);
    reply_builder_function!(421, r421_service_not_available);
    reply_builder_function!(450, r450_mailbox_temporarily_unavailable);
    reply_builder_function!(451, r451_local_error);
    reply_builder_function!(452, r452_insufficient_storage);
    reply_builder_function!(455, r455_unable_to_accept_parameters);
    reply_builder_function!(500, r500_command_unrecognized);
    reply_builder_function!(501, r501_syntax_error);
    reply_builder_function!(502, r502_command_unimplemented);
    reply_builder_function!(503, r503_bad_sequence);
    reply_builder_function!(504, r504_parameter_unimplemented);
    reply_builder_function!(550, r550_mailbox_unavailable);
    reply_builder_function!(550, r550_policy_reason);
    reply_builder_function!(551, r551_user_not_local);
    reply_builder_function!(552, r552_exceeded_storage);
    reply_builder_function!(553, r553_mailbox_name_incorrect);
    reply_builder_function!(554, r554_transaction_failed);
    reply_builder_function!(555, r555_mail_or_rcpt_parameter_unimplemented);
}

impl<'a> fmt::Debug for Reply<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(
            f,
            "Reply {{ num: {}, is_last: {}, line: {:?} }}",
            self.num,
            self.is_last,
            bytes_to_dbg(self.line)
        )
    }
}

named!(pub reply(&[u8]) -> Reply, do_parse!(
    num: verify!(
             map_res!(
                 map_res!(take!(3), |bytes| str::from_utf8(bytes)),
                 |utf8| u16::from_str(utf8)),
             |num| num < 1000) >>
    is_last: map!(alt!(tag!("-") | tag!(" ")), |b| b == b" ") >>
    line: take_until_and_consume!("\r\n") >>
    (Reply { num, is_last, line })
));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_not_last() {
        let r = Reply::r220_service_ready(false, b"hello world!").unwrap();
        assert_eq!(
            r,
            Reply {
                num: 220,
                is_last: false,
                line: b"hello world!",
            }
        );

        let mut res = Vec::new();
        r.send_to(&mut res).unwrap();
        assert_eq!(res, b"220-hello world!\r\n");
    }

    #[test]
    fn reply_last() {
        let r = Reply::r502_command_unimplemented(true, b"test").unwrap();
        assert_eq!(
            r,
            Reply {
                num: 502,
                is_last: true,
                line: b"test",
            }
        );

        let mut res = Vec::new();
        r.send_to(&mut res).unwrap();
        assert_eq!(res, b"502 test\r\n");
    }

    #[test]
    fn reply_codes() {
        assert_eq!(Reply::r211_system_status(true, b"").unwrap().num, 211);
        assert_eq!(Reply::r214_help_message(true, b"").unwrap().num, 214);
        assert_eq!(Reply::r220_service_ready(true, b"").unwrap().num, 220);
        assert_eq!(Reply::r221_closing_channel(true, b"").unwrap().num, 221);
        assert_eq!(Reply::r250_okay(true, b"").unwrap().num, 250);
        assert_eq!(
            Reply::r251_user_not_local_will_forward(true, b"")
                .unwrap()
                .num,
            251
        );
        assert_eq!(
            Reply::r252_cannot_vrfy_but_please_try(true, b"")
                .unwrap()
                .num,
            252
        );
        assert_eq!(Reply::r354_start_mail_input(true, b"").unwrap().num, 354);
        assert_eq!(
            Reply::r421_service_not_available(true, b"").unwrap().num,
            421
        );
        assert_eq!(
            Reply::r450_mailbox_temporarily_unavailable(true, b"")
                .unwrap()
                .num,
            450
        );
        assert_eq!(Reply::r451_local_error(true, b"").unwrap().num, 451);
        assert_eq!(
            Reply::r452_insufficient_storage(true, b"").unwrap().num,
            452
        );
        assert_eq!(
            Reply::r455_unable_to_accept_parameters(true, b"")
                .unwrap()
                .num,
            455
        );
        assert_eq!(
            Reply::r500_command_unrecognized(true, b"").unwrap().num,
            500
        );
        assert_eq!(Reply::r501_syntax_error(true, b"").unwrap().num, 501);
        assert_eq!(
            Reply::r502_command_unimplemented(true, b"").unwrap().num,
            502
        );
        assert_eq!(Reply::r503_bad_sequence(true, b"").unwrap().num, 503);
        assert_eq!(
            Reply::r504_parameter_unimplemented(true, b"").unwrap().num,
            504
        );
        assert_eq!(Reply::r550_mailbox_unavailable(true, b"").unwrap().num, 550);
        assert_eq!(Reply::r550_policy_reason(true, b"").unwrap().num, 550);
        assert_eq!(Reply::r551_user_not_local(true, b"").unwrap().num, 551);
        assert_eq!(Reply::r552_exceeded_storage(true, b"").unwrap().num, 552);
        assert_eq!(
            Reply::r553_mailbox_name_incorrect(true, b"").unwrap().num,
            553
        );
        assert_eq!(Reply::r554_transaction_failed(true, b"").unwrap().num, 554);
        assert_eq!(
            Reply::r555_mail_or_rcpt_parameter_unimplemented(true, b"")
                .unwrap()
                .num,
            555
        );
    }

    #[test]
    fn refuse_build() {
        assert!(Reply::r552_exceeded_storage(true, &vec![b'a'; 1000]).is_err());
        assert!(Reply::r552_exceeded_storage(true, b"\r").is_err());
    }

    #[test]
    fn parse_ok() {
        let tests: &[(&[u8], Reply)] = &[
            (
                b"250 All is well\r\n",
                Reply {
                    num: 250,
                    is_last: true,
                    line: b"All is well",
                },
            ),
            (
                b"450-Temporary\r\n",
                Reply {
                    num: 450,
                    is_last: false,
                    line: b"Temporary",
                },
            ),
            (
                b"354-Please do start input now\r\n",
                Reply {
                    num: 354,
                    is_last: false,
                    line: b"Please do start input now",
                },
            ),
            (
                b"550 Something is really very wrong!\r\n",
                Reply {
                    num: 550,
                    is_last: true,
                    line: b"Something is really very wrong!",
                },
            ),
        ];
        for test in tests {
            assert_eq!(reply(test.0), IResult::Done(&b""[..], test.1.clone()));
        }
    }
}
