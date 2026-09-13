//! Process entry: `wicket serve | migrate | db check | iq | manifest export`.

#![allow(unused_crate_dependencies)] // lib.rs is the graph; the binary parses CLI.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    wicket_server::run_cli()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
}
