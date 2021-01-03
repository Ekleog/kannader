@0xb58685c8767cf62b;

using import "reply.capnp".Reply;
using Types = import "types.capnp";

struct Decision {
    reply @0 :Reply;

    union {
        accept @1 :Void;
        reject @2 :Void;
        kill @3 :Types.IoResult(Types.Unit);
    }
}