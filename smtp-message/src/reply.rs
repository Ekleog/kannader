use bytes::Bytes;
use std::{io, str::FromStr};

use crate::{
    builderror::BuildError,
    byteslice::ByteSlice,
    parseresult::{nom_to_result, ParseError},
    smtpstring::SmtpString,
};

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug, Clone, Copy)]
pub struct ReplyCode {
    code: u16,
}

#[cfg_attr(test, allow(dead_code))]
impl ReplyCode {
    pub const SYSTEM_STATUS: ReplyCode = ReplyCode { code: 211 };
    pub const HELP_MESSAGE: ReplyCode = ReplyCode { code: 214 };
    pub const SERVICE_READY: ReplyCode = ReplyCode { code: 220 };
    pub const CLOSING_CHANNEL: ReplyCode = ReplyCode { code: 221 };
    pub const OKAY: ReplyCode = ReplyCode { code: 250 };
    pub const USER_NOT_LOCAL_WILL_FORWARD: ReplyCode = ReplyCode { code: 251 };
    pub const CANNOT_VRFY_BUT_PLEASE_TRY: ReplyCode = ReplyCode { code: 252 };
    pub const START_MAIL_INPUT: ReplyCode = ReplyCode { code: 354 };
    pub const SERVICE_NOT_AVAILABLE: ReplyCode = ReplyCode { code: 421 };
    pub const MAILBOX_TEMPORARILY_UNAVAILABLE: ReplyCode = ReplyCode { code: 450 };
    pub const LOCAL_ERROR: ReplyCode = ReplyCode { code: 451 };
    pub const INSUFFICIENT_STORAGE: ReplyCode = ReplyCode { code: 452 };
    pub const UNABLE_TO_ACCEPT_PARAMETERS: ReplyCode = ReplyCode { code: 455 };
    pub const COMMAND_UNRECOGNIZED: ReplyCode = ReplyCode { code: 500 };
    pub const SYNTAX_ERROR: ReplyCode = ReplyCode { code: 501 };
    pub const COMMAND_UNIMPLEMENTED: ReplyCode = ReplyCode { code: 502 };
    pub const BAD_SEQUENCE: ReplyCode = ReplyCode { code: 503 };
    pub const PARAMETER_UNIMPLEMENTED: ReplyCode = ReplyCode { code: 504 };
    pub const MAILBOX_UNAVAILABLE: ReplyCode = ReplyCode { code: 550 };
    pub const POLICY_REASON: ReplyCode = ReplyCode { code: 550 };
    pub const USER_NOT_LOCAL: ReplyCode = ReplyCode { code: 551 };
    pub const EXCEEDED_STORAGE: ReplyCode = ReplyCode { code: 552 };
    pub const MAILBOX_NAME_INCORRECT: ReplyCode = ReplyCode { code: 553 };
    pub const TRANSACTION_FAILED: ReplyCode = ReplyCode { code: 554 };
    pub const MAIL_OR_RCPT_PARAMETER_UNIMPLEMENTED: ReplyCode = ReplyCode { code: 555 };

    pub fn custom(code: u16) -> ReplyCode {
        assert!(code < 1000);
        ReplyCode { code }
    }

    pub fn code(&self) -> u16 {
        self.code
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsLastLine {
    Yes,
    No,
}

// TODO: (B) add a multi-line Reply helper p:tcHW p:E4tJ p:J6HX p:XIP2

#[cfg_attr(test, derive(PartialEq))]
#[derive(Clone, Debug)]
pub struct ReplyLine {
    code: ReplyCode,
    is_last: IsLastLine,
    line: SmtpString,
}

impl ReplyLine {
    pub const MAX_LEN: usize = 506; // 506 is 512 - strlen(code) - strlen(is_last) - strlen("\r\n")

    pub fn build(
        code: ReplyCode,
        is_last: IsLastLine,
        line: SmtpString,
    ) -> Result<ReplyLine, BuildError> {
        if line.byte_len() > Self::MAX_LEN {
            Err(BuildError::LineTooLong {
                length: line.byte_len(),
                limit: Self::MAX_LEN,
            })
        } else if let Some(p) = line
            .iter_bytes()
            .position(|&x| !(x == 9 || (x >= 32 && x <= 126)))
        {
            Err(BuildError::DisallowedByte {
                b: line.byte(p),
                pos: p,
            })
        } else {
            Ok(ReplyLine {
                code,
                is_last,
                line,
            })
        }
    }

    // Parse one line of SMTP reply
    pub fn parse(arg: Bytes) -> Result<ReplyLine, ParseError> {
        nom_to_result(reply(ByteSlice::from(&arg)))
    }

    pub fn byte_len(&self) -> usize {
        6 + self.line.byte_len()
    }

    pub fn send_to(&self, w: &mut dyn io::Write) -> io::Result<()> {
        let code = &[
            ((self.code.code() % 1000) / 100) as u8 + b'0',
            ((self.code.code() % 100) / 10) as u8 + b'0',
            (self.code.code() % 10) as u8 + b'0',
        ];
        w.write_all(code)?;
        w.write_all(if self.is_last == IsLastLine::Yes {
            b" "
        } else {
            b"-"
        })?;
        w.write_all(&self.line.bytes()[..])?;
        w.write_all(b"\r\n")
    }
}

named!(pub reply(ByteSlice) -> ReplyLine, do_parse!(
    code: map!(
        verify!(
            map_res!(
                map_res!(take!(3), ByteSlice::into_utf8),
                |utf8| u16::from_str(utf8)
            ),
            |x: u16| x < 1000
        ),
        ReplyCode::custom
    ) >>
    is_last: map!(alt!(tag!("-") | tag!(" ")), |b| {
        if b.len() == 1 && b[0] == b' ' {
            IsLastLine::Yes
        } else {
            IsLastLine::No
        }
    }) >>
    line: take_until_and_consume!("\r\n") >>
    (ReplyLine { code, is_last, line: line.promote().into() })
));

#[cfg(test)]
mod tests {
    use super::*;

    use nom::IResult;

    #[test]
    fn reply_not_last() {
        let r = ReplyLine::build(
            ReplyCode::SERVICE_READY,
            IsLastLine::No,
            (&b"hello world!"[..]).into(),
        )
        .unwrap();
        assert_eq!(
            r,
            ReplyLine {
                code: ReplyCode { code: 220 },
                is_last: IsLastLine::No,
                line: (&b"hello world!"[..]).into(),
            }
        );

        let mut res = Vec::new();
        r.send_to(&mut res).unwrap();
        assert_eq!(res, b"220-hello world!\r\n");
    }

    #[test]
    fn reply_last() {
        let r = ReplyLine::build(
            ReplyCode::COMMAND_UNIMPLEMENTED,
            IsLastLine::Yes,
            (&b"test"[..]).into(),
        )
        .unwrap();
        assert_eq!(
            r,
            ReplyLine {
                code: ReplyCode { code: 502 },
                is_last: IsLastLine::Yes,
                line: (&b"test"[..]).into(),
            }
        );

        let mut res = Vec::new();
        r.send_to(&mut res).unwrap();
        assert_eq!(res, b"502 test\r\n");
    }

    #[test]
    fn refuse_build() {
        assert!(ReplyLine::build(
            ReplyCode::EXCEEDED_STORAGE,
            IsLastLine::Yes,
            (&vec![b'a'; 1000][..]).into(),
        )
        .is_err());
        assert!(ReplyLine::build(
            ReplyCode::EXCEEDED_STORAGE,
            IsLastLine::No,
            (&b"\r"[..]).into()
        )
        .is_err());
    }

    #[test]
    fn parse_ok() {
        let tests: &[(&[u8], ReplyLine)] = &[
            (
                b"250 All is well\r\n",
                ReplyLine {
                    code: ReplyCode { code: 250 },
                    is_last: IsLastLine::Yes,
                    line: (&b"All is well"[..]).into(),
                },
            ),
            (
                b"450-Temporary\r\n",
                ReplyLine {
                    code: ReplyCode { code: 450 },
                    is_last: IsLastLine::No,
                    line: (&b"Temporary"[..]).into(),
                },
            ),
            (
                b"354-Please do start input now\r\n",
                ReplyLine {
                    code: ReplyCode { code: 354 },
                    is_last: IsLastLine::No,
                    line: (&b"Please do start input now"[..]).into(),
                },
            ),
            (
                b"550 Something is really very wrong!\r\n",
                ReplyLine {
                    code: ReplyCode { code: 550 },
                    is_last: IsLastLine::Yes,
                    line: (&b"Something is really very wrong!"[..]).into(),
                },
            ),
        ];
        for (inp, out) in tests.iter().cloned() {
            let b = Bytes::from(inp);
            let res = reply(ByteSlice::from(&b));
            match res {
                IResult::Done(rem, ref res) if rem.len() == 0 => assert_eq!(res, &out),
                x => panic!("Unexpected `reply` result: {:?}", x),
            }
        }
    }
}
