use smtp_message::Email;

// TODO: (B) only provide methods to access these
pub struct MailMetadata {
    pub from: Option<Email>,
    pub to: Vec<Email>,
}

pub struct ConnectionMetadata<U> {
    pub user: U,
}
