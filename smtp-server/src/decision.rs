use smtp_message::{ReplyCode, SmtpString};

// TODO(low): make pub fields private?
// TODO(low): merge into Decision<T> once Reply is a thing
pub struct Refusal {
    pub code: ReplyCode,
    pub msg:  SmtpString,
}

pub enum Decision {
    Accept,
    Reject(Refusal),
}
