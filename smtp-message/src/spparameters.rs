use std::collections::HashMap;

use byteslice::ByteSlice;
use smtpstring::SmtpString;
use stupidparsers::eat_spaces;

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct SpParameters(pub HashMap<SmtpString, Option<SmtpString>>);

named!(pub sp_parameters(ByteSlice) -> SpParameters, do_parse!(
    params: separated_nonempty_list_complete!(
        do_parse!(
            many1!(one_of!(" \t")) >>
            tag!("SP") >>
            many1!(one_of!(" \t")) >>
            ()
        ),
        do_parse!(
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
            (b"key=value", &[(b"key", Some(b"value"))]),
            (
                b"key=value SP key2=value2",
                &[(b"key", Some(b"value")), (b"key2", Some(b"value2"))],
            ),
            (
                b"KeY2=V4\"l\\u@e.z\tSP\t0tterkeyz=very_muchWh4t3ver",
                &[
                    (b"KeY2", Some(b"V4\"l\\u@e.z")),
                    (b"0tterkeyz", Some(b"very_muchWh4t3ver")),
                ],
            ),
            (b"NoValueKey", &[(b"NoValueKey", None)]),
            (b"A SP B", &[(b"A", None), (b"B", None)]),
            (
                b"A=B SP C SP D=SP",
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
