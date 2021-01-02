use yuubind_config::{server, Email};

yuubind_config::allocator_implement_guest!();

yuubind_config::server_config_implement_guest!(mod server_config, ServerConfig);

struct ServerConfig;

impl server_config::WasmSide for ServerConfig {
    // TODO: actually return a DecisionWithMessage for more flexibility?
    fn filter_from(
        _from: Option<Email>,
        _meta: &mut server::MailMetadata,
        _conn_meta: &mut server::ConnectionMetadata,
    ) -> server::SerializableDecision {
        server::SerializableDecision::Accept
    }
}
