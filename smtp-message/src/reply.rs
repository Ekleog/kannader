use std::{convert::TryInto, fmt, io::IoSlice, iter, str};

use lazy_static::lazy_static;
use nom::{
    branch::alt,
    bytes::streaming::{tag, take},
    combinator::{map, opt, peek, value, verify},
    multi::many0,
    sequence::{pair, preceded, terminated, tuple},
    IResult,
};
use regex_automata::{Regex, RegexBuilder};

use crate::*;

lazy_static! {
    static ref REPLY_CODE: Regex = RegexBuilder::new()
        .anchored(true)
        .build(r#"[2-5][0-9][0-9]"#)
        .unwrap();
    static ref EXTENDED_REPLY_CODE: Regex = RegexBuilder::new()
        .anchored(true)
        .build(r#"[245]\.[0-9]{1,3}\.[0-9]{1,3}"#)
        .unwrap();
    static ref REPLY_TEXT_ASCII: Regex = RegexBuilder::new()
        .anchored(true)
        .build(r#"[\t -~]*"#)
        .unwrap();
    static ref REPLY_TEXT_UTF8: Regex = RegexBuilder::new()
        .anchored(true)
        .build(r#"[\t -~[:^ascii:]]*"#)
        .unwrap();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplyCodeKind {
    PositiveCompletion,
    PositiveIntermediate,
    TransientNegative,
    PermanentNegative,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplyCodeCategory {
    Syntax,
    Information,
    Connection,
    ReceiverStatus,
    Unspecified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplyCode(pub [u8; 3]);

#[rustfmt::skip]
impl ReplyCode {
    pub const SYSTEM_STATUS: ReplyCode = ReplyCode(*b"211");
    pub const HELP_MESSAGE: ReplyCode = ReplyCode(*b"214");
    pub const SERVICE_READY: ReplyCode = ReplyCode(*b"220");
    pub const CLOSING_CHANNEL: ReplyCode = ReplyCode(*b"221");
    pub const OKAY: ReplyCode = ReplyCode(*b"250");
    pub const USER_NOT_LOCAL_WILL_FORWARD: ReplyCode = ReplyCode(*b"251");
    pub const CANNOT_VRFY_BUT_PLEASE_TRY: ReplyCode = ReplyCode(*b"252");
    pub const START_MAIL_INPUT: ReplyCode = ReplyCode(*b"354");
    pub const SERVICE_NOT_AVAILABLE: ReplyCode = ReplyCode(*b"421");
    pub const MAILBOX_TEMPORARILY_UNAVAILABLE: ReplyCode = ReplyCode(*b"450");
    pub const LOCAL_ERROR: ReplyCode = ReplyCode(*b"451");
    pub const INSUFFICIENT_STORAGE: ReplyCode = ReplyCode(*b"452");
    pub const UNABLE_TO_ACCEPT_PARAMETERS: ReplyCode = ReplyCode(*b"455");
    pub const COMMAND_UNRECOGNIZED: ReplyCode = ReplyCode(*b"500");
    pub const SYNTAX_ERROR: ReplyCode = ReplyCode(*b"501");
    pub const COMMAND_UNIMPLEMENTED: ReplyCode = ReplyCode(*b"502");
    pub const BAD_SEQUENCE: ReplyCode = ReplyCode(*b"503");
    pub const PARAMETER_UNIMPLEMENTED: ReplyCode = ReplyCode(*b"504");
    pub const SERVER_DOES_NOT_ACCEPT_MAIL: ReplyCode = ReplyCode(*b"521");
    pub const MAILBOX_UNAVAILABLE: ReplyCode = ReplyCode(*b"550");
    pub const POLICY_REASON: ReplyCode = ReplyCode(*b"550");
    pub const USER_NOT_LOCAL: ReplyCode = ReplyCode(*b"551");
    pub const EXCEEDED_STORAGE: ReplyCode = ReplyCode(*b"552");
    pub const MAILBOX_NAME_INCORRECT: ReplyCode = ReplyCode(*b"553");
    pub const TRANSACTION_FAILED: ReplyCode = ReplyCode(*b"554");
    pub const MAIL_OR_RCPT_PARAMETER_UNIMPLEMENTED: ReplyCode = ReplyCode(*b"555");
    pub const DOMAIN_DOES_NOT_ACCEPT_MAIL: ReplyCode = ReplyCode(*b"556");
}

impl ReplyCode {
    #[inline]
    pub fn parse(buf: &[u8]) -> IResult<&[u8], ReplyCode> {
        map(apply_regex(&REPLY_CODE), |b| {
            // The below unwrap is OK, as the regex already validated
            // that there are exactly 3 characters
            ReplyCode(b.try_into().unwrap())
        })(buf)
    }

    #[inline]
    pub fn kind(&self) -> ReplyCodeKind {
        match self.0[0] {
            b'2' => ReplyCodeKind::PositiveCompletion,
            b'3' => ReplyCodeKind::PositiveIntermediate,
            b'4' => ReplyCodeKind::TransientNegative,
            b'5' => ReplyCodeKind::PermanentNegative,
            _ => panic!("Asked kind of invalid reply code!"),
        }
    }

    #[inline]
    pub fn category(&self) -> ReplyCodeCategory {
        match self.0[1] {
            b'0' => ReplyCodeCategory::Syntax,
            b'1' => ReplyCodeCategory::Information,
            b'2' => ReplyCodeCategory::Connection,
            b'5' => ReplyCodeCategory::ReceiverStatus,
            _ => ReplyCodeCategory::Unspecified,
        }
    }

    #[inline]
    pub fn code(&self) -> u16 {
        self.0[0] as u16 * 100 + self.0[1] as u16 * 10 + self.0[2] as u16 - b'0' as u16 * 111
    }

    #[inline]
    pub fn as_io_slices(&self) -> impl Iterator<Item = IoSlice> {
        iter::once(IoSlice::new(&self.0))
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum EnhancedReplyCodeClass {
    Success = 2,
    PersistentTransient = 4,
    PermanentFailure = 5,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum EnhancedReplyCodeSubject {
    Undefined,
    Addressing,
    Mailbox,
    MailSystem,
    Network,
    MailDelivery,
    Content,
    Policy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnhancedReplyCode<S> {
    pub raw: S,
    pub class: EnhancedReplyCodeClass,
    pub raw_subject: u16,
    pub raw_detail: u16,
}

macro_rules! extended_reply_codes {
    ($(($success:tt, $transient:tt, $permanent:tt, $subject:tt, $detail:tt),)*) => {
        $(
            extended_reply_codes!(@, success, $success, $subject, $detail);
            extended_reply_codes!(@, transient, $transient, $subject, $detail);
            extended_reply_codes!(@, permanent, $permanent, $subject, $detail);
        )*
    };

    (@, $any:ident, _, $subject:tt, $detail:tt) => {}; // ignore these

    (@, success, $success:ident, $subject:tt, $detail:tt) => {
        pub const $success: EnhancedReplyCode<&'static str> = EnhancedReplyCode {
            raw: concat!("2.", stringify!($subject), ".", stringify!($detail)),
            class: EnhancedReplyCodeClass::Success,
            raw_subject: $subject,
            raw_detail: $detail,
        };
    };

    (@, transient, $transient:ident, $subject:tt, $detail:tt) => {
        pub const $transient: EnhancedReplyCode<&'static str> = EnhancedReplyCode {
            raw: concat!("4.", stringify!($subject), ".", stringify!($detail)),
            class: EnhancedReplyCodeClass::PersistentTransient,
            raw_subject: $subject,
            raw_detail: $detail,
        };
    };

    (@, permanent, $permanent:ident, $subject:tt, $detail:tt) => {
        pub const $permanent: EnhancedReplyCode<&'static str> = EnhancedReplyCode {
            raw: concat!("5.", stringify!($subject), ".", stringify!($detail)),
            class: EnhancedReplyCodeClass::PermanentFailure,
            raw_subject: $subject,
            raw_detail: $detail,
        };
    };
}

#[rustfmt::skip]
impl EnhancedReplyCode<&'static str> {
    extended_reply_codes!(
        (SUCCESS_UNDEFINED, TRANSIENT_UNDEFINED, PERMANENT_UNDEFINED, 0, 0),

        (SUCCESS_ADDRESS_OTHER, TRANSIENT_ADDRESS_OTHER, PERMANENT_ADDRESS_OTHER, 1, 0),
        (_, _, PERMANENT_BAD_DEST_MAILBOX, 1, 1),
        (_, _, PERMANENT_BAD_DEST_SYSTEM, 1, 2),
        (_, _, PERMANENT_BAD_DEST_MAILBOX_SYNTAX, 1, 3),
        (SUCCESS_DEST_MAILBOX_AMBIGUOUS, TRANSIENT_DEST_MAILBOX_AMBIGUOUS, PERMANENT_DEST_MAILBOX_AMBIGUOUS, 1, 4),
        (SUCCESS_DEST_VALID, _, _, 1, 5),
        (_, _, PERMANENT_DEST_MAILBOX_HAS_MOVED, 1, 6),
        (_, _, PERMANENT_BAD_SENDER_MAILBOX_SYNTAX, 1, 7),
        (_, TRANSIENT_BAD_SENDER_SYSTEM, PERMANENT_BAD_SENDER_SYSTEM, 1, 8),
        (SUCCESS_MESSAGE_RELAYED_TO_NON_COMPLIANT_MAILER, _, PERMANENT_MESSAGE_RELAYED_TO_NON_COMPLIANT_MAILER, 1, 9),
        (_, _, PERMANENT_RECIPIENT_ADDRESS_HAS_NULL_MX, 1, 10),

        (SUCCESS_MAILBOX_OTHER, TRANSIENT_MAILBOX_OTHER, PERMANENT_MAILBOX_OTHER, 2, 0),
        (_, TRANSIENT_MAILBOX_DISABLED, PERMANENT_MAILBOX_DISABLED, 2, 1),
        (_, TRANSIENT_MAILBOX_FULL, _, 2, 2),
        (_, _, PERMANENT_MESSAGE_TOO_LONG_FOR_MAILBOX, 2, 3),
        (_, TRANSIENT_MAILING_LIST_EXPANSION_ISSUE, PERMANENT_MAILING_LIST_EXPANSION_ISSUE, 2, 4),

        (SUCCESS_SYSTEM_OTHER, TRANSIENT_SYSTEM_OTHER, PERMANENT_SYSTEM_OTHER, 3, 0),
        (_, TRANSIENT_SYSTEM_FULL, _, 3, 1),
        (_, TRANSIENT_SYSTEM_NOT_ACCEPTING_MESSAGES, PERMANENT_SYSTEM_NOT_ACCEPTING_MESSAGES, 3, 2),
        (_, TRANSIENT_SYSTEM_INCAPABLE_OF_FEATURE, PERMANENT_SYSTEM_INCAPABLE_OF_FEATURE, 3, 3),
        (_, _, PERMANENT_MESSAGE_TOO_BIG, 3, 4),
        (_, TRANSIENT_SYSTEM_INCORRECTLY_CONFIGURED, PERMANENT_SYSTEM_INCORRECTLY_CONFIGURED, 3, 5),
        (SUCCESS_REQUESTED_PRIORITY_WAS_CHANGED, _, _, 3, 6),

        (SUCCESS_NETWORK_OTHER, TRANSIENT_NETWORK_OTHER, PERMANENT_NETWORK_OTHER, 4, 0),
        (_, TRANSIENT_NO_ANSWER_FROM_HOST, _, 4, 1),
        (_, TRANSIENT_BAD_CONNECTION, _, 4, 2),
        (_, TRANSIENT_DIRECTORY_SERVER_FAILURE, _, 4, 3),
        (_, TRANSIENT_UNABLE_TO_ROUTE, PERMANENT_UNABLE_TO_ROUTE, 4, 4),
        (_, TRANSIENT_SYSTEM_CONGESTION, _, 4, 5),
        (_, TRANSIENT_ROUTING_LOOP_DETECTED, _, 4, 6),
        (_, TRANSIENT_DELIVERY_TIME_EXPIRED, PERMANENT_DELIVERY_TIME_EXPIRED, 4, 7),

        (SUCCESS_DELIVERY_OTHER, TRANSIENT_DELIVERY_OTHER, PERMANENT_DELIVERY_OTHER, 5, 0),
        (_, _, PERMANENT_INVALID_COMMAND, 5, 1),
        (_, _, PERMANENT_SYNTAX_ERROR, 5, 2),
        (_, TRANSIENT_TOO_MANY_RECIPIENTS, PERMANENT_TOO_MANY_RECIPIENTS, 5, 3),
        (_, _, PERMANENT_INVALID_COMMAND_ARGUMENTS, 5, 4),
        (_, TRANSIENT_WRONG_PROTOCOL_VERSION, PERMANENT_WRONG_PROTOCOL_VERSION, 5, 5),
        (_, TRANSIENT_AUTH_EXCHANGE_LINE_TOO_LONG, PERMANENT_AUTH_EXCHANGE_LINE_TOO_LONG, 5, 6),

        (SUCCESS_CONTENT_OTHER, TRANSIENT_CONTENT_OTHER, PERMANENT_CONTENT_OTHER, 6, 0),
        (_, _, PERMANENT_MEDIA_NOT_SUPPORTED, 6, 1),
        (_, TRANSIENT_CONVERSION_REQUIRED_AND_PROHIBITED, PERMANENT_CONVERSION_REQUIRED_AND_PROHIBITED, 6, 2),
        (_, TRANSIENT_CONVERSION_REQUIRED_BUT_NOT_SUPPORTED, PERMANENT_CONVERSION_REQUIRED_BUT_NOT_SUPPORTED, 6, 3),
        (SUCCESS_CONVERSION_WITH_LOSS_PERFORMED, TRANSIENT_CONVERSION_WITH_LOSS_PERFORMED, PERMANENT_CONVERSION_WITH_LOSS_PERFORMED, 6, 4),
        (_, TRANSIENT_CONVERSION_FAILED, PERMANENT_CONVERSION_FAILED, 6, 5),
        (_, TRANSIENT_MESSAGE_CONTENT_NOT_AVAILABLE, PERMANENT_MESSAGE_CONTENT_NOT_AVAILABLE, 6, 6),
        (_, _, PERMANENT_NON_ASCII_ADDRESSES_NOT_PERMITTED, 6, 7),
        (SUCCESS_UTF8_WOULD_BE_REQUIRED, TRANSIENT_UTF8_WOULD_BE_REQUIRED, PERMANENT_UTF8_WOULD_BE_REQUIRED, 6, 8),
        (_, _, PERMANENT_UTF8_MESSAGE_CANNOT_BE_TRANSMITTED, 6, 9),
        (SUCCESS_UTF8_WOULD_BE_REQUIRED_BIS, TRANSIENT_UTF8_WOULD_BE_REQUIRED_BIS, PERMANENT_UTF8_WOULD_BE_REQUIRED_BIS, 6, 10),

        (SUCCESS_POLICY_OTHER, TRANSIENT_POLICY_OTHER, PERMANENT_POLICY_OTHER, 7, 0),
        (_, _, PERMANENT_DELIVERY_NOT_AUTHORIZED, 7, 1),
        (_, _, PERMANENT_MAILING_LIST_EXPANSION_PROHIBITED, 7, 2),
        (_, _, PERMANENT_SECURITY_CONVERSION_REQUIRED_BUT_NOT_POSSIBLE, 7, 3),
        (_, _, PERMANENT_SECURITY_FEATURES_NOT_SUPPORTED, 7, 4),
        (_, TRANSIENT_CRYPTO_FAILURE, PERMANENT_CRYPTO_FAILURE, 7, 5),
        (_, TRANSIENT_CRYPTO_ALGO_NOT_SUPPORTED, PERMANENT_CRYPTO_ALGO_NOT_SUPPORTED, 7, 6),
        (SUCCESS_MESSAGE_INTEGRITY_FAILURE, TRANSIENT_MESSAGE_INTEGRITY_FAILURE, PERMANENT_MESSAGE_INTEGRITY_FAILURE, 7, 7),
        (_, _, PERMANENT_AUTH_CREDENTIALS_INVALID, 7, 8),
        (_, _, PERMANENT_AUTH_MECHANISM_TOO_WEAK, 7, 9),
        (_, _, PERMANENT_ENCRYPTION_NEEDED, 7, 10),
        (_, _, PERMANENT_ENCRYPTION_REQUIRED_FOR_REQUESTED_AUTH_MECHANISM, 7, 11),
        (_, TRANSIENT_PASSWORD_TRANSITION_NEEDED, _, 7, 12),
        (_, _, PERMANENT_USER_ACCOUNT_DISABLED, 7, 13),
        (_, _, PERMANENT_TRUST_RELATIONSHIP_REQUIRED, 7, 14),
        (_, TRANSIENT_PRIORITY_TOO_LOW, PERMANENT_PRIORITY_TOO_LOW, 7, 15),
        (_, TRANSIENT_MESSAGE_TOO_BIG_FOR_PRIORITY, PERMANENT_MESSAGE_TOO_BIG_FOR_PRIORITY, 7, 16),
        (_, _, PERMANENT_MAILBOX_OWNER_HAS_CHANGED, 7, 17),
        (_, _, PERMANENT_DOMAIN_OWNER_HAS_CHANGED, 7, 18),
        (_, _, PERMANENT_RRVS_CANNOT_BE_COMPLETED, 7, 19),
        (_, _, PERMANENT_NO_PASSING_DKIM_SIGNATURE_FOUND, 7, 20),
        (_, _, PERMANENT_NO_ACCEPTABLE_DKIM_SIGNATURE_FOUND, 7, 21),
        (_, _, PERMANENT_NO_AUTHOR_MATCHED_DKIM_SIGNATURE_FOUND, 7, 22),
        (_, _, PERMANENT_SPF_VALIDATION_FAILED, 7, 23),
        (_, TRANSIENT_SPF_VALIDATION_ERROR, PERMANENT_SPF_VALIDATION_ERROR, 7, 24),
        (_, _, PERMANENT_REVERSE_DNS_VALIDATION_FAILED, 7, 25),
        (_, _, PERMANENT_MULTIPLE_AUTH_CHECKS_FAILED, 7, 26),
        (_, _, PERMANENT_SENDER_ADDRESS_HAS_NULL_MX, 7, 27),
        (SUCCESS_MAIL_FLOOD_DETECTED, TRANSIENT_MAIL_FLOOD_DETECTED, PERMANENT_MAIL_FLOOD_DETECTED, 7, 28),
        (_, _, PERMANENT_ARC_VALIDATION_FAILURE, 7, 29),
        (_, _, PERMANENT_REQUIRETLS_SUPPORT_REQUIRED, 7, 30),
    );
}

impl<S> EnhancedReplyCode<S> {
    pub fn parse<'a>(buf: &'a [u8]) -> IResult<&'a [u8], EnhancedReplyCode<S>>
    where
        S: From<&'a str>,
    {
        map(apply_regex(&EXTENDED_REPLY_CODE), |raw| {
            let class = raw[0] - b'0';
            let class = match class {
                2 => EnhancedReplyCodeClass::Success,
                4 => EnhancedReplyCodeClass::PersistentTransient,
                5 => EnhancedReplyCodeClass::PermanentFailure,
                _ => panic!("Regex allowed unexpected elements"),
            };
            let after_class = &raw[2..];
            // These unwrap and unsafe are OK thanks to the regex
            // already matching
            let second_dot = after_class.iter().position(|c| *c == b'.').unwrap();
            let raw_subject = unsafe { str::from_utf8_unchecked(&after_class[..second_dot]) }
                .parse()
                .unwrap();
            let raw_detail = unsafe { str::from_utf8_unchecked(&after_class[second_dot + 1..]) }
                .parse()
                .unwrap();
            let raw = unsafe { str::from_utf8_unchecked(raw) };
            EnhancedReplyCode {
                raw: raw.into(),
                class,
                raw_subject,
                raw_detail,
            }
        })(buf)
    }

    #[inline]
    pub fn subject(&self) -> EnhancedReplyCodeSubject {
        match self.raw_subject {
            1 => EnhancedReplyCodeSubject::Addressing,
            2 => EnhancedReplyCodeSubject::Mailbox,
            3 => EnhancedReplyCodeSubject::MailSystem,
            4 => EnhancedReplyCodeSubject::Network,
            5 => EnhancedReplyCodeSubject::MailDelivery,
            6 => EnhancedReplyCodeSubject::Content,
            7 => EnhancedReplyCodeSubject::Policy,
            _ => EnhancedReplyCodeSubject::Undefined,
        }
    }

    #[inline]
    pub fn into<T>(self) -> EnhancedReplyCode<T>
    where
        T: From<S>,
    {
        EnhancedReplyCode {
            raw: self.raw.into(),
            class: self.class,
            raw_subject: self.raw_subject,
            raw_detail: self.raw_detail,
        }
    }
}

impl<S> EnhancedReplyCode<S>
where
    S: AsRef<str>,
{
    #[inline]
    pub fn as_io_slices(&self) -> impl Iterator<Item = IoSlice> {
        iter::once(IoSlice::new(self.raw.as_ref().as_ref()))
    }
}

impl EnhancedReplyCode<&str> {
    pub fn to_owned(&self) -> EnhancedReplyCode<String> {
        EnhancedReplyCode {
            raw: self.raw.to_owned(),
            class: self.class,
            raw_subject: self.raw_subject,
            raw_detail: self.raw_detail,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplyLine<S> {
    pub code: ReplyCode,
    pub last: bool,
    pub ecode: Option<EnhancedReplyCode<S>>,
    pub text: MaybeUtf8<S>,
}

impl<S> ReplyLine<S> {
    pub fn parse<'a>(buf: &'a [u8]) -> IResult<&'a [u8], ReplyLine<S>>
    where
        S: From<&'a str>,
    {
        map(
            tuple((
                ReplyCode::parse,
                alt((value(false, tag(b"-")), value(true, opt(tag(b" "))))),
                opt(terminated(
                    EnhancedReplyCode::parse,
                    alt((tag(b" "), peek(tag(b"\r\n")))),
                )),
                alt((
                    map(
                        terminated(apply_regex(&REPLY_TEXT_ASCII), tag(b"\r\n")),
                        |b: &[u8]| {
                            // The below unsafe is OK, thanks to our
                            // regex validating that `b` is proper
                            // ascii (and thus utf-8)
                            let s = unsafe { str::from_utf8_unchecked(b) };
                            MaybeUtf8::Ascii(s.into())
                        },
                    ),
                    map(
                        terminated(apply_regex(&REPLY_TEXT_UTF8), tag(b"\r\n")),
                        |b: &[u8]| {
                            // The below unsafe is OK, thanks to our
                            // regex validating that `b` is proper
                            // utf8
                            let s = unsafe { str::from_utf8_unchecked(b) };
                            MaybeUtf8::Utf8(s.into())
                        },
                    ),
                )),
            )),
            |(code, last, ecode, text)| ReplyLine {
                code,
                last,
                ecode,
                text,
            },
        )(buf)
    }
}

#[inline]
fn line_as_io_slices<'a, S>(
    code: &'a ReplyCode,
    last: bool,
    ecode: &'a Option<EnhancedReplyCode<S>>,
    text: &'a MaybeUtf8<S>,
) -> impl 'a + Iterator<Item = IoSlice<'a>>
where
    S: AsRef<str>,
{
    let is_last_char = match last {
        true => b" ",
        false => b"-",
    };
    code.as_io_slices()
        .chain(iter::once(IoSlice::new(is_last_char)))
        .chain(
            ecode
                .iter()
                .flat_map(|c| c.as_io_slices().chain(iter::once(IoSlice::new(b" ")))),
        )
        .chain(text.as_io_slices())
        .chain(iter::once(IoSlice::new(b"\r\n")))
}

impl<S> ReplyLine<S>
where
    S: AsRef<str>,
{
    #[inline]
    pub fn as_io_slices(&self) -> impl Iterator<Item = IoSlice> {
        line_as_io_slices(&self.code, self.last, &self.ecode, &self.text)
    }
}

// TODO: use ascii crate for From<&'a AsciiStr> instead of From<&'a
// str> for the ascii variants

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Reply<S> {
    pub code: ReplyCode,
    pub ecode: Option<EnhancedReplyCode<S>>,
    // TODO: should we try to make constructing a constant reply noalloc?
    pub text: Vec<MaybeUtf8<S>>,
}

impl<S> Reply<S> {
    #[inline]
    pub fn parse<'a>(buf: &'a [u8]) -> IResult<&'a [u8], Reply<S>>
    where
        S: From<&'a str>,
    {
        // TODO: raise yellow flags if .code and .ecode are different
        // between the parsed reply lines
        map(
            pair(
                many0(preceded(
                    peek(pair(take(3usize), tag(b"-"))),
                    ReplyLine::parse,
                )),
                verify(ReplyLine::parse, |l| l.last),
            ),
            |(beg, end)| Reply {
                code: end.code,
                ecode: end.ecode,
                text: beg
                    .into_iter()
                    .map(|l| l.text)
                    .chain(iter::once(end.text))
                    .collect(),
            },
        )(buf)
    }
}

impl<S> Reply<S>
where
    S: AsRef<str>,
{
    #[inline]
    pub fn as_io_slices(&self) -> impl Iterator<Item = IoSlice> {
        let code = &self.code;
        let ecode = &self.ecode;
        let last_i = self.text.len() - 1;
        self.text
            .iter()
            .enumerate()
            .flat_map(move |(i, l)| line_as_io_slices(code, i == last_i, ecode, l))
    }
}

impl<S> fmt::Display for Reply<S>
where
    S: AsRef<str>,
{
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for s in self.as_io_slices() {
            write!(f, "{}", String::from_utf8_lossy(&s))?;
        }
        Ok(())
    }
}

impl Reply<&str> {
    #[inline]
    pub fn into_owned(self) -> Reply<String> {
        Reply {
            code: self.code,
            ecode: self.ecode.map(|c| c.to_owned()),
            text: self.text.into_iter().map(|l| l.to_owned()).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_code_valid() {
        let tests: &[(&[u8], [u8; 3])] = &[(b"523", *b"523"), (b"234", *b"234")];
        for (inp, out) in tests {
            println!("Test: {:?}", show_bytes(inp));
            let r = ReplyCode::parse(inp);
            println!("Result: {:?}", r);
            match r {
                Ok((rest, res)) => {
                    assert_eq!(rest, b"");
                    assert_eq!(res, ReplyCode(*out));
                }
                x => panic!("Unexpected result: {:?}", x),
            }
        }
    }

    #[test]
    fn reply_code_incomplete() {
        let tests: &[&[u8]] = &[b"3", b"43"];
        for inp in tests {
            let r = ReplyCode::parse(inp);
            println!("{:?}:  {:?}", show_bytes(inp), r);
            assert!(r.unwrap_err().is_incomplete());
        }
    }

    #[test]
    fn reply_code_invalid() {
        let tests: &[&[u8]] = &[b"foo", b"123", b"648"];
        for inp in tests {
            let r = ReplyCode::parse(inp);
            assert!(!r.unwrap_err().is_incomplete());
        }
    }

    // TODO: test reply code builder

    #[test]
    pub fn extended_reply_code_valid() {
        let tests: &[(&[u8], (EnhancedReplyCodeClass, u16, u16))] = &[
            (b"2.1.23", (EnhancedReplyCodeClass::Success, 1, 23)),
            (
                b"5.243.567",
                (EnhancedReplyCodeClass::PermanentFailure, 243, 567),
            ),
        ];
        for (inp, (class, raw_subject, raw_detail)) in tests.iter().cloned() {
            println!("Test: {:?}", show_bytes(inp));
            let r = EnhancedReplyCode::parse(inp);
            println!("Result: {:?}", r);
            match r {
                Ok((rest, res)) => {
                    assert_eq!(rest, b"");
                    assert_eq!(res, EnhancedReplyCode {
                        raw: str::from_utf8(inp).unwrap(),
                        class,
                        raw_subject,
                        raw_detail,
                    });
                }
                x => panic!("Unexpected result: {:?}", x),
            }
        }
    }

    #[test]
    fn extended_reply_code_incomplete() {
        let tests: &[&[u8]] = &[b"4.", b"5.23"];
        for inp in tests {
            let r = EnhancedReplyCode::<&str>::parse(inp);
            println!("{:?}:  {:?}", show_bytes(inp), r);
            assert!(r.unwrap_err().is_incomplete());
        }
    }

    #[test]
    fn extended_reply_code_invalid() {
        let tests: &[&[u8]] = &[b"foo", b"3.5.1", b"1.1000.2"];
        for inp in tests {
            let r = EnhancedReplyCode::<String>::parse(inp);
            assert!(!r.unwrap_err().is_incomplete());
        }
    }

    // TODO: test extended reply code builder

    #[test]
    fn reply_line_valid() {
        let tests: &[(&[u8], ReplyLine<&str>)] = &[
            (b"250 All is well\r\n", ReplyLine {
                code: ReplyCode(*b"250"),
                last: true,
                ecode: None,
                text: MaybeUtf8::Ascii("All is well"),
            }),
            (b"450-Temporary\r\n", ReplyLine {
                code: ReplyCode(*b"450"),
                last: false,
                ecode: None,
                text: MaybeUtf8::Ascii("Temporary"),
            }),
            (b"354 Please do start input now\r\n", ReplyLine {
                code: ReplyCode(*b"354"),
                last: true,
                ecode: None,
                text: MaybeUtf8::Ascii("Please do start input now"),
            }),
            (b"550 5.1.1 Mailbox does not exist\r\n", ReplyLine {
                code: ReplyCode(*b"550"),
                last: true,
                ecode: Some(EnhancedReplyCode::parse(b"5.1.1").unwrap().1),
                text: MaybeUtf8::Ascii("Mailbox does not exist"),
            }),
        ];
        for (inp, out) in tests.iter().cloned() {
            println!("Test: {:?}", show_bytes(inp));
            let r = ReplyLine::parse(inp);
            println!("Result: {:?}", r);
            match r {
                Ok((rest, res)) => {
                    assert_eq!(rest, b"");
                    assert_eq!(res, out);
                }
                x => panic!("Unexpected result: {:?}", x),
            }
        }
    }

    // TODO: test incomplete, invalid for ReplyLine

    #[test]
    fn reply_line_build() {
        let tests: &[(ReplyLine<&str>, &[u8])] = &[
            (
                ReplyLine {
                    code: ReplyCode::SERVICE_READY,
                    last: false,
                    ecode: None,
                    text: MaybeUtf8::Ascii("hello world!"),
                },
                b"220-hello world!\r\n",
            ),
            (
                ReplyLine {
                    code: ReplyCode::COMMAND_UNIMPLEMENTED,
                    last: true,
                    ecode: None,
                    text: MaybeUtf8::Ascii("test"),
                },
                b"502 test\r\n",
            ),
            (
                ReplyLine {
                    code: ReplyCode::MAILBOX_UNAVAILABLE,
                    last: true,
                    ecode: Some(EnhancedReplyCode::PERMANENT_BAD_DEST_MAILBOX),
                    text: MaybeUtf8::Utf8("mélbox does not exist"),
                },
                "550 5.1.1 mélbox does not exist\r\n".as_bytes(),
            ),
            (
                ReplyLine {
                    code: ReplyCode::USER_NOT_LOCAL,
                    last: false,
                    ecode: Some(EnhancedReplyCode::PERMANENT_DELIVERY_NOT_AUTHORIZED),
                    text: MaybeUtf8::Ascii("Forwarding is disabled"),
                },
                "551-5.7.1 Forwarding is disabled\r\n".as_bytes(),
            ),
        ];
        for (inp, out) in tests {
            println!("Test: {:?}", inp);
            let res = inp
                .as_io_slices()
                .flat_map(|s| s.iter().cloned().collect::<Vec<_>>().into_iter())
                .collect::<Vec<u8>>();
            println!("Result  : {:?}", show_bytes(&res));
            println!("Expected: {:?}", show_bytes(out));
            assert_eq!(&res, out);
        }
    }
}
