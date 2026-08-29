use std::ffi::OsString;
use std::fs;

use apophenia::builder::build_database;
use apophenia::database::Database;
use apophenia::runtime::build_command;
use clap::ValueHint;

#[tokio::test]
async fn builds_sqlite_and_restores_dynamic_completion_model() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let source = temp.path().join("commands");
    let child_dir = source.join("demo").join("1").join("commands").join("child");
    fs::create_dir_all(&child_dir).expect("create child manifest directory");

    fs::write(
        source.join("demo").join("1").join("main.toml"),
        r#"
schema_version = 1

[command]
name = "demo"
binary = "demo"
platforms = ["windows"]
supported_versions = ["*"]
allow_external_subcommands = true

[[command.options]]
id = "verbose"
names = ["--verbose", "-v"]
action = "flag"
global = true

[[command.options]]
id = "mode"
names = ["--mode"]
action = "value"
possible_values = ["fast", "safe"]

[[command.options]]
id = "remote"
names = ["--remote"]
action = "value"
min_values = 2
max_values = 2

[[command.options.value_completers]]
kind = "candidates"
arg_index = 0
candidates = [{ value = "origin", prefix = "remote:", help = "Default remote" }]

[[command.options.value_completers]]
kind = "candidates"
arg_index = 1
candidates = [{ value = "main" }, { value = "develop" }]

[[command.options]]
id = "path"
names = ["--path"]
action = "value"

[command.options.path_completion]
kind = "any"
stdio = true

[[command.arguments]]
id = "input"
position = 1
value_hint = "any_path"

[[command.subcommand_candidates]]
value = "plugin"
help = "External plugin"
tag = "external"
display_order = 10

[[command.subcommand_candidates]]
value = "hidden-plugin"
hidden = true
"#,
    )
    .expect("write root manifest");

    fs::write(
        child_dir.join("main.toml"),
        r#"
[command]
name = "child"
aliases = ["c"]
position = 1

[[command.options]]
id = "child_flag"
names = ["--child-flag"]
action = "flag"
"#,
    )
    .expect("write child manifest");

    let database_path = temp.path().join("dist").join("apophenia.db");
    let stats = build_database(&source, &database_path)
        .await
        .expect("build SQLite database");
    assert_eq!(stats.commands, 2);
    assert_eq!(stats.options, 5);
    assert_eq!(stats.arguments, 1);

    let database = Database::open(&database_path, true)
        .await
        .expect("open generated database");
    let bundle = database
        .load_runtime("demo", "1")
        .await
        .expect("load runtime bundle");
    database.close().await;

    let mut command = build_command(&bundle).expect("restore clap command");
    command.clone().debug_assert();

    let verbose = command
        .get_arguments()
        .find(|argument| argument.get_id() == "verbose")
        .expect("verbose option");
    assert_eq!(verbose.get_short(), Some('v'));
    assert_eq!(verbose.get_long(), Some("verbose"));

    let input = command
        .get_arguments()
        .find(|argument| argument.get_id() == "input")
        .expect("input argument");
    assert_eq!(input.get_value_hint(), ValueHint::Unknown);

    let root = complete(&mut command, ["demo", ""], 1);
    assert_contains(&root, "--mode");
    assert_contains(&root, "--verbose");
    assert_contains(&root, "child");
    assert_contains(&root, "plugin");

    let long_verbose = complete(&mut command, ["demo", "--v"], 1);
    assert_contains(&long_verbose, "--verbose");

    let child = complete(&mut command, ["demo", "child", ""], 2);
    assert_contains(&child, "--verbose");

    let mode = complete(&mut command, ["demo", "--mode", ""], 2);
    assert_contains(&mode, "fast");
    assert_contains(&mode, "safe");

    let first_remote = complete(&mut command, ["demo", "--remote", ""], 2);
    assert_contains(&first_remote, "remote:origin");

    let second_remote = complete(&mut command, ["demo", "--remote", "origin", ""], 3);
    assert_contains(&second_remote, "main");
    assert_contains(&second_remote, "develop");

    let native_path = complete(&mut command, ["demo", "--path", ""], 2);
    assert!(
        native_path.is_empty(),
        "path candidates must be supplied by the shell: {native_path:?}"
    );
}

fn complete<const N: usize>(
    command: &mut clap::Command,
    args: [&str; N],
    index: usize,
) -> Vec<String> {
    clap_complete::engine::complete(
        command,
        args.into_iter().map(OsString::from).collect(),
        index,
        None,
    )
    .expect("completion result")
    .into_iter()
    .map(|candidate| candidate.get_value().to_string_lossy().into_owned())
    .collect()
}

fn assert_contains(values: &[String], expected: &str) {
    assert!(
        values.iter().any(|value| value == expected),
        "{expected:?} not found in {values:?}"
    );
}
