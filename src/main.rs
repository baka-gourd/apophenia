use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand};

use apophenia::builder::build_database;
use apophenia::config::{load as load_config, write_output};
use apophenia::database::{Database, database_path_from_env_or_default};
use apophenia::download::{DEFAULT_DATABASE_URL, download_database};
use apophenia::install::{Shell, completion_registration, detect_installations, detect_shell};
use apophenia::paths::app_paths;
use apophenia::runtime::build_command;
use apophenia::version::parse_app_selector;
use apophenia::{APP_SELECTOR_ENV, COMPLETION_ENV};

fn main() -> Result<()> {
    // completion app
    clap_complete::CompleteEnv::with_factory(command_from_environment)
        .var(COMPLETION_ENV)
        .complete();

    // main app
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Build { source, output }) => {
            let runtime = tokio::runtime::Runtime::new()?;
            let stats = runtime.block_on(build_database(&source, &output))?;
            eprintln!(
                "built {}: {} app(s), {} version(s), {} command(s), {} option(s), {} argument(s), {} candidate(s)",
                output.display(),
                stats.applications,
                stats.versions,
                stats.commands,
                stats.options,
                stats.arguments,
                stats.candidates
            );
        }
        Some(Command::Install { shell, database }) => {
            let runtime = tokio::runtime::Runtime::new()?;
            runtime.block_on(install(shell, database))?;
        }
        Some(Command::Download { output }) => {
            let runtime = tokio::runtime::Runtime::new()?;
            runtime.block_on(download(output))?;
        }
        None => {
            Cli::command().print_help()?;
        }
    }
    Ok(())
}

fn command_from_environment() -> clap::Command {
    match load_command_from_environment() {
        Ok(command) => command,
        Err(error) => {
            eprintln!("apophenia completion: {error}");
            std::process::exit(2);
        }
    }
}

fn load_command_from_environment() -> Result<clap::Command> {
    let selector = std::env::var(APP_SELECTOR_ENV).with_context(|| {
        format!("{APP_SELECTOR_ENV} must be set to <application>:<internal-version>")
    })?;
    let selector = parse_app_selector(&selector)?;
    let paths = app_paths()?;
    let default_database = if Path::new("dist/apophenia.db").exists() {
        PathBuf::from("dist/apophenia.db")
    } else {
        paths.database
    };
    let database = database_path_from_env_or_default(default_database);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let bundle = runtime.block_on(async {
        let database = Database::open(&database, true).await?;
        let bundle = database
            .load_runtime(&selector.application, &selector.internal_version)
            .await;
        database.close().await;
        bundle
    })?;
    build_command(&bundle)
}

async fn install(shell: Option<Shell>, database: Option<PathBuf>) -> Result<()> {
    let paths = app_paths()?;
    let config = load_config(&paths.config)?;
    let default_database = if Path::new("dist/apophenia.db").exists() {
        PathBuf::from("dist/apophenia.db")
    } else {
        paths.database
    };
    let database_path = database_path_from_env_or_default(database.unwrap_or(default_database));
    let database = Database::open(&database_path, true).await?;
    let versions = database.list_all_install_versions().await?;
    let report = detect_installations(&versions)?;
    for skipped in &report.skipped {
        eprintln!("apophenia install: skipped {skipped}");
    }

    let shell = match shell {
        Some(shell) => shell,
        None => detect_shell()?,
    };
    let executable = std::env::current_exe().context("finding the apophenia executable")?;
    let mut generated = format!(
        "# apophenia detected {} compatible command(s) for {}\n",
        report.matches.len(),
        std::env::consts::OS
    );
    let mut added = Vec::with_capacity(report.matches.len());
    for selected in report.matches {
        let selector = format!(
            "{}:{}",
            selected.application_name, selected.internal_version
        );
        let binary_name = selected.binary_name.clone();
        let bundle = database
            .load_runtime(&selected.application_name, &selected.internal_version)
            .await?;
        let command = build_command(&bundle)?;
        let instruction = completion_registration(shell, &selector, &command, &executable)?;
        writeln!(
            &mut generated,
            "# {selector} (target command: {})",
            binary_name
        )?;
        generated.push_str(&instruction);
        if !generated.ends_with('\n') {
            generated.push('\n');
        }
        added.push((selector, binary_name));
    }
    database.close().await;
    if let Some(output) = config.output_path(&paths.config)? {
        write_output(&output, &generated)?;
        eprintln!("apophenia install: wrote {}", output.display());
    } else {
        print!("{generated}");
    }
    for (selector, binary_name) in added {
        eprintln!(
            "apophenia install: added completion for {selector} (target command: {binary_name})"
        );
    }
    Ok(())
}

async fn download(output: Option<PathBuf>) -> Result<()> {
    let paths = app_paths()?;
    let config = load_config(&paths.config)?;
    let url = config.download_url()?.unwrap_or(DEFAULT_DATABASE_URL);
    let destination = output.unwrap_or_else(|| database_path_from_env_or_default(paths.database));
    let downloaded = download_database(url, &destination).await?;
    eprintln!(
        "apophenia download: downloaded {} bytes to {}",
        downloaded.bytes,
        destination.display()
    );
    Ok(())
}

#[derive(Debug, Parser)]
#[command(
    name = "apophenia",
    version,
    about = "Data-driven dynamic CLI completion adapters"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Build commands/ into an SQLite completion database.
    Build {
        #[arg(long, default_value = "commands")]
        source: PathBuf,
        #[arg(long, default_value = "dist/apophenia.db")]
        output: PathBuf,
    },
    /// Detect every available target CLI/version and print shell registrations.
    Install {
        #[arg(long, value_enum)]
        shell: Option<Shell>,
        #[arg(long)]
        database: Option<PathBuf>,
    },
    /// Download the published completion database.
    Download {
        #[arg(long, help = "destination database path")]
        output: Option<PathBuf>,
    },
}
