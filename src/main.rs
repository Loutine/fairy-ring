use std::env;

use color_eyre::eyre;
use fairy_ring::{matrix, qq};

#[tokio::main]
async fn main() -> eyre::Result<()> {
    env::set_var(
        "RUST_LOG",
        "matrix_sdk=debug,matrix_sdk_appservice=debug,fairy_ring=debug",
    );
    color_eyre::install()?;
    tracing_subscriber::fmt::init();

    fairy_ring::config::init("config.toml")?;

    let matrix_appservice = matrix::new_appservice().await?;
    let qq_client = qq::new_client(matrix_appservice.clone()).await?;

    let matrix_handle = {
        let qq_client = qq_client.clone();
        tokio::spawn(matrix::run_appservice(matrix_appservice, qq_client))
    };

    let qq_handle = tokio::spawn(qq::run_client(qq_client.clone()));

    tokio::select! {
        e = matrix_handle => e,
        e = qq_handle => e,
    }??;

    Ok(())
}
