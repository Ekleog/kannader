use ::Command;

macro_rules! alpha_lower { () => ("abcdefghijklmnopqrstuvwxyz") }
macro_rules! alpha_upper { () => ("ABCDEFGHIJKLMNOPQRSTUVWXYZ") }
macro_rules! alpha       { () => (concat!(alpha_lower!(), alpha_upper!())) }
macro_rules! digit       { () => ("0123456789") }
macro_rules! alnum       { () => (concat!(alpha!(), digit!())) }

named!(hostname(&[u8]) -> &[u8], recognize!(
    separated_list_complete!(tag!("."), is_a!(concat!(alnum!(), "-")))
));

named!(email(&[u8]) -> &[u8], recognize!(do_parse!(
    take_until_and_consume!("@") >> hostname >> ()
)));

named!(address_in_path(&[u8]) -> &[u8], do_parse!(
    separated_list!(tag!(","), do_parse!(tag!("@") >> hostname >> ())) >>
    tag!(":") >>
    res: email >>
    (res)
));

named!(full_path(&[u8]) -> &[u8], recognize!(address_in_path));

#[cfg(test)]
mod tests {
    use nom::*;
    use parser::*;

    #[test]
    fn valid_hostnames() {
        let tests = &[
            &b"foo--bar"[..],
            &b"foo.bar.baz"[..],
        ];
        for test in tests {
            assert_eq!(hostname(test), IResult::Done(&b""[..], *test));
        }
    }

    #[test]
    fn valid_emails() {
        let tests = &[
            &b"t+e-s.t_i+n-g@foo.bar.baz"[..],
        ];
        for test in tests {
            assert_eq!(email(test), IResult::Done(&b""[..], *test));
        }
    }

    #[test]
    fn valid_full_paths() {
        let tests = &[
            &b"@foo.bar,@baz.quux:test@example.org"[..]
        ];
        for test in tests {
            assert_eq!(full_path(test), IResult::Done(&b""[..], *test));
        }
    }

    #[test]
    fn valid_addresses_in_paths() {
        let tests = &[
            (&b"@foo.bar,@baz.quux:test@example.org"[..], &b"test@example.org"[..]),
        ];
        for test in tests {
            assert_eq!(address_in_path(test.0), IResult::Done(&b""[..], test.1));
        }
    }
}
