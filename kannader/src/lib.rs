#![feature(core_intrinsics)]

// TODO: split into multiple processes, with multiple uids (stretch goal: do not
// require root and allow the user to directly call multiple executables and
// pass it the pre-opened sockets)

// TODO: make everything configurable, and actually implement the wasm scheme
// described in the docs

use std::{convert::TryFrom, io, path::PathBuf, sync::Arc, time::SystemTime};

use anyhow::Context;
use easy_parallel::Parallel;
use futures::StreamExt;
use scoped_tls::scoped_thread_local;
use smol::{future::FutureExt, unblock};
use tracing::{debug, info};

use smtp_queue_fs::FsStorage;

const NUM_THREADS: usize = 4;
const DATABUF_SIZE: usize = 16 * 1024;

mod client_config;
mod queue_config;
mod queue_transport;
mod server_config;
mod wasm_config;

use client_config::ClientConfig;
use queue_config::QueueConfig;
use queue_transport::QueueTransport;
use server_config::ServerConfig;
use wasm_config::WasmConfig;

#[derive(serde::Deserialize, serde::Serialize)]
pub struct Meta;

struct NoCertVerifier;

impl rustls::client::ServerCertVerifier for NoCertVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::Certificate,
        _intermediates: &[rustls::Certificate],
        _server_name: &rustls::client::ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: SystemTime,
    ) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::ServerCertVerified::assertion())
    }
}

scoped_thread_local!(static WASM_CONFIG: WasmConfig);

fn parse_dirs(s: &str) -> anyhow::Result<(PathBuf, PathBuf)> {
    let d = s.split("::").collect::<Vec<_>>();
    anyhow::ensure!(d.len() == 2, "invalid syntax");
    Ok((d[0].into(), d[1].into()))
}

#[derive(structopt::StructOpt)]
#[structopt(
    name = "kannader",
    about = "A highly configurable SMTP server written in Rust."
)]
pub struct Opt {
    /// Path to the wasm configuration blob
    #[structopt(
        short = "b",
        long,
        parse(from_os_str),
        default_value = "/etc/kannader/config.wasm"
    )]
    // TODO: have wasm configuration blobs pre-provided in /usr/lib or similar
    pub wasm_blob: PathBuf,

    /// Path to the configuration of the wasm configuration blob
    #[structopt(short, long, parse(from_os_str), default_value = "")]
    pub config: PathBuf,

    /// Directories to make available to the wasm configuration blob
    #[structopt(short, long = "dir", value_name = "GUEST_DIR::HOST_DIR", parse(try_from_str = parse_dirs))]
    pub dirs: Vec<(PathBuf, PathBuf)>,
}

pub fn run(opt: &Opt, shutdown: smol::channel::Receiver<()>) -> anyhow::Result<()> {
    info!("Kannader starting up");

    // TODO: get from listenfd (-> from Opt?)
    let listener =
        std::net::TcpListener::bind("0.0.0.0:2525").context("Binding on the listening port")?;

    // Load the configuration and run WasmConfig::new once to make sure errors are
    // caught early on. We can reuse this blob for the `.finish()` call.
    // TODO: limit the stack size, and make sure we always build with all
    // optimizations
    let engine = wasmtime::Engine::default();
    let module = wasmtime::Module::from_file(&engine, &opt.wasm_blob)
        .context("Compiling the wasm configuration blob")?;
    let wasm_config = WasmConfig::new(&opt.dirs, &opt.config, &engine, &module)
        .context("Preparing the wasm configuration blob")?;

    // Start the executor
    let ex = &Arc::new(smol::Executor::new());

    let (stop_signal, local_shutdown) = smol::channel::unbounded::<()>();

    let (_, res): (_, anyhow::Result<()>) = Parallel::new()
        .each(0..NUM_THREADS, |_| {
            let wasm_config = WasmConfig::new(&opt.dirs, &opt.config, &engine, &module)
                .context("Preparing the wasm configuration blob")?;
            WASM_CONFIG.set(&wasm_config, || {
                smol::block_on(ex.run(async {
                    shutdown
                        .recv()
                        .or(local_shutdown.recv())
                        .await
                        .context("Receiving shutdown notification")
                }))
            })
        })
        .finish(move || {
            let wasm_config = &wasm_config;
            WASM_CONFIG.set(wasm_config, move || {
                smol::block_on(async move {
                    // Prepare the clients
                    debug!("Preparing the client configuration");
                    // TODO: see for configuring persistence, for more performance?
                    let mut tls_client_cfg =
                        rustls::ClientConfig::builder()
                        .with_cipher_suites(&rustls::ALL_CIPHER_SUITES)
                        .with_kx_groups(&rustls::ALL_KX_GROUPS)
                        .with_protocol_versions(&rustls::ALL_VERSIONS)
                        .context("Configuring the rustls client")?
                        .with_custom_certificate_verifier(Arc::new(NoCertVerifier))
                        .with_no_client_auth();
                    let connector = tokio_rustls::TlsConnector::from(Arc::new(tls_client_cfg));
                    let client = smtp_client::Client::new(
                        async_std_resolver::resolver_from_system_conf()
                            .await
                            .context("Configuring a resolver from system configuration")?,
                        Arc::new(ClientConfig::new(connector)),
                    );

                    // Spawn the queue
                    debug!("Preparing the queue configuration");
                    let storage = (wasm_config.queue_config.storage_type)(&mut wasm_config.store)
                        .context("Retrieving storage type")?;
                    let storage = match storage {
                        kannader_types::QueueStorage::Fs(path) => FsStorage::new(Arc::new(path))
                            .await
                            .context("Opening the queue storage folder")?,
                    };
                    let queue = smtp_queue::Queue::new(
                        ex.clone(),
                        QueueConfig::new(),
                        storage,
                        QueueTransport::new(client),
                    )
                    .await;

                    // Spawn the server
                    // TODO: introduce some tests that make sure that starting kannader with an
                    // invalid config does result in a user-visible error
                    debug!("Preparing the TLS configuration");
                    let cert_file = (wasm_config.server_config.tls_cert_file)(&mut wasm_config.store)
                        .context("Getting the path to the TLS cert file")?;
                    let keys_file = (wasm_config.server_config.tls_key_file)(&mut wasm_config.store)
                        .context("Getting the path to the TLS key file")?;
                    let tls_server_cfg = unblock(move || {
                        // Load the certificates and keys
                        let certs = rustls_pemfile::certs(&mut io::BufReader::new(
                            std::fs::File::open(&cert_file).with_context(|| {
                                format!("Opening the certificate file ‘{}’", cert_file.display())
                            })?,
                        ))
                        .with_context(|| {
                            format!("Parsing the TLS certificate file ‘{}’", cert_file.display())
                        })?
                        .into_iter()
                        .map(rustls::Certificate)
                        .collect::<Vec<_>>();
                        debug!(num_certs = certs.len(), "Parsed certificates");

                        let keys = rustls_pemfile::pkcs8_private_keys(&mut io::BufReader::new(
                            std::fs::File::open(&keys_file).with_context(|| {
                                format!("Opening the key file ‘{}’", keys_file.display())
                            })?,
                        ))
                        .with_context(|| {
                            format!("Parsing the key file ‘{}’", keys_file.display())
                        })?;
                        debug!(num_keys = keys.len(), "Parsed keys");
                        anyhow::ensure!(
                            keys.len() == 1,
                            "Key file did not have just one key, but had {}",
                            keys.len()
                        );
                        let key = rustls::PrivateKey(keys.into_iter().next().unwrap());

                        // Configure rustls
                        // TODO: see for configuring persistence, for more performance?
                        // TODO: support SNI
                        let mut tls_server_cfg = rustls::ServerConfig::builder()
                            .with_cipher_suites(&rustls::ALL_CIPHER_SUITES)
                            .with_kx_groups(&rustls::ALL_KX_GROUPS)
                            .with_protocol_versions(&rustls::ALL_VERSIONS)
                            .context("Configuring the rustls server")?
                            .with_no_client_auth()
                            .with_single_cert(certs, key)
                            .context("Setting the key and certificates")?;

                        Ok(tls_server_cfg)
                    })
                    .await?;
                    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls_server_cfg));

                    debug!("Reopening the listener as async");
                    let server_cfg = Arc::new(ServerConfig::new(acceptor, queue));
                    let listener = smol::net::TcpListener::try_from(listener)
                        .context("Making listener async")?;
                    let mut incoming = listener.incoming();

                    info!("Server up, waiting for connections");
                    while let Some(stream) = incoming.next().await {
                        let stream = stream.context("Receiving a new incoming stream")?;
                        // TODO: attach uuid metadata to stream for logging purposes (or in
                        // smtp-server directly?)
                        tracing::trace!("New incoming stream");
                        ex.spawn(smtp_server::interact(
                            stream,
                            smtp_server::IsAlreadyTls::No,
                            Vec::new(), // TODO
                            server_cfg.clone(),
                        ))
                        .detach();
                    }

                    std::mem::drop(stop_signal);

                    Ok(())
                })
            })
        });

    res
}
