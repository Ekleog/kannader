use smtp_message::Email;

pub struct MailMetadata {
    pub from: Option<Email>,
    pub to: Vec<Email>,
}

pub struct ConnectionMetadata<U> {
    pub user: U,
}
