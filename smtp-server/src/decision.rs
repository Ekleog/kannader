use smtp_message::{ReplyCode, SmtpString};

// TODO: (B) merge into Decision<T> id:J6HX
pub struct Refusal {
    pub code: ReplyCode,
    pub msg: SmtpString,
}

#[must_use]
pub enum Decision {
    Accept,
    Reject(Refusal),
    // TODO: add Kill option to cut the connection
}
