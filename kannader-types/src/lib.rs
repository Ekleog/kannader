use std::path::PathBuf;

#[derive(serde::Deserialize, serde::Serialize)]
pub enum TlsHandler {
    Rustls,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub enum QueueStorage {
    Fs(PathBuf),
}
