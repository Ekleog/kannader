use smtp_message::{ReplyCode, SmtpString};

pub struct Refusal {
    pub code: ReplyCode,
    pub msg: SmtpString,
}

#[must_use]
pub enum Decision {
    Accept,
    Reject(Refusal),
}
