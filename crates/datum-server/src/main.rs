//! Process entry. Prints version and exits; does not bind a port.

#![allow(unused_crate_dependencies)] // lib.rs is the graph; the binary only parses CLI.

use clap::Parser;

/// Command-line arguments.
#[derive(Parser, Debug)]
#[command(name = "datum-server", version)]
struct Args {
    /// Print version and exit (clap also provides --version).
    #[arg(long)]
    print_version: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let _ = args.print_version;
    println!("{}", datum_server::version());
    Ok(())
}
