use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub output: Option<PathBuf>,
    #[serde(default)]
    pub download_url: Option<String>,
}

pub fn load(path: &Path) -> Result<Config> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Config::default()),
        Err(error) => {
            return Err(error).with_context(|| format!("reading config {}", path.display()));
        }
    };
    toml::from_str(&contents).with_context(|| format!("parsing config {}", path.display()))
}

impl Config {
    pub fn output_path(&self, config_path: &Path) -> Result<Option<PathBuf>> {
        let Some(output) = &self.output else {
            return Ok(None);
        };
        if output.as_os_str().is_empty() {
            bail!("config output path cannot be empty");
        }
        if output.is_absolute() {
            return Ok(Some(output.clone()));
        }
        let base = config_path.parent().unwrap_or_else(|| Path::new("."));
        Ok(Some(base.join(output)))
    }

    pub fn download_url(&self) -> Result<Option<&str>> {
        let Some(url) = &self.download_url else {
            return Ok(None);
        };
        let url = url.trim();
        if url.is_empty() {
            bail!("config download_url cannot be empty");
        }
        Ok(Some(url))
    }
}

pub fn write_output(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating output directory {}", parent.display()))?;
    }
    fs::write(path, contents).with_context(|| format!("writing install output {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{load, write_output};

    #[test]
    fn resolves_relative_output_from_config_directory_and_writes_it() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let config_path = temporary.path().join("config").join("config.toml");
        std::fs::create_dir_all(config_path.parent().expect("config parent"))
            .expect("create config directory");
        std::fs::write(
            &config_path,
            "output = \"generated/completion.ps1\"\ndownload_url = \"https://example.test/apophenia.db\"\n",
        )
            .expect("write config");

        let config = load(&config_path).expect("load config");
        assert_eq!(
            config.download_url().expect("read download URL"),
            Some("https://example.test/apophenia.db")
        );
        let output = config
            .output_path(&config_path)
            .expect("resolve output")
            .expect("configured output");
        assert_eq!(
            output,
            temporary
                .path()
                .join("config")
                .join("generated")
                .join("completion.ps1")
        );

        write_output(&output, "registration\n").expect("write output");
        assert_eq!(
            std::fs::read_to_string(output).expect("read output"),
            "registration\n"
        );
    }
}
