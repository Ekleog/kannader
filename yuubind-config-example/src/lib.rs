use smtp_message::{Email, EnhancedReplyCode, MaybeUtf8, Reply, ReplyCode};
use yuubind_config::server;

yuubind_config::allocator_implement_guest!();

yuubind_config::server_config_implement_guest!(mod server_config, ServerConfig);

struct ServerConfig;

impl server_config::WasmSide for ServerConfig {
    fn filter_from(
        from: Option<Email>,
        _meta: &mut server::MailMetadata,
        _conn_meta: &mut server::ConnectionMetadata,
    ) -> server::SerializableDecision<Option<Email>> {
        server::SerializableDecision::Accept {
            // TODO: this should be factored in some library (shared with the defaults of
            // smtp_server
            reply: Reply {
                code: ReplyCode::OKAY,
                ecode: Some(EnhancedReplyCode::SUCCESS_UNDEFINED.into()),
                text: vec![MaybeUtf8::Ascii("Okay".into())],
            },
            res: from,
        }
    }
}
