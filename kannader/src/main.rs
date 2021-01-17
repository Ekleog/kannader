use structopt::StructOpt;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    kannader::run(&kannader::Opt::from_args())
}
