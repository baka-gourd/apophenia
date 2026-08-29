# Apophenia

Apophenia is a third-party tools to manage and complete CLI commands.

## Getting Started

[Download](https://github.com/baka-gourd/apophenia/releases) or build Apophenia from the source repository.

```sh
git clone https://github.com/baka-gourd/apophenia.git
cd apophenia
cargo build --release
```

1. Run `apophenia download` to download database.
2. Use `apophenia install` to enable command completion for your shell.

### File location and Config

Apophenia stores its configuration and database files in the following locations:

| Platform | Database | Config |
| --- | --- | --- |
| Windows | `%LOCALAPPDATA%/Apophenia/apophenia.db` | `%APPDATA%/Apophenia/config.toml` |
| Unix | `$HOME/.apophenia/apophenia.db` | `$HOME/.apophenia/config.toml` |

`config.toml` can change the `install`/`download` behavior, for example:

```toml
output = "R:\\apophenia.ps1"
download_url = "https://example.com/apophenia.db"
```

when output is specified, the completion script will be written to the given path, not stdio.
When download_url is specified, the database will be downloaded from the given URL instead of the default location.
