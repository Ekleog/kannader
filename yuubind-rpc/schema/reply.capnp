@0x8521811898e814e1;

using import "types.capnp".Option;

struct Reply {
    code @0 :UInt8;
    ecode @1 :Option(EnhancedReplyCode);
    text @2 :List(Text);
}

struct EnhancedReplyCode {
    class @0 :UInt8;
    subject @1 :UInt16;
    detail @2 :UInt16;
}