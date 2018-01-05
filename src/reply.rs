use std::{fmt, str};
use std::str::FromStr;

use helpers::*;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Clone)]
pub struct Reply<'a> {
    num: u16,
    lines: Vec<&'a [u8]>
}

macro_rules! reply_builder_function {
    ($code:tt, $fun:ident) => {
        pub fn $fun<'b>(lines: Vec<&'b [u8]>) -> Reply<'b> {
            Reply {
                num: $code,
                lines: lines,
            }
        }
    }
}

impl<'a> Reply<'a> {
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
        let mut res = "vec![".to_owned();
        for i in 0..self.lines.len() {
            res += &bytes_to_dbg(self.lines[i]);
            if i != self.lines.len() - 1 { res += ", " }
        }
        res += "]";
        write!(f, "Reply {{ num: {}, lines: {} }}", self.num, res)
    }
}

pub fn build(r: &Reply) -> Vec<u8> {
    let mut res = Vec::new();
    let code = &[((r.num % 1000) / 100) as u8 + b'0',
                 ((r.num % 100 ) / 10 ) as u8 + b'0',
                 ((r.num % 10  )      ) as u8 + b'0'];
    for i in 0..(r.lines.len() - 1) {
        res.extend_from_slice(code);
        res.push(b'-');
        res.extend_from_slice(r.lines[i]);
        res.extend_from_slice(b"\r\n");
    }
    res.extend_from_slice(code);
    res.push(b' ');
    if let Some(last) = r.lines.last() {
        res.extend_from_slice(last);
    }
    res.extend_from_slice(b"\r\n");
    res
}

named!(pub reply(&[u8]) -> Reply, do_parse!(
    num: verify!(
             map_res!(
                 map_res!(take!(3),
                          |bytes| str::from_utf8(bytes).map(|utf8| (bytes, utf8))),
                 |(bytes, utf8)| u16::from_str(utf8).map(|num| (bytes, num))),
             |(bytes, num)| num < 1000) >>
    lines: alt!(
        do_parse!(
            tag!(" ") >> line: take_until_and_consume!("\r\n") >>
            (vec![line])
        ) |
        do_parse!(
            tag!("-") >> first_line: take_until_and_consume!("\r\n") >>
            lines: many0!(do_parse!(
                tag!(num.0) >> tag!("-") >>
                line: take_until_and_consume!("\r\n") >>
                (line)
            )) >>
            tag!(num.0) >> tag!(" ") >> last_line: take_until_and_consume!("\r\n") >>
            ({
                let mut res = Vec::with_capacity(1 + lines.len() + 1);
                let mut mut_lines = lines;
                res.push(first_line);
                res.append(&mut mut_lines);
                res.push(last_line);
                res
            })
        )
    ) >>
    (Reply {
        num: num.1,
        lines,
    })
));

#[cfg(test)]
mod tests {
    use super::*;
    use nom::*;

    #[test]
    fn reply_multiline() {
        let text: Vec<&[u8]> = vec![b"hello", b"world", b"!"];
        let r = Reply::r220_service_ready(text.clone());
        assert_eq!(r, Reply { num: 220, lines: text });
        assert_eq!(build(&r), b"220-hello\r\n220-world\r\n220 !\r\n");
    }

    #[test]
    fn reply_oneline() {
        let text: Vec<&[u8]> = vec![b"test"];
        let r = Reply::r502_command_unimplemented(text.clone());
        assert_eq!(r, Reply { num: 502, lines: text });
        assert_eq!(build(&r), b"502 test\r\n");
    }

    #[test]
    fn reply_codes() {
        assert_eq!(Reply::r211_system_status(Vec::new()).num, 211);
        assert_eq!(Reply::r214_help_message(Vec::new()).num, 214);
        assert_eq!(Reply::r220_service_ready(Vec::new()).num, 220);
        assert_eq!(Reply::r221_closing_channel(Vec::new()).num, 221);
        assert_eq!(Reply::r250_okay(Vec::new()).num, 250);
        assert_eq!(Reply::r251_user_not_local_will_forward(Vec::new()).num, 251);
        assert_eq!(Reply::r252_cannot_vrfy_but_please_try(Vec::new()).num, 252);
        assert_eq!(Reply::r354_start_mail_input(Vec::new()).num, 354);
        assert_eq!(Reply::r421_service_not_available(Vec::new()).num, 421);
        assert_eq!(Reply::r450_mailbox_temporarily_unavailable(Vec::new()).num, 450);
        assert_eq!(Reply::r451_local_error(Vec::new()).num, 451);
        assert_eq!(Reply::r452_insufficient_storage(Vec::new()).num, 452);
        assert_eq!(Reply::r455_unable_to_accept_parameters(Vec::new()).num, 455);
        assert_eq!(Reply::r500_command_unrecognized(Vec::new()).num, 500);
        assert_eq!(Reply::r501_syntax_error(Vec::new()).num, 501);
        assert_eq!(Reply::r502_command_unimplemented(Vec::new()).num, 502);
        assert_eq!(Reply::r503_bad_sequence(Vec::new()).num, 503);
        assert_eq!(Reply::r504_parameter_unimplemented(Vec::new()).num, 504);
        assert_eq!(Reply::r550_mailbox_unavailable(Vec::new()).num, 550);
        assert_eq!(Reply::r550_policy_reason(Vec::new()).num, 550);
        assert_eq!(Reply::r551_user_not_local(Vec::new()).num, 551);
        assert_eq!(Reply::r552_exceeded_storage(Vec::new()).num, 552);
        assert_eq!(Reply::r553_mailbox_name_incorrect(Vec::new()).num, 553);
        assert_eq!(Reply::r554_transaction_failed(Vec::new()).num, 554);
        assert_eq!(Reply::r555_mail_or_rcpt_parameter_unimplemented(Vec::new()).num, 555);
    }

    #[test]
    fn parse_ok() {
        let tests: &[(&[u8], Reply)] = &[
            (b"250 All is well\r\n", Reply {
                num: 250,
                lines: vec![b"All is well"],
            }),
            (b"450-Temporary\r\n450 Failure\r\n", Reply {
                num: 450,
                lines: vec![b"Temporary", b"Failure"],
            }),
            (b"354-Please do\r\n354-Start\r\n354 input now\r\n", Reply {
                num: 354,
                lines: vec![b"Please do", b"Start", b"input now"],
            }),
            (b"550-Something\r\n550-Is\r\n550-Really\r\n550 Very wrong!\r\n", Reply {
                num: 550,
                lines: vec![b"Something", b"Is", b"Really", b"Very wrong!"],
            }),
        ];
        for test in tests {
            assert_eq!(reply(test.0), IResult::Done(&b""[..], test.1.clone()));
        }
    }
}
