use std::fmt;

use helpers::*;

#[cfg_attr(test, derive(PartialEq))]
pub struct Reply<'a> {
    num: u16,
    lines: &'a[&'a [u8]]
}

macro_rules! reply_builder_function {
    ($code:tt, $fun:ident) => {
        pub fn $fun<'b>(lines: &'b [&'b [u8]]) -> Reply<'b> {
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
        let mut res = "[".to_owned();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_multiline() {
        let text: &[&[u8]] = &[b"hello", b"world", b"!"];
        let r = Reply::r220_service_ready(text);
        assert_eq!(r, Reply { num: 220, lines: text });
        assert_eq!(build(&r), b"220-hello\r\n220-world\r\n220 !\r\n");
    }

    #[test]
    fn reply_oneline() {
        let text: &[&[u8]] = &[b"test"];
        let r = Reply::r502_command_unimplemented(text);
        assert_eq!(r, Reply { num: 502, lines: text });
        assert_eq!(build(&r), b"502 test\r\n");
    }

    #[test]
    fn reply_codes() {
        assert_eq!(Reply::r211_system_status(&[]).num, 211);
        assert_eq!(Reply::r214_help_message(&[]).num, 214);
        assert_eq!(Reply::r220_service_ready(&[]).num, 220);
        assert_eq!(Reply::r221_closing_channel(&[]).num, 221);
        assert_eq!(Reply::r250_okay(&[]).num, 250);
        assert_eq!(Reply::r251_user_not_local_will_forward(&[]).num, 251);
        assert_eq!(Reply::r252_cannot_vrfy_but_please_try(&[]).num, 252);
        assert_eq!(Reply::r354_start_mail_input(&[]).num, 354);
        assert_eq!(Reply::r421_service_not_available(&[]).num, 421);
        assert_eq!(Reply::r450_mailbox_temporarily_unavailable(&[]).num, 450);
        assert_eq!(Reply::r451_local_error(&[]).num, 451);
        assert_eq!(Reply::r452_insufficient_storage(&[]).num, 452);
        assert_eq!(Reply::r455_unable_to_accept_parameters(&[]).num, 455);
        assert_eq!(Reply::r500_command_unrecognized(&[]).num, 500);
        assert_eq!(Reply::r501_syntax_error(&[]).num, 501);
        assert_eq!(Reply::r502_command_unimplemented(&[]).num, 502);
        assert_eq!(Reply::r503_bad_sequence(&[]).num, 503);
        assert_eq!(Reply::r504_parameter_unimplemented(&[]).num, 504);
        assert_eq!(Reply::r550_mailbox_unavailable(&[]).num, 550);
        assert_eq!(Reply::r550_policy_reason(&[]).num, 550);
        assert_eq!(Reply::r551_user_not_local(&[]).num, 551);
        assert_eq!(Reply::r552_exceeded_storage(&[]).num, 552);
        assert_eq!(Reply::r553_mailbox_name_incorrect(&[]).num, 553);
        assert_eq!(Reply::r554_transaction_failed(&[]).num, 554);
        assert_eq!(Reply::r555_mail_or_rcpt_parameter_unimplemented(&[]).num, 555);
    }
}
