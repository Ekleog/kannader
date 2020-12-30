fn main() {
    capnpc::CompilerCommand::new()
        .file("schema/decision.capnp")
        .file("schema/misc.capnp")
        .file("schema/reply.capnp")
        .file("schema/types.capnp")
        .run()
        .expect("compiling decision.capnp");
}
