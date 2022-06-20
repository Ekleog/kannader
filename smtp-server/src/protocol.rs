use std::pin::Pin;

use futures::StreamExt;

use smtp_server_types::Decision;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum ProtocolName {
    Smtp,
    Lmtp,
}

pub trait Protocol<'resp>: private::Sealed {
    const PROTOCOL: ProtocolName;

    type HandleMailReturnType;

    fn handle_mail_return_type_as_stream(
        _resp: Self::HandleMailReturnType,
    ) -> Pin<Box<dyn futures::Stream<Item = Decision<()>> + Send + 'resp>>;
}

pub struct Smtp;
impl<'resp> Protocol<'resp> for Smtp {
    type HandleMailReturnType = Decision<()>;

    const PROTOCOL: ProtocolName = ProtocolName::Smtp;

    fn handle_mail_return_type_as_stream(
        resp: Self::HandleMailReturnType,
    ) -> Pin<Box<dyn futures::Stream<Item = Decision<()>> + Send + 'resp>> {
        futures::stream::once(async move { resp }).boxed()
    }
}

pub struct Lmtp;
impl<'resp> Protocol<'resp> for Lmtp {
    type HandleMailReturnType = Pin<Box<dyn futures::Stream<Item = Decision<()>> + Send + 'resp>>;

    const PROTOCOL: ProtocolName = ProtocolName::Lmtp;

    fn handle_mail_return_type_as_stream(
        resp: Self::HandleMailReturnType,
    ) -> Pin<Box<dyn futures::Stream<Item = Decision<()>> + Send + 'resp>> {
        resp
    }
}

mod private {
    pub trait Sealed {}
    impl Sealed for super::Smtp {}
    impl Sealed for super::Lmtp {}
}
