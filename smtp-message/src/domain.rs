use bytes::Bytes;
use std::ops::Deref;

use byteslice::ByteSlice;
use parseresult::{nom_to_result, ParseError};
use smtpstring::SmtpString;

use parse_helpers::*;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Clone, Debug)]
pub struct Domain(SmtpString); // TODO: split between IP and DNS

impl Domain {
    pub fn new(domain: ByteSlice) -> Result<Domain, ParseError> {
        nom_to_result(hostname(domain))
    }

    pub fn parse_slice(b: &[u8]) -> Result<Domain, ParseError> {
        let b = Bytes::from(b);
        nom_to_result(hostname(ByteSlice::from(&b)))
    }

    pub fn as_string(&self) -> &SmtpString {
        &self.0
    }
}

impl Deref for Domain {
    type Target = SmtpString;

    fn deref(&self) -> &SmtpString {
        &self.0
    }
}

pub fn new_domain_unchecked(s: SmtpString) -> Domain {
    Domain(s)
}
