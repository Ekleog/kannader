#[derive(serde::Deserialize, serde::Serialize)]
pub enum TlsHandler {
    Rustls,
}
