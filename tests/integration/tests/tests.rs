use std::{
    io,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::Context;
use assert_fs::prelude::*;
use async_trait::async_trait;
use futures::SinkExt;
use libtest_mimic::Test;
use netsim_embed::{Ipv4Range, NetworkBuilder};
use smol::{io::Cursor, prelude::*};

use smtp_message::{Email, EscapedDataReader, Hostname};
use smtp_server::{reply, ConnectionMetadata, Decision, MailMetadata};

const FORWARDER: &str = "../../target/wasm32-wasi/debug/forwarder.wasm";

struct TestSenderCfg(());

impl TestSenderCfg {
    fn new() -> TestSenderCfg {
        TestSenderCfg(())
    }
}

#[async_trait]
impl smtp_client::Config for TestSenderCfg {
    fn ehlo_hostname(&self) -> Hostname {
        Hostname::parse(b"sender.example.org").unwrap().1
    }

    fn can_do_tls(&self) -> bool {
        false
    }

    async fn tls_connect<IO>(&self, _: IO) -> io::Result<smtp_client::DynAsyncReadWrite>
    where
        IO: 'static + Unpin + Send + AsyncRead + AsyncWrite,
    {
        unimplemented!()
    }
}

struct TestReceiverCfg {
    mails: Arc<Mutex<Vec<(Option<Email>, Vec<Email>, Vec<u8>)>>>,
}

impl TestReceiverCfg {
    fn new() -> TestReceiverCfg {
        TestReceiverCfg {
            mails: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[async_trait]
impl smtp_server::Config for TestReceiverCfg {
    type ConnectionUserMeta = ();
    type MailUserMeta = ();
    type Protocol = smtp_server::protocol::Smtp;

    fn hostname(&self, _conn_meta: &ConnectionMetadata<()>) -> &str {
        "receiver.example.org".into()
    }

    async fn new_mail(&self, _conn_meta: &mut ConnectionMetadata<()>) {}

    fn can_do_tls(&self, _conn_meta: &ConnectionMetadata<()>) -> bool {
        false
    }

    async fn tls_accept<IO>(
        &self,
        _io: IO,
        _conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> io::Result<
        duplexify::Duplex<Pin<Box<dyn Send + AsyncRead>>, Pin<Box<dyn Send + AsyncWrite>>>,
    >
    where
        IO: 'static + Unpin + Send + AsyncRead + AsyncWrite,
    {
        unimplemented!()
    }

    async fn filter_from(
        &self,
        addr: Option<Email>,
        _meta: &mut MailMetadata<()>,
        _conn_meta: &mut ConnectionMetadata<()>,
    ) -> Decision<Option<Email>> {
        Decision::Accept {
            reply: reply::okay_from().convert(),
            res: addr,
        }
    }

    async fn filter_to(
        &self,
        email: Email,
        _meta: &mut MailMetadata<()>,
        _conn_meta: &mut ConnectionMetadata<()>,
    ) -> Decision<Email> {
        Decision::Accept {
            reply: reply::okay_to().convert(),
            res: email,
        }
    }

    async fn handle_mail<'contents, 'cfg, 'connmeta, 'resp, R>(
        &'cfg self,
        reader: &mut EscapedDataReader<'contents, R>,
        _meta: MailMetadata<Self::MailUserMeta>,
        _conn_meta: &'connmeta mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Decision<()>
    where
        R: Send + Unpin + AsyncRead,
    {
        let mut mail_text = Vec::new();
        let res = reader.read_to_end(&mut mail_text).await;
        assert!(reader.is_finished());
        reader.complete();
        if res.is_err() {
            panic!("Closed the channel too early");
        } else {
            self.mails
                .lock()
                .expect("failed to load mutex")
                .push((meta.from, meta.to, mail_text));
            Decision::Accept {
                reply: reply::okay_mail().convert(),
                res: (),
            }
        }
    }
}

fn basic_test() {
    let d = assert_fs::TempDir::new().expect("creating tempdir");
    d.child("cert.pem")
        .write_str(
            r#"
-----BEGIN CERTIFICATE-----
MIIEpzCCAo8CAgKaMA0GCSqGSIb3DQEBCwUAMBYxFDASBgNVBAMMC1NuYWtlb2ls
IENBMCAXDTE4MDcxMjAwMjIxOVoYDzIxMTgwNjE4MDAyMjE5WjAaMRgwFgYDVQQD
DA9sZXRzZW5jcnlwdC5vcmcwggIiMA0GCSqGSIb3DQEBAQUAA4ICDwAwggIKAoIC
AQDA++GXB6aA+Lr3X2xIPs/PqXoJF9TUb98NzQC+ww3+dOaIY0Omqf15/UID5G0u
0649zUsvISi9x+7vn9X7opKA7iTX0TYKsUIXCMQ5YMYXgOByVPfvnVYaYUmDM8R9
l+Fs6uZB9Bq7zorEC01lWn0XFwpu4vJD+w03F80wH7sgdtWHC/NVyd0elhkQ/qR2
fC80s/bZui5fmrdq/FZK2WfTW5nQF9SbhfDFpQ0hTu3QO/noUr0L2Fb0Blp7R2jQ
EkFZEXvFLAt4Mmqf25nP6xfRf+0hppJBIG5nJKPQd7lkrN1Q7S2nHbFNLYGuXdjY
E4lo4T3we5ta90qiyUu3hfatWszEZrzpKYXNzIMvVFKjBw/+u7LLnnmXul6kvmzh
yYyBEEFePm1eGpRNU6flY74kKUuT6u2+0BPiuiAmYyoQqYzDa3ss3LVb2uL/fvxA
b+LcAZosgv/saw9u5E7nVJ22LhAxV7bM0XN7vabM2aPVJeiRbLv5m+NQynnmgSNF
GxM9Rl004QclbLo6zbOe6ovtmkgcLEEbwUqeQJWJdEmoADxvNwSFH6ktqYsg4eW/
iR3MnqmEeRK8rryq9tFF5SY6MmBK6lUEj2OEr58drL9RZHjN8lB5330fxk6iJeWs
sJWz5U8PKdRQqvMXWdjKiU3OL81Lh7gBud9aDILkkpGmNQIDAQABMA0GCSqGSIb3
DQEBCwUAA4ICAQAkx3jcryukAuYP7PQxMy3LElOl65ZFVqxDtTDlr7DvAkWJzVCb
g08L6Tu+K0rKh2RbG/PqS0+8/jBgc4IwSOPfDDAX+sinfj0kwXG34WMzB0G3fQzU
2BMplJDOaBcNqHG8pLP1BG+9HAtR/RHe9p2Jw8LG2qmZs6uemPT/nCTNoyIL4oxh
UncjETV4ayCHDKD1XA7/icgddYsnfLQHWuIMuCrmQCHo0uQAd7qVHfUWZ+gcsZx0
jTNCcaI8OTS2S65Bjaq2HaM7GMcUYNUD2vSyNQeQbha4ZeyZ9bPyFzznPMmrPXQe
MJdkbJ009RQIG9As79En4m+l+/6zrdx4DNdROqaL6YNiSebWMnuFHpMW/rCnhrT/
HYadijHOiJJGj9tWSdC4XJs7fvZW3crMPUYxpOvl01xW2ZlgaekILi1FAjSMQVoV
NhWstdGCKJdthJqLL5MtNdfgihKcmgkJqKFXTkPv7sgAQCopu6X+S+srCgn856Lv
21haRWZa8Ml+E0L/ticT8Fd8Luysc6K9TJ4mT8ENC5ywvgDlEkwBD3yvINXm5lg1
xOIxv/Ye5gFk1knuM7OzpUFBrXUHdVVxflCUqNAhFPbcXwjgEQ+A+S5B0vI6Ohue
ZnR/wuiou6Y+Yzh8XfqL/3H18mGDdjyMXI1B6l4Judk000UVyr46cnI7mw==
-----END CERTIFICATE-----
            "#,
        )
        .expect("writing cert.pem");
    d.child("key.pem")
        .write_str(
            r#"
-----BEGIN PRIVATE KEY-----
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
Gvfq54FbDuB10XP8JdLrsy9Z6GEsmoE=
-----END PRIVATE KEY-----
            "#,
        )
        .expect("writing key.pem");
    let fwd = d.child("forwarder.toml");
    fwd.write_str(&format!(
        r#"
[queue]
path = "{0}/queue"

[server]
cert_path = "{0}/cert.pem"
key_path = "{0}/key.pem"
        "#,
        d.path().display(),
    ))
    .expect("writing forwarder.toml");

    let opt = kannader::Opt {
        wasm_blob: FORWARDER.into(),
        config: "/forwarder.toml".into(),
        dirs: vec![("/".into(), d.path().into())],
    };

    let (_signal, shutdown) = smol::channel::unbounded::<()>();

    let recv_cfg = Arc::new(TestReceiverCfg::new());
    let recv_cfg2 = recv_cfg.clone();

    // TODO: check that network code is optimal (as of writing this comment, running
    // this test with RUST_LOG=info,smtp_client=trace,kannader=trace shows
    // smtp_client always receiving replies in two batches, first the code and then
    // the rest, this could be a problem of either smtp-client or smtp-server)
    futures::executor::block_on(async move {
        let mut net = NetworkBuilder::<(), ()>::new(Ipv4Range::local_subnet_10());

        let last_recipient = net.spawn_machine(|_, mut evt| async move {
            let listener = smol::net::TcpListener::bind("0.0.0.0:25")
                .await
                .expect("Binding on the listening port");
            let mut incoming = listener.incoming();
            // We know only one message is incoming
            if let Some(stream) = incoming.next().await {
                let stream = stream.expect("receiving new incoming stream");
                smtp_server::interact(stream, smtp_server::IsAlreadyTls::No, (), recv_cfg2)
                    .await
                    .expect("Failed to receive mail");
            }
            evt.send(()).await.unwrap();
        });

        let kannader_server = net.spawn_machine(move |_, _| async move {
            kannader::run(&opt, shutdown).expect("Failed to run kannader");
        });

        let _initial_client = net.spawn_machine(move |_, _| async move {
            // Sleep to make sure that last_recipient and kannader have opened their socket
            smol::Timer::after(Duration::from_secs(1)).await;
            let client = smtp_client::Client::new(
                async_std_resolver::resolver_from_system_conf()
                    .await
                    .expect("Failed to configure resolver from system conf"),
                Arc::new(TestSenderCfg::new()),
            );
            let mut sender = client
                .connect_to_ip(kannader_server.into(), 2525)
                .await
                .expect("Failed to connect to kannader");
            sender
                .send(
                    Some(&Email::parse_bracketed(b"<foo@sender.example.org>").unwrap()),
                    &Email::parse_bracketed(format!("<bar@[{}]>", last_recipient).as_bytes())
                        .unwrap(),
                    Cursor::new(b"Hello, world!\r\n.\r\n"),
                )
                .await
                .expect("Failed sending the email");
        });

        net.spawn().machine(0).recv().await.unwrap();
    });

    let mails = recv_cfg.mails.lock().expect("getting mail lock").clone();
    assert_eq!(mails.len(), 1, "should have transmitted exactly one mail");
    assert_eq!(
        mails[0].0,
        Some(Email::parse_bracketed(b"<foo@sender.example.org>").unwrap()),
        "should have the right sender",
    );
    assert_eq!(mails[0].1.len(), 1, "should have one recipient");
    assert_eq!(
        mails[0].1[0].localpart.raw(),
        "bar",
        "should have the right recipient"
    );
    assert_eq!(
        mails[0].2, b"Hello, world!\r\n.\r\n",
        "should have the right (escaped) contents"
    )
}

pub fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // Initialize netsim by entering the userns
    netsim_embed_machine::namespace::unshare_user().context("Entering the user namespace")?;

    let args = libtest_mimic::Arguments::from_args();

    let success = std::process::Command::new("cargo")
        .arg("rustc")
        .args(&["-p", "kannader-config-forwarder"])
        .args(&["--target", "wasm32-wasi"])
        .arg("--")
        .args(&["-Z", "wasi-exec-model=reactor"])
        .spawn()
        .expect("Failed to start compiling wasm blob")
        .wait()
        .expect("Failed to wait for end of wasm blob compilation")
        .success();
    assert!(success, "Failed to compile wasm blob");

    let tests: Vec<Test<()>> = vec![Test::test("basic_test")];

    libtest_mimic::run_tests(&args, tests, |test| {
        match test.name.as_str() {
            "basic_test" => basic_test(),
            _ => panic!("Unknown test called"),
        }
        libtest_mimic::Outcome::Passed
    })
    .exit();
}
