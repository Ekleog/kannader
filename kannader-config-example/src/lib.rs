use std::path::PathBuf;

use kannader_config::server;
use smtp_message::{Email, EnhancedReplyCode, MaybeUtf8, Reply, ReplyCode};

struct Config;
kannader_config::implement_guest!(trait ConfigTrait, Config);
impl ConfigTrait for Config {
    fn setup(_path: PathBuf) -> Config {
        Config
    }
}

struct ServerConfig;
kannader_config::server_config_implement_guest!(Config, trait ServerConfigTrait, ServerConfig);
impl ServerConfigTrait for ServerConfig {
    fn filter_from(
        _cfg: &Config,
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
