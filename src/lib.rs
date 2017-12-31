mod parser;

// TODO: transform all CR or LF to CRLF
// TODO: return "500 syntax error - invalid character" if receiving a non-ASCII character in
// envelope commands
// TODO: escape initial '.' in DataItem by adding another '.' in front (and opposite when
// receiving)

pub struct MailCommand<'a> {
    from: &'a [u8],
}

pub struct RcptCommand<'a> {
    // TO: parameter with the “@ONE,@TWO:” portion removed, as per RFC5321 Appendix C
    to: &'a [u8],
}

pub enum Command<'a> {
    Mail(MailCommand<'a>), // MAIL FROM:<@ONE,@TWO:JOE@THREE> [SP <mail-parameters>] <CRLF>
    Rcpt(RcptCommand<'a>), // RCPT TO:<@ONE,@TWO:JOE@THREE> [SP <rcpt-parameters] <CRLF>
    Data,                  // DATA <CRLF>
    DataItem(&'a [u8]),    // Data item between DATA and end of mail data indicator
    EndOfData,             // . <CRLF>
}

pub struct Reply {
    code: u16,
    text: &[u8],
}
