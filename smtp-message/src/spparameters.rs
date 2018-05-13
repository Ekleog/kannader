use std::{collections::HashMap, io};

use byteslice::ByteSlice;
use sendable::Sendable;
use smtpstring::SmtpString;
use stupidparsers::eat_spaces;

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct SpParameters(pub HashMap<SmtpString, Option<SmtpString>>);

impl SpParameters {
    pub fn none() -> SpParameters {
        SpParameters(HashMap::new())
    }
}

impl Sendable for SpParameters {
    fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        for (k, v) in self.0.iter() {
            w.write_all(b" SP ")?;
            k.send_to(w)?;
            if let Some(v) = v {
                w.write_all(b"=")?;
                v.send_to(w)?;
            }
        }
        Ok(())
    }
}

named!(pub sp_parameters(ByteSlice) -> SpParameters, do_parse!(
    params: many0!(
        do_parse!(
            eat_spaces >>
            tag_no_case!("SP") >>
            eat_spaces >>
            key: recognize!(preceded!(one_of!(alnum!()), opt!(is_a!(alnumdash!())))) >>
            value: opt!(complete!(preceded!(tag!("="), is_a!(graph_except_equ!())))) >>
            (key.promote().into(), value.map(|x| x.promote().into()))
        )
    ) >>
    (SpParameters(params.into_iter().collect()))
));

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[test]
    fn valid_sp_parameters() {
        let tests: &[(&[u8], &[(&[u8], Option<&[u8]>)])] = &[
            (b"SP key=value", &[(b"key", Some(b"value"))]),
            (
                b"sp key=value SP key2=value2",
                &[(b"key", Some(b"value")), (b"key2", Some(b"value2"))],
            ),
            (
                b"sP KeY2=V4\"l\\u@e.z\tSP\t0tterkeyz=very_muchWh4t3ver",
                &[
                    (b"KeY2", Some(b"V4\"l\\u@e.z")),
                    (b"0tterkeyz", Some(b"very_muchWh4t3ver")),
                ],
            ),
            (b"Sp NoValueKey", &[(b"NoValueKey", None)]),
            (b"SP A SP B", &[(b"A", None), (b"B", None)]),
            (
                b"sp A=B SP C SP D=SP",
                &[(b"A", Some(b"B")), (b"C", None), (b"D", Some(b"SP"))],
            ),
        ];
        for (inp, out) in tests {
            let b = Bytes::from(*inp);
            let res = sp_parameters(ByteSlice::from(&b));
            let (rem, res) = res.unwrap();
            assert_eq!(&rem[..], b"");
            let res_reference = out.iter()
                .map(|(a, b)| ((*a).into(), b.map(|x| x.into())))
                .collect::<HashMap<_, _>>();
            assert_eq!(res.0, res_reference);
        }
    }
}
