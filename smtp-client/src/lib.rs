use std::{io, net::IpAddr, pin::Pin};

use futures::{AsyncRead, AsyncWrite};
use trust_dns_resolver::AsyncResolver;

use smtp_message::{Email, Hostname};

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

    pub fn get_destination(&self, host: &Hostname) -> io::Result<Destination> {
        // TODO: already resolve here, but that means having to handle DNS expiration
        // down the road
        Ok(Destination { host: host.clone() })
    }

    pub fn connect(&self, dest: &Destination) -> io::Result<Sender> {
        match dest.host {
            Hostname::Ipv4 { ip, .. } => self.connect_ip(IpAddr::V4(ip)),
            Hostname::Ipv6 { ip, .. } => self.connect_ip(IpAddr::V6(ip)),
            Hostname::AsciiDomain { ref raw } => self.connect_host(&raw),
            Hostname::Utf8Domain { ref punycode, .. } => self.connect_host(&punycode),
        }
    }

    fn connect_host(&self, host: &str) -> io::Result<Sender> {
        let _ = host;
        todo!()
    }

    fn connect_ip(&self, ip: IpAddr) -> io::Result<Sender> {
        let _ = (ip, &self.resolver);
        todo!()
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
