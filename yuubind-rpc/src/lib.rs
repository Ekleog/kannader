pub mod decision_capnp {
    include!(concat!(env!("OUT_DIR"), "/schema/decision_capnp.rs"));
}

pub mod misc_capnp {
    include!(concat!(env!("OUT_DIR"), "/schema/misc_capnp.rs"));
}

pub mod reply_capnp {
    include!(concat!(env!("OUT_DIR"), "/schema/reply_capnp.rs"));
}

pub mod types_capnp {
    include!(concat!(env!("OUT_DIR"), "/schema/types_capnp.rs"));
}
