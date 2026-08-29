use std::path::PathBuf;

use anyhow::{Result, anyhow};
use directories::BaseDirs;

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub data_root: PathBuf,
    pub config_root: PathBuf,
    pub database: PathBuf,
    pub config: PathBuf,
}

pub fn app_paths() -> Result<AppPaths> {
    let (data_root, config_root) = if cfg!(windows) {
        let dirs =
            BaseDirs::new().ok_or_else(|| anyhow!("cannot determine Windows Known Folders"))?;
        (
            dirs.data_local_dir().join("Apophenia"),
            dirs.config_dir().join("Apophenia"),
        )
    } else {
        let root = BaseDirs::new()
            .map(|dirs| dirs.home_dir().join(".apophenia"))
            .ok_or_else(|| anyhow!("cannot determine the user home directory"))?
            .to_owned();
        (root.clone(), root)
    };
    Ok(AppPaths {
        database: data_root.join("apophenia.db"),
        config: config_root.join("config.toml"),
        data_root,
        config_root,
    })
}
