use structopt::StructOpt;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // TODO: figure out a better shutdown story than brutally killing the server
    // (ie. drop(signal) when the user wants to stop the server)
    let (_signal, shutdown) = smol::channel::unbounded::<()>();

    kannader::run(&kannader::Opt::from_args(), shutdown)
}
