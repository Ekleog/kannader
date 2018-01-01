macro_rules! alpha_lower { () => ("abcdefghijklmnopqrstuvwxyz") }
macro_rules! alpha_upper { () => ("ABCDEFGHIJKLMNOPQRSTUVWXYZ") }
macro_rules! alpha       { () => (concat!(alpha_lower!(), alpha_upper!())) }
macro_rules! digit       { () => ("0123456789") }
macro_rules! alnum       { () => (concat!(alpha!(), digit!())) }
macro_rules! atext       { () => (concat!(alnum!(), "!#$%&'*+-/=?^_`{|}~")) }

// TODO: strip return-path in MAIL FROM, like OpenSMTPD does, in order to not be thrown out by mail
// systems like orange's, maybe?
// TODO: RFC5321 §4.1.2 for local-part all quoted forms MUST be treated as equivalent

named!(pub hostname(&[u8]) -> &[u8],
    alt!(
        recognize!(preceded!(tag!("["), take_until_and_consume!("]"))) |
        recognize!(separated_list_complete!(tag!("."), is_a!(concat!(alnum!(), "-"))))
    )
);

named!(dot_string(&[u8]) -> &[u8], recognize!(
    separated_list!(tag!("."), is_a!(atext!()))
));

// See RFC 5321 § 4.1.2
named!(quoted_string(&[u8]) -> &[u8], recognize!(do_parse!(
    tag!("\"") >>
    many0!(alt!(
        preceded!(tag!("\\"), verify!(take!(1), |x: &[u8]| 32 <= x[0] && x[0] <= 126)) |
        verify!(take!(1), |x: &[u8]| 32 <= x[0] && x[0] != 34 && x[0] != 92 && x[0] <= 126)
    )) >>
    tag!("\"") >>
    ()
)));

named!(localpart(&[u8]) -> &[u8], alt!(quoted_string | dot_string));

named!(email(&[u8]) -> &[u8], recognize!(do_parse!(
    localpart >> tag!("@") >> hostname >> ()
)));

named!(address_in_path(&[u8]) -> &[u8], do_parse!(
    opt!(do_parse!(
        separated_list!(tag!(","), do_parse!(tag!("@") >> hostname >> ())) >>
        tag!(":") >>
        ()
    )) >>
    res: email >>
    (res)
));

named!(pub address_in_maybe_bracketed_path(&[u8]) -> &[u8],
    alt!(
        do_parse!(
            tag!("<") >>
            addr: address_in_path >>
            tag!(">") >>
            (addr)
        ) |
        address_in_path
    )
);

named!(pub full_maybe_bracketed_path(&[u8]) -> &[u8], recognize!(address_in_maybe_bracketed_path));

named!(pub eat_spaces, eat_separator!(" \t"));

#[cfg(test)]
mod tests {
    use nom::*;
    use parse_helpers::*;

    #[test]
    fn valid_hostnames() {
        let tests = &[
            &b"foo--bar"[..],
            &b"foo.bar.baz"[..],
            &b"1.2.3.4"[..],
            &b"[123.255.37.2]"[..],
            &b"[IPv6:0::ffff:8.7.6.5]"[..],
        ];
        for test in tests {
            assert_eq!(hostname(test), IResult::Done(&b""[..], *test));
        }
    }

    #[test]
    fn valid_dot_strings() {
        let tests: &[&[u8]] = &[
            // Adding an '@' so that tests do not return Incomplete
            b"helloooo@",
            b"test.ing@",
        ];
        for test in tests {
            assert_eq!(dot_string(test), IResult::Done(&b"@"[..], &test[0..test.len()-1]));
        }
    }

    #[test]
    fn valid_quoted_strings() {
        let tests: &[&[u8]] = &[
            br#""hello""#,
            br#""hello world. This |$ a g#eat place to experiment !""#,
            br#""\"escapes\", useless like h\ere, except for quotes and \\backslashes""#,
        ];
        for test in tests {
            assert_eq!(quoted_string(test), IResult::Done(&b""[..], *test));
        }
    }

    #[test]
    fn valid_emails() {
        let tests: &[&[u8]] = &[
            b"t+e-s.t_i+n-g@foo.bar.baz",
            br#""quoted\"example"@example.org"#,
        ];
        for test in tests {
            assert_eq!(email(test), IResult::Done(&b""[..], *test));
        }
    }

    #[test]
    fn valid_addresses_in_paths() {
        let tests = &[
            (&b"@foo.bar,@baz.quux:test@example.org"[..], &b"test@example.org"[..]),
            (&b"foo.bar@baz.quux"[..], &b"foo.bar@baz.quux"[..]),
        ];
        for test in tests {
            assert_eq!(address_in_path(test.0), IResult::Done(&b""[..], test.1));
        }
    }

    #[test]
    fn valid_addresses_in_maybe_bracketed_paths() {
        let tests = &[
            (&b"@foo.bar,@baz.quux:test@example.org"[..], &b"test@example.org"[..]),
            (&b"<@foo.bar,@baz.quux:test@example.org>"[..], &b"test@example.org"[..]),
            (&b"<foo@bar.baz>"[..], &b"foo@bar.baz"[..]),
            (&b"foo@bar.baz"[..], &b"foo@bar.baz"[..]),
        ];
        for test in tests {
            assert_eq!(address_in_maybe_bracketed_path(test.0), IResult::Done(&b""[..], test.1));
        }
    }

    #[test]
    fn valid_full_maybe_bracketed_paths() {
        let tests = &[
            &b"@foo.bar,@baz.quux:test@example.org"[..],
            &b"<@foo.bar,@baz.quux:test@example.org>"[..],
            &b"foo@bar.baz"[..],
            &b"<foo@bar.baz>"[..],
        ];
        for test in tests {
            assert_eq!(full_maybe_bracketed_path(test), IResult::Done(&b""[..], *test));
        }
    }
}
