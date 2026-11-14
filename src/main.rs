use std::net::SocketAddr;
use std::sync::Arc;

use clap::Parser;
use tokio::net::TcpListener;
use tokio::signal::unix::{SignalKind, signal};
use tracing_subscriber::EnvFilter;

use quorum::hub::Hub;
use quorum::web::Embedded;

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

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args = Args::parse();
    if args.version {
        println!("{}", quorum::VERSION);
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .init();

    let listener = TcpListener::bind(args.listen).await?;
    // Printed unconditionally: the default log level hides `info!`.
    eprintln!(
        "quorum {} listening on {}",
        quorum::VERSION,
        listener.local_addr()?
    );

    let shutdown = async {
        let mut term = signal(SignalKind::terminate()).expect("install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    };
    quorum::serve(listener, Hub::new(), Arc::new(Embedded), shutdown).await
}
