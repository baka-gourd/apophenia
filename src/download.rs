use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result, bail};
use tempfile::NamedTempFile;

/// The published database on the project's jsDelivr GitHub endpoint.
pub const DEFAULT_DATABASE_URL: &str =
    "https://cdn.jsdelivr.net/gh/baka-gourd/apophenia@release/database/apophenia.db";

const SQLITE_HEADER: &[u8] = b"SQLite format 3\0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadedDatabase {
    pub bytes: u64,
}

pub async fn download_database(url: &str, destination: &Path) -> Result<DownloadedDatabase> {
    let client = reqwest::Client::builder()
        .user_agent(format!("apophenia/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .context("creating HTTP client")?;
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("downloading database from {url}"))?;
    let status = response.status();
    if !status.is_success() {
        bail!("downloading database from {url} returned HTTP {status}");
    }
    let body = response
        .bytes()
        .await
        .with_context(|| format!("reading database response from {url}"))?;
    install_database_bytes(destination, &body)
}

fn install_database_bytes(destination: &Path, body: &[u8]) -> Result<DownloadedDatabase> {
    if !body.starts_with(SQLITE_HEADER) {
        bail!("downloaded database does not start with the SQLite file signature");
    }
    let Some(file_name) = destination.file_name() else {
        bail!("download destination must include a file name");
    };
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .with_context(|| format!("creating download directory {}", parent.display()))?;

    let mut temporary = NamedTempFile::new_in(parent)
        .with_context(|| format!("creating temporary database in {}", parent.display()))?;
    temporary
        .write_all(body)
        .with_context(|| format!("writing temporary database in {}", parent.display()))?;
    temporary
        .as_file()
        .sync_all()
        .with_context(|| format!("flushing temporary database in {}", parent.display()))?;

    if destination.exists() {
        fs::remove_file(destination)
            .with_context(|| format!("replacing existing database {}", destination.display()))?;
    }
    temporary
        .persist(parent.join(file_name))
        .map_err(|error| error.error)
        .with_context(|| format!("installing database at {}", destination.display()))?;

    Ok(DownloadedDatabase {
        bytes: body.len() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::install_database_bytes;

    #[test]
    fn validates_and_replaces_sqlite_database() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let destination = temporary.path().join("nested").join("apophenia.db");
        let first = b"SQLite format 3\0first";
        let second = b"SQLite format 3\0second";

        assert_eq!(
            install_database_bytes(&destination, first)
                .expect("install first database")
                .bytes,
            first.len() as u64
        );
        assert_eq!(
            std::fs::read(&destination).expect("read first database"),
            first
        );

        install_database_bytes(&destination, second).expect("replace database");
        assert_eq!(
            std::fs::read(&destination).expect("read second database"),
            second
        );
    }

    #[test]
    fn rejects_non_sqlite_response() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let destination = temporary.path().join("apophenia.db");
        let error = install_database_bytes(&destination, b"not sqlite")
            .expect_err("invalid database should fail");
        assert!(error.to_string().contains("SQLite file signature"));
        assert!(!destination.exists());
    }
}
