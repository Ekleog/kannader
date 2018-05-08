use std::collections::HashMap;

use smtpstring::SmtpString;

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct SpParameters(pub HashMap<SmtpString, Option<SmtpString>>);
