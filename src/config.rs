use std::{fs, sync::OnceLock, path::Path};

use color_eyre::eyre;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Config {
    pub qq: QQ,
    pub matrix: Matrix,
}

#[derive(Deserialize)]
pub struct QQ {
    /// QQ groups to bridge
    pub groups: Vec<i64>,
}

// TODO: store parsed versions
#[derive(Deserialize)]
pub struct Matrix {
    /// Target homeserver's name, like `matrix.org`
    pub homeserver_name: String,
    /// URL to connect to the homeserver, like `http://127.0.0.1:6517` or `https://matrix.org`
    pub homeserver_url: String,
}

pub static CONFIG: OnceLock<crate::config::Config> = OnceLock::new();

pub fn init<P: AsRef<Path>>(path: P) -> eyre::Result<()> {
    let config = toml::from_str(&fs::read_to_string(path)?)?;
    CONFIG
        .set(config)
        .map_err(|_| eyre::eyre!("Failed to initialize config"))
}
