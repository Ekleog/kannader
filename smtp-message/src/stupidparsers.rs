use byteslice::ByteSlice;

macro_rules! spaces {
    () => {
        " \t"
    };
}
macro_rules! alpha_lower {
    () => {
        "abcdefghijklmnopqrstuvwxyz"
    };
}
macro_rules! alpha_upper {
    () => {
        "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
    };
}
macro_rules! alpha {
    () => {
        concat!(alpha_lower!(), alpha_upper!())
    };
}
macro_rules! digit {
    () => {
        "0123456789"
    };
}
macro_rules! alnum {
    () => {
        concat!(alpha!(), digit!())
    };
}
macro_rules! atext {
    () => {
        concat!(alnum!(), "!#$%&'*+-/=?^_`{|}~")
    };
}
macro_rules! alnumdash {
    () => {
        concat!(alnum!(), "-")
    };
}
macro_rules! graph_except_equ {
    () => {
        concat!(alnum!(), "!\"#$%&'()*+,-./:;<>?@[\\]^_`{|}~")
    };
}

named!(pub eat_spaces(ByteSlice) -> ByteSlice, eat_separator!(spaces!()));
