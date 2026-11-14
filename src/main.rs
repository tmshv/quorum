use std::net::SocketAddr;

use clap::Parser;

#[derive(Parser)]
#[command(name = "quorum", disable_version_flag = true)]
struct Args {
    /// Address to listen on.
    #[arg(long, default_value = "127.0.0.1:20481")]
    listen: SocketAddr,

    /// Print the version and exit.
    #[arg(long)]
    version: bool,
}

fn main() {
    let args = Args::parse();
    if args.version {
        println!("{}", quorum::VERSION);
        return;
    }
    // Server start comes in a later task.
    let _ = args.listen;
}
