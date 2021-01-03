@0x868b81c7141d6f25;

# Like Void except it's a pointer type for use in generics
struct Unit {}

struct Bool {
    union {
        true @0 :Void;
        false @1 :Void;
    }
}

struct Option(T) {
    union {
        some @0 :T;
        none @1 :Void;
    }
}

struct IoResult(T) {
    union {
        ok @0 :T;
        err @1 :Text;
    }
}