use smtp_message::*;

use helpers::*;

// TODO: add new_mail called before filter_from
pub trait Config<U> {
    fn filter_from(&mut self, from: &Option<Email>, conn_meta: &ConnectionMetadata<U>) -> Decision;

    fn filter_to(
        &mut self,
        to: &Email,
        conn_meta: &ConnectionMetadata<U>,
        meta: &MailMetadata,
    ) -> Decision;
}
