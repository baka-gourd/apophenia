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

## Supported CLIs

| CLI | Platforms | Version(verified) |
| --- | --- | --- |
| [zpaqfranz](https://github.com/fcorbelli/zpaqfranz) | Windows | |
| [flac](https://xiph.org/flac/index.html) | Windows | 1.5.0 |
| [metaflac](https://xiph.org/flac/index.html) | Windows | 1.5.0 |
| [cjxl](https://github.com/libjxl/libjxl) | Windows | 0.12.0 |
| [cksum(uutils)](https://github.com/uutils/coreutils) | Windows | 0.10.0 |
| [meme](https://github.com/MemeCrafters/meme-generator-rs) | Windows | |
| [wavpack](https://www.wavpack.com/downloads.html) | Windows | 5.9.0 |
| [wvgain](https://www.wavpack.com/downloads.html) | Windows | 5.9.0 |
| [wvtag](https://www.wavpack.com/downloads.html) | Windows | 5.9.0 |
| [wvunpack](https://www.wavpack.com/downloads.html) | Windows | 5.9.0 |
| [fx](https://fx.wtf) | Windows | 39.2.0 |
