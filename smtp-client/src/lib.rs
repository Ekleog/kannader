use std::{io, net::IpAddr};

use futures::AsyncRead;

use smtp_message::{Email, Hostname};

pub type Destination = IpAddr;

pub fn get_destination(host: &Hostname) -> io::Result<Destination> {
    let _ = host;
    todo!()
}

pub fn connect(dest: &Destination) -> io::Result<Sender> {
    let _ = dest;
    todo!()
}

#[derive(Debug, thiserror::Error)]
pub enum SendError {}

pub struct Sender {}

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
        let _ = (from, to, mail);
        todo!()
    }
}
