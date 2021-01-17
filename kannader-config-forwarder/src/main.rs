use std::path::PathBuf;

use kannader_config::{queue, reply, server};
use smtp_message::{Email, Hostname, Reply};

trait ResultExt<T, E> {
    fn log_err<S>(self, s: impl FnOnce() -> S, f: impl FnOnce() -> T) -> T
    where
        S: std::fmt::Display;

    fn log_trap<S>(self, s: impl FnOnce() -> S) -> T
    where
        S: std::fmt::Display;
}

impl<T, E> ResultExt<T, E> for Result<T, E>
where
    E: 'static + Send + Sync + std::error::Error,
{
    fn log_err<S>(self, s: impl FnOnce() -> S, f: impl FnOnce() -> T) -> T
    where
        S: std::fmt::Display,
    {
        match self {
            Ok(r) => r,
            Err(e) => {
                kannader_config::error!("Error: {} {:?}", s(), anyhow::Error::new(e));
                f()
            }
        }
    }

    fn log_trap<S>(self, s: impl FnOnce() -> S) -> T
    where
        S: std::fmt::Display,
    {
        // TODO: improve on this panic! ?
        self.log_err(s, || panic!())
    }
}

#[derive(Debug, serde::Deserialize)]
struct Config {
    queue: QueueCfg,
    server: ServerCfg,
}

#[derive(Debug, serde::Deserialize)]
struct QueueCfg {
    path: PathBuf,
}

#[derive(Debug, serde::Deserialize)]
struct ServerCfg {
    cert_path: PathBuf,
    key_path: PathBuf,
}

impl kannader_config::Config for Config {
    fn setup(path: PathBuf) -> Config {
        kannader_config::info!("Reading config file…");
        let contents = std::fs::read_to_string(path.clone())
            .log_trap(|| format!("Reading file ‘{}’:", path.display()));
        toml::from_str(&contents).log_trap(|| format!("Parsing file ‘{}’ as toml:", path.display()))
    }
}

kannader_config::implement_guest!(Config);

struct ClientConfig;

impl kannader_config::ClientConfig for ClientConfig {
    type Cfg = Config;

    fn ehlo_hostname(_cfg: &Config) -> Hostname {
        // TODO: this should not be re-parsed on each attempt
        Hostname::parse(b"localhost").unwrap().1
    }
}

kannader_config::client_config_implement_guest_server!(ClientConfig);

struct QueueConfig;

impl kannader_config::QueueConfig for QueueConfig {
    type Cfg = Config;

    fn storage_type(cfg: &Config) -> kannader_types::QueueStorage {
        kannader_types::QueueStorage::Fs(cfg.queue.path.clone())
    }

    fn next_interval(_cfg: &Config, _schedule: queue::ScheduleInfo) -> Option<std::time::Duration> {
        // TODO: most definitely should try again
        // TODO: add bounce support to both transport and here
        None
    }
}

kannader_config::queue_config_implement_guest_server!(QueueConfig);

struct ServerConfig;

impl kannader_config::ServerConfig for ServerConfig {
    type Cfg = Config;

    fn tls_cert_file(cfg: &Config) -> PathBuf {
        cfg.server.cert_path.clone()
    }

    fn tls_key_file(cfg: &Config) -> PathBuf {
        cfg.server.key_path.clone()
    }

    fn welcome_banner_reply(_cfg: &Config, _conn_meta: &mut server::ConnMeta) -> Reply {
        reply::welcome_banner("localhost", "Service ready")
    }

    fn filter_hello(
        cfg: &Config,
        is_ehlo: bool,
        hostname: Hostname,
        conn_meta: &mut server::ConnMeta,
    ) -> server::SerializableDecision<server::HelloInfo> {
        let mut cm = conn_meta.clone();
        cm.hello = Some(server::HelloInfo {
            is_ehlo,
            hostname: hostname.clone(),
        });
        server::SerializableDecision::Accept {
            reply: reply::okay_hello(is_ehlo, "localhost", "", Self::can_do_tls(cfg, cm)).convert(),
            res: server::HelloInfo { is_ehlo, hostname },
        }
    }

    fn new_mail(_cfg: &Config, _conn_meta: &mut server::ConnMeta) -> Vec<u8> {
        Vec::new()
    }

    fn filter_from(
        _cfg: &Config,
        from: Option<Email>,
        _meta: &mut server::MailMeta,
        _conn_meta: &mut server::ConnMeta,
    ) -> server::SerializableDecision<Option<Email>> {
        server::SerializableDecision::Accept {
            reply: reply::okay_from().convert(),
            res: from,
        }
    }

    fn filter_to(
        _cfg: &Config,
        to: Email,
        _meta: &mut server::MailMeta,
        _conn_meta: &mut server::ConnMeta,
    ) -> server::SerializableDecision<Email> {
        // TODO TODO THIS IS BAD
        server::SerializableDecision::Accept {
            reply: reply::okay_to().convert(),
            res: to,
        }
    }
}

kannader_config::server_config_implement_guest_server!(ServerConfig);
