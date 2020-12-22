use std::{collections::BTreeMap, io, net::IpAddr, pin::Pin};

use futures::{AsyncRead, AsyncReadExt, AsyncWrite};
use rand::prelude::SliceRandom;
use smol::net::TcpStream;
use trust_dns_resolver::{AsyncResolver, IntoName};

use smtp_message::{Email, Hostname};

const SMTP_PORT: u16 = 25;

pub struct Destination {
    host: Hostname,
}

pub struct Client<C, P>
where
    C: trust_dns_resolver::proto::DnsHandle,
    P: trust_dns_resolver::ConnectionProvider<Conn = C>,
{
    resolver: AsyncResolver<C, P>,
}

impl<C, P> Client<C, P>
where
    C: trust_dns_resolver::proto::DnsHandle,
    P: trust_dns_resolver::ConnectionProvider<Conn = C>,
{
    pub fn new(resolver: AsyncResolver<C, P>) -> Client<C, P> {
        Client { resolver }
    }

    pub async fn get_destination(&self, host: &Hostname) -> io::Result<Destination> {
        // TODO: already resolve here, but that means having to handle DNS expiration
        // down the road
        Ok(Destination { host: host.clone() })
    }

    pub async fn connect(&self, dest: &Destination) -> io::Result<Sender> {
        match dest.host {
            Hostname::Ipv4 { ip, .. } => self.connect_to_ip(IpAddr::V4(ip)).await,
            Hostname::Ipv6 { ip, .. } => self.connect_to_ip(IpAddr::V6(ip)).await,
            Hostname::AsciiDomain { ref raw } => self.connect_to_mx(&raw).await,
            Hostname::Utf8Domain { ref punycode, .. } => self.connect_to_mx(&punycode).await,
        }
    }

    pub async fn connect_to_mx(&self, host: &str) -> io::Result<Sender> {
        // TODO: consider adding a `.` at the end of `host`... but is it
        // actually allowed?
        // Run MX lookup
        let lookup = self.resolver.mx_lookup(host).await?;

        // Retrieve the actual records
        let mut mx_records = BTreeMap::new();
        for record in lookup.iter() {
            mx_records
                .entry(record.preference())
                .or_insert(Vec::with_capacity(1))
                .push(record.exchange());
        }

        // If there are no MX records, try A/AAAA records
        if mx_records.is_empty() {
            return self.connect_to_host(&host.into_name()?).await;
        }

        // By increasing order of priority, try each MX
        let mut first_error = None;
        for (_, mut mxes) in mx_records {
            // Among a single priority level, randomize the order
            // TODO: consider giving a way to seed for reproducibility?
            mxes.shuffle(&mut rand::thread_rng());

            // Then try to connect to each address
            for mx in mxes {
                match self.connect_to_host(mx).await {
                    Ok(sender) => return Ok(sender),
                    Err(e) => first_error = first_error.or(Some(e)),
                }
            }
        }

        // The below unwrap is safe because, to reach it:
        // - there must be some MX records or we'd have returned in the if above
        // - there have been no error as otherwise first_error wouldn't be None
        // - there must have only be errors as otherwise we'd have returned in the match
        //   above
        // Hence, if it triggers it means that \exists N, N > 1 \wedge N = 0, where N is
        // the number of errors.
        //   QED.
        Err(first_error.unwrap())
    }

    async fn connect_to_host(&self, name: &trust_dns_resolver::Name) -> io::Result<Sender> {
        let _ = name;
        todo!()
    }

    pub async fn connect_to_ip(&self, ip: IpAddr) -> io::Result<Sender> {
        let io = TcpStream::connect((ip, SMTP_PORT)).await?;
        let (reader, writer) = io.split();
        Ok(Sender {
            reader: Box::pin(reader),
            writer: Box::pin(writer),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SendError {}

pub struct Sender {
    reader: Pin<Box<dyn Send + AsyncRead>>,
    writer: Pin<Box<dyn Send + AsyncWrite>>,
}

impl Sender {
    // TODO: Figure out a way to batch a single mail (with the same metadata) going
    // out to multiple recipients, so as to just use multiple RCPT TO
    pub async fn send<Reader>(
        &self,
        from: Option<&Email>,
        to: &Email,
        mail: Reader,
    ) -> Result<(), SendError>
    where
        Reader: AsyncRead,
    {
        let _ = (from, to, mail, &self.reader, &self.writer);
        todo!()
    }
}

// TODO: add tests
