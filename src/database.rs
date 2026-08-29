use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;
use sqlx::{Row, SqlitePool, sqlite::SqlitePoolOptions};

pub const DB_SCHEMA_VERSION: i32 = 1;
pub const SCHEMA_SQL: &str = include_str!("../dist/schema.sql");

const APP_VERSION_BY_KEY: &str = r#"
SELECT
    av.id,
    a.id AS application_id,
    a.name AS application_name,
    av.internal_version,
    av.binary_name,
    av.description,
    av.long_description,
    av.platforms_json,
    av.source_path
FROM app_versions AS av
JOIN applications AS a ON a.id = av.application_id
WHERE a.name = ? AND av.internal_version = ? AND a.enabled = 1
"#;

const APPLICATION_VERSIONS: &str = r#"
SELECT
    av.id,
    a.id AS application_id,
    a.name AS application_name,
    av.internal_version,
    av.binary_name,
    av.description,
    av.long_description,
    av.platforms_json,
    av.source_path
FROM app_versions AS av
JOIN applications AS a ON a.id = av.application_id
WHERE a.name = ? AND a.enabled = 1
ORDER BY av.internal_version
"#;

const ALL_APPLICATION_VERSIONS: &str = r#"
SELECT
    av.id,
    a.id AS application_id,
    a.name AS application_name,
    av.internal_version,
    av.binary_name,
    av.description,
    av.long_description,
    av.platforms_json,
    av.source_path
FROM app_versions AS av
JOIN applications AS a ON a.id = av.application_id
WHERE a.enabled = 1
ORDER BY a.name, av.internal_version
"#;

const SUPPORT_RULES_FOR_VERSION: &str = r#"
SELECT id, expression, kind, normalized_expression, specificity
FROM support_rules
WHERE app_version_id = ?
ORDER BY specificity DESC, id
"#;

const VERSION_COMMANDS_FOR_VERSION: &str = r#"
SELECT ordinal, argv_json
FROM version_commands
WHERE app_version_id = ?
ORDER BY ordinal
"#;

const VERSION_PREPROCESSORS_FOR_VERSION: &str = r#"
SELECT ordinal, engine, pattern, replacement, template
FROM version_preprocessors
WHERE app_version_id = ?
ORDER BY ordinal
"#;

const COMMANDS_FOR_VERSION: &str = r#"
SELECT
    id,
    parent_id,
    path,
    name,
    about,
    long_about,
    hidden,
    position,
    subcommand_required,
    arg_required_else_help,
    subcommand_precedence_over_arg,
    infer_subcommands,
    disable_help_subcommand,
    allow_external_subcommands,
    args_conflicts_with_subcommands,
    subcommand_negates_reqs,
    multicall,
    no_binary_name,
    disable_help_flag,
    disable_version_flag
FROM commands
WHERE app_version_id = ?
ORDER BY length(path) - length(replace(path, '/', '')), path, position, name
"#;

const COMMAND_NAMES_FOR_VERSION: &str = r#"
SELECT cn.command_id, cn.ordinal, cn.name, cn.name_kind
FROM command_names AS cn
JOIN commands AS c ON c.id = cn.command_id
WHERE c.app_version_id = ?
ORDER BY cn.command_id, cn.ordinal
"#;

const OPTIONS_FOR_VERSION: &str = r#"
SELECT
    o.id,
    o.command_id,
    o.stable_id,
    o.action,
    o.help,
    o.long_help,
    o.value_name,
    o.value_names_json,
    o.value_hint,
    o.required,
    o.global_option,
    o.multiple,
    o.hidden,
    o.hide_possible_values,
    o.value_delimiter,
    o.value_terminator,
    o.default_value,
    o.default_missing_value,
    o.require_equals,
    o.allow_hyphen_values,
    o.allow_negative_numbers,
    o.exclusive,
    o.last,
    o.trailing_var_arg,
    o.requires_json,
    o.conflicts_with_json,
    o.overrides_with_json,
    o.min_values,
    o.max_values,
    o.position
FROM options AS o
JOIN commands AS c ON c.id = o.command_id
WHERE c.app_version_id = ?
ORDER BY o.command_id, o.position, o.id
"#;

const OPTION_NAMES_FOR_VERSION: &str = r#"
SELECT n.option_id, n.ordinal, n.name, n.name_kind, n.token_kind
FROM option_names AS n
JOIN options AS o ON o.id = n.option_id
JOIN commands AS c ON c.id = o.command_id
WHERE c.app_version_id = ?
ORDER BY n.option_id, n.ordinal
"#;

const ARGUMENTS_FOR_VERSION: &str = r#"
SELECT
    a.id,
    a.command_id,
    a.stable_id,
    a.position,
    a.help,
    a.long_help,
    a.value_name,
    a.value_names_json,
    a.value_hint,
    a.required,
    a.global_argument,
    a.multiple,
    a.hidden,
    a.hide_possible_values,
    a.value_delimiter,
    a.value_terminator,
    a.default_value,
    a.default_missing_value,
    a.require_equals,
    a.allow_hyphen_values,
    a.allow_negative_numbers,
    a.exclusive,
    a.last,
    a.trailing_var_arg,
    a.requires_json,
    a.conflicts_with_json,
    a.overrides_with_json,
    a.min_values,
    a.max_values
FROM arguments AS a
JOIN commands AS c ON c.id = a.command_id
WHERE c.app_version_id = ?
ORDER BY a.command_id, a.position, a.id
"#;

const OPTION_VALUES_FOR_VERSION: &str = r#"
SELECT
    v.option_id,
    v.value_index,
    v.ordinal,
    v.value,
    v.prefix,
    v.help,
    v.candidate_id,
    v.tag,
    v.display_order,
    v.hidden,
    v.value_kind
FROM option_values AS v
JOIN options AS o ON o.id = v.option_id
JOIN commands AS c ON c.id = o.command_id
WHERE c.app_version_id = ?
ORDER BY v.option_id, v.value_index, v.ordinal
"#;

const ARGUMENT_VALUES_FOR_VERSION: &str = r#"
SELECT
    v.argument_id,
    v.value_index,
    v.ordinal,
    v.value,
    v.prefix,
    v.help,
    v.candidate_id,
    v.tag,
    v.display_order,
    v.hidden,
    v.value_kind
FROM argument_values AS v
JOIN arguments AS a ON a.id = v.argument_id
JOIN commands AS c ON c.id = a.command_id
WHERE c.app_version_id = ?
ORDER BY v.argument_id, v.value_index, v.ordinal
"#;

const OPTION_COMPLETERS_FOR_VERSION: &str = r#"
SELECT option_id, value_index, completer_kind, path_kind, path_stdio, path_current_dir
FROM option_completers
WHERE option_id IN (
    SELECT o.id
    FROM options AS o
    JOIN commands AS c ON c.id = o.command_id
    WHERE c.app_version_id = ?
)
ORDER BY option_id, value_index
"#;

const ARGUMENT_COMPLETERS_FOR_VERSION: &str = r#"
SELECT argument_id, value_index, completer_kind, path_kind, path_stdio, path_current_dir
FROM argument_completers
WHERE argument_id IN (
    SELECT a.id
    FROM arguments AS a
    JOIN commands AS c ON c.id = a.command_id
    WHERE c.app_version_id = ?
)
ORDER BY argument_id, value_index
"#;

const COMMAND_CANDIDATES_FOR_VERSION: &str = r#"
SELECT command_id, ordinal, value, prefix, help, candidate_id, tag, display_order, hidden
FROM command_candidates
WHERE command_id IN (SELECT id FROM commands WHERE app_version_id = ?)
ORDER BY command_id, ordinal
"#;

#[derive(Clone, Debug)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    pub async fn open(path: impl AsRef<Path>, read_only: bool) -> Result<Self> {
        let path = path.as_ref();
        let mut options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(path)
            .foreign_keys(true)
            .create_if_missing(!read_only);
        if read_only {
            options = options.read_only(true).pragma("query_only", "ON");
        }
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .with_context(|| format!("opening SQLite database {}", path.display()))?;
        Ok(Self { pool })
    }

    pub async fn create(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let database = Self::open(path, false).await?;
        sqlx::raw_sql(SCHEMA_SQL).execute(&database.pool).await?;
        Ok(database)
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn close(self) {
        self.pool.close().await;
    }

    pub async fn load_runtime(
        &self,
        application: &str,
        internal_version: &str,
    ) -> Result<LoadedBundle> {
        let row = sqlx::query(APP_VERSION_BY_KEY)
            .bind(application)
            .bind(internal_version)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| {
                anyhow!("no installed command data for {application}:{internal_version}")
            })?;
        self.load_bundle_from_version_row(row).await
    }

    pub async fn list_install_versions(&self, application: &str) -> Result<Vec<InstallVersion>> {
        let rows = sqlx::query(APPLICATION_VERSIONS)
            .bind(application)
            .fetch_all(&self.pool)
            .await?;
        self.load_install_versions(rows).await
    }

    pub async fn list_all_install_versions(&self) -> Result<Vec<InstallVersion>> {
        let rows = sqlx::query(ALL_APPLICATION_VERSIONS)
            .fetch_all(&self.pool)
            .await?;
        self.load_install_versions(rows).await
    }

    async fn load_install_versions(
        &self,
        rows: Vec<sqlx::sqlite::SqliteRow>,
    ) -> Result<Vec<InstallVersion>> {
        let mut versions = Vec::with_capacity(rows.len());
        for row in rows {
            versions.push(self.load_install_version_from_row(row).await?);
        }
        Ok(versions)
    }

    async fn load_bundle_from_version_row(
        &self,
        row: sqlx::sqlite::SqliteRow,
    ) -> Result<LoadedBundle> {
        let version = self.load_install_version_from_row(row).await?;
        let commands = sqlx::query(COMMANDS_FOR_VERSION)
            .bind(version.id)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(command_from_row)
            .collect::<Result<Vec<_>>>()?;
        let command_names = sqlx::query(COMMAND_NAMES_FOR_VERSION)
            .bind(version.id)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(command_name_from_row)
            .collect::<Result<Vec<_>>>()?;
        let options = sqlx::query(OPTIONS_FOR_VERSION)
            .bind(version.id)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(option_from_row)
            .collect::<Result<Vec<_>>>()?;
        let option_names = sqlx::query(OPTION_NAMES_FOR_VERSION)
            .bind(version.id)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(option_name_from_row)
            .collect::<Result<Vec<_>>>()?;
        let arguments = sqlx::query(ARGUMENTS_FOR_VERSION)
            .bind(version.id)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(argument_from_row)
            .collect::<Result<Vec<_>>>()?;
        let option_values = sqlx::query(OPTION_VALUES_FOR_VERSION)
            .bind(version.id)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(option_value_from_row)
            .collect::<Result<Vec<_>>>()?;
        let argument_values = sqlx::query(ARGUMENT_VALUES_FOR_VERSION)
            .bind(version.id)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(argument_value_from_row)
            .collect::<Result<Vec<_>>>()?;
        let option_completers = sqlx::query(OPTION_COMPLETERS_FOR_VERSION)
            .bind(version.id)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(completer_from_row)
            .collect::<Result<Vec<_>>>()?;
        let argument_completers = sqlx::query(ARGUMENT_COMPLETERS_FOR_VERSION)
            .bind(version.id)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(completer_from_row)
            .collect::<Result<Vec<_>>>()?;
        let command_candidates = sqlx::query(COMMAND_CANDIDATES_FOR_VERSION)
            .bind(version.id)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(command_candidate_from_row)
            .collect::<Result<Vec<_>>>()?;

        Ok(LoadedBundle {
            version,
            commands,
            command_names,
            options,
            option_names,
            arguments,
            option_values,
            argument_values,
            option_completers,
            argument_completers,
            command_candidates,
        })
    }

    async fn load_install_version_from_row(
        &self,
        row: sqlx::sqlite::SqliteRow,
    ) -> Result<InstallVersion> {
        let id = row.try_get("id")?;
        let rules = sqlx::query(SUPPORT_RULES_FOR_VERSION)
            .bind(id)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|row| {
                Ok(SupportRule {
                    id: row.try_get("id")?,
                    expression: row.try_get("expression")?,
                    kind: row.try_get("kind")?,
                    normalized_expression: row.try_get("normalized_expression")?,
                    specificity: row.try_get("specificity")?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let version_commands = sqlx::query(VERSION_COMMANDS_FOR_VERSION)
            .bind(id)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|row| {
                let json: String = row.try_get("argv_json")?;
                serde_json::from_str(&json)
                    .with_context(|| format!("invalid version command JSON for app_version {id}"))
            })
            .collect::<Result<Vec<Vec<String>>>>()?;
        let preprocessors = sqlx::query(VERSION_PREPROCESSORS_FOR_VERSION)
            .bind(id)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|row| {
                Ok(VersionPreprocessor {
                    ordinal: row.try_get("ordinal")?,
                    engine: row.try_get("engine")?,
                    pattern: row.try_get("pattern")?,
                    replacement: row.try_get("replacement")?,
                    template: row.try_get("template")?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let platforms_json: String = row.try_get("platforms_json")?;
        let platforms = serde_json::from_str(&platforms_json)
            .with_context(|| format!("invalid platforms JSON for app_version {id}"))?;
        Ok(InstallVersion {
            id,
            application_id: row.try_get("application_id")?,
            application_name: row.try_get("application_name")?,
            internal_version: row.try_get("internal_version")?,
            binary_name: row.try_get("binary_name")?,
            description: row.try_get("description")?,
            long_description: row.try_get("long_description")?,
            platforms,
            source_path: row.try_get("source_path")?,
            rules,
            version_commands,
            preprocessors,
        })
    }
}

#[derive(Clone, Debug)]
pub struct InstallVersion {
    pub id: i64,
    pub application_id: i64,
    pub application_name: String,
    pub internal_version: String,
    pub binary_name: String,
    pub description: Option<String>,
    pub long_description: Option<String>,
    pub platforms: Vec<String>,
    pub source_path: String,
    pub rules: Vec<SupportRule>,
    pub version_commands: Vec<Vec<String>>,
    pub preprocessors: Vec<VersionPreprocessor>,
}

#[derive(Clone, Debug)]
pub struct SupportRule {
    pub id: i64,
    pub expression: String,
    pub kind: String,
    pub normalized_expression: Option<String>,
    pub specificity: i64,
}

#[derive(Clone, Debug)]
pub struct VersionPreprocessor {
    pub ordinal: i64,
    pub engine: String,
    pub pattern: Option<String>,
    pub replacement: Option<String>,
    pub template: Option<String>,
}

#[derive(Clone, Debug)]
pub struct LoadedBundle {
    pub version: InstallVersion,
    pub commands: Vec<CommandRow>,
    pub command_names: Vec<CommandNameRow>,
    pub options: Vec<OptionRow>,
    pub option_names: Vec<OptionNameRow>,
    pub arguments: Vec<ArgumentRow>,
    pub option_values: Vec<ValueRow>,
    pub argument_values: Vec<ValueRow>,
    pub option_completers: Vec<CompleterRow>,
    pub argument_completers: Vec<CompleterRow>,
    pub command_candidates: Vec<CommandCandidateRow>,
}

#[derive(Clone, Debug)]
pub struct CommandRow {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub path: String,
    pub name: String,
    pub about: Option<String>,
    pub long_about: Option<String>,
    pub hidden: bool,
    pub position: i64,
    pub subcommand_required: bool,
    pub arg_required_else_help: bool,
    pub subcommand_precedence_over_arg: bool,
    pub infer_subcommands: bool,
    pub disable_help_subcommand: bool,
    pub allow_external_subcommands: bool,
    pub args_conflicts_with_subcommands: bool,
    pub subcommand_negates_reqs: bool,
    pub multicall: bool,
    pub no_binary_name: bool,
    pub disable_help_flag: bool,
    pub disable_version_flag: bool,
}

#[derive(Clone, Debug)]
pub struct CommandNameRow {
    pub command_id: i64,
    pub ordinal: i64,
    pub name: String,
    pub name_kind: String,
}

#[derive(Clone, Debug)]
pub struct OptionRow {
    pub id: i64,
    pub command_id: i64,
    pub stable_id: String,
    pub action: String,
    pub help: Option<String>,
    pub long_help: Option<String>,
    pub value_name: Option<String>,
    pub value_names: Vec<String>,
    pub value_hint: String,
    pub required: bool,
    pub global: bool,
    pub multiple: bool,
    pub hidden: bool,
    pub hide_possible_values: bool,
    pub value_delimiter: Option<String>,
    pub value_terminator: Option<String>,
    pub default_value: Option<String>,
    pub default_missing_value: Option<String>,
    pub require_equals: bool,
    pub allow_hyphen_values: bool,
    pub allow_negative_numbers: bool,
    pub exclusive: bool,
    pub last: bool,
    pub trailing_var_arg: bool,
    pub requires: Vec<String>,
    pub conflicts_with: Vec<String>,
    pub overrides_with: Vec<String>,
    pub min_values: Option<i64>,
    pub max_values: Option<i64>,
    pub position: i64,
}

#[derive(Clone, Debug)]
pub struct OptionNameRow {
    pub option_id: i64,
    pub ordinal: i64,
    pub name: String,
    pub name_kind: String,
    pub token_kind: String,
}

#[derive(Clone, Debug)]
pub struct ArgumentRow {
    pub id: i64,
    pub command_id: i64,
    pub stable_id: String,
    pub position: i64,
    pub help: Option<String>,
    pub long_help: Option<String>,
    pub value_name: Option<String>,
    pub value_names: Vec<String>,
    pub value_hint: String,
    pub required: bool,
    pub global: bool,
    pub multiple: bool,
    pub hidden: bool,
    pub hide_possible_values: bool,
    pub value_delimiter: Option<String>,
    pub value_terminator: Option<String>,
    pub default_value: Option<String>,
    pub default_missing_value: Option<String>,
    pub require_equals: bool,
    pub allow_hyphen_values: bool,
    pub allow_negative_numbers: bool,
    pub exclusive: bool,
    pub last: bool,
    pub trailing_var_arg: bool,
    pub requires: Vec<String>,
    pub conflicts_with: Vec<String>,
    pub overrides_with: Vec<String>,
    pub min_values: Option<i64>,
    pub max_values: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct ValueRow {
    pub owner_id: i64,
    pub value_index: i64,
    pub ordinal: i64,
    pub value: String,
    pub prefix: Option<String>,
    pub help: Option<String>,
    pub candidate_id: Option<String>,
    pub tag: Option<String>,
    pub display_order: Option<i64>,
    pub hidden: bool,
    pub value_kind: String,
}

#[derive(Clone, Debug)]
pub struct CompleterRow {
    pub owner_id: i64,
    pub value_index: i64,
    pub completer_kind: String,
    pub path_kind: Option<String>,
    pub path_stdio: bool,
    pub path_current_dir: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CommandCandidateRow {
    pub command_id: i64,
    pub ordinal: i64,
    pub value: String,
    pub prefix: Option<String>,
    pub help: Option<String>,
    pub candidate_id: Option<String>,
    pub tag: Option<String>,
    pub display_order: Option<i64>,
    pub hidden: bool,
}

fn command_from_row(row: sqlx::sqlite::SqliteRow) -> Result<CommandRow> {
    Ok(CommandRow {
        id: row.try_get("id")?,
        parent_id: row.try_get("parent_id")?,
        path: row.try_get("path")?,
        name: row.try_get("name")?,
        about: row.try_get("about")?,
        long_about: row.try_get("long_about")?,
        hidden: integer_bool(row.try_get("hidden")?, "commands.hidden")?,
        position: row.try_get("position")?,
        subcommand_required: integer_bool(
            row.try_get("subcommand_required")?,
            "subcommand_required",
        )?,
        arg_required_else_help: integer_bool(
            row.try_get("arg_required_else_help")?,
            "arg_required_else_help",
        )?,
        subcommand_precedence_over_arg: integer_bool(
            row.try_get("subcommand_precedence_over_arg")?,
            "subcommand_precedence_over_arg",
        )?,
        infer_subcommands: integer_bool(row.try_get("infer_subcommands")?, "infer_subcommands")?,
        disable_help_subcommand: integer_bool(
            row.try_get("disable_help_subcommand")?,
            "disable_help_subcommand",
        )?,
        allow_external_subcommands: integer_bool(
            row.try_get("allow_external_subcommands")?,
            "allow_external_subcommands",
        )?,
        args_conflicts_with_subcommands: integer_bool(
            row.try_get("args_conflicts_with_subcommands")?,
            "args_conflicts_with_subcommands",
        )?,
        subcommand_negates_reqs: integer_bool(
            row.try_get("subcommand_negates_reqs")?,
            "subcommand_negates_reqs",
        )?,
        multicall: integer_bool(row.try_get("multicall")?, "multicall")?,
        no_binary_name: integer_bool(row.try_get("no_binary_name")?, "no_binary_name")?,
        disable_help_flag: integer_bool(row.try_get("disable_help_flag")?, "disable_help_flag")?,
        disable_version_flag: integer_bool(
            row.try_get("disable_version_flag")?,
            "disable_version_flag",
        )?,
    })
}

fn command_name_from_row(row: sqlx::sqlite::SqliteRow) -> Result<CommandNameRow> {
    Ok(CommandNameRow {
        command_id: row.try_get("command_id")?,
        ordinal: row.try_get("ordinal")?,
        name: row.try_get("name")?,
        name_kind: row.try_get("name_kind")?,
    })
}

fn option_from_row(row: sqlx::sqlite::SqliteRow) -> Result<OptionRow> {
    Ok(OptionRow {
        id: row.try_get("id")?,
        command_id: row.try_get("command_id")?,
        stable_id: row.try_get("stable_id")?,
        action: row.try_get("action")?,
        help: row.try_get("help")?,
        long_help: row.try_get("long_help")?,
        value_name: row.try_get("value_name")?,
        value_names: json_vec(&row, "value_names_json")?,
        value_hint: row.try_get("value_hint")?,
        required: integer_bool(row.try_get("required")?, "options.required")?,
        global: integer_bool(row.try_get("global_option")?, "options.global_option")?,
        multiple: integer_bool(row.try_get("multiple")?, "options.multiple")?,
        hidden: integer_bool(row.try_get("hidden")?, "options.hidden")?,
        hide_possible_values: integer_bool(
            row.try_get("hide_possible_values")?,
            "options.hide_possible_values",
        )?,
        value_delimiter: row.try_get("value_delimiter")?,
        value_terminator: row.try_get("value_terminator")?,
        default_value: row.try_get("default_value")?,
        default_missing_value: row.try_get("default_missing_value")?,
        require_equals: integer_bool(row.try_get("require_equals")?, "options.require_equals")?,
        allow_hyphen_values: integer_bool(
            row.try_get("allow_hyphen_values")?,
            "options.allow_hyphen_values",
        )?,
        allow_negative_numbers: integer_bool(
            row.try_get("allow_negative_numbers")?,
            "options.allow_negative_numbers",
        )?,
        exclusive: integer_bool(row.try_get("exclusive")?, "options.exclusive")?,
        last: integer_bool(row.try_get("last")?, "options.last")?,
        trailing_var_arg: integer_bool(
            row.try_get("trailing_var_arg")?,
            "options.trailing_var_arg",
        )?,
        requires: json_vec(&row, "requires_json")?,
        conflicts_with: json_vec(&row, "conflicts_with_json")?,
        overrides_with: json_vec(&row, "overrides_with_json")?,
        min_values: row.try_get("min_values")?,
        max_values: row.try_get("max_values")?,
        position: row.try_get("position")?,
    })
}

fn option_name_from_row(row: sqlx::sqlite::SqliteRow) -> Result<OptionNameRow> {
    Ok(OptionNameRow {
        option_id: row.try_get("option_id")?,
        ordinal: row.try_get("ordinal")?,
        name: row.try_get("name")?,
        name_kind: row.try_get("name_kind")?,
        token_kind: row.try_get("token_kind")?,
    })
}

fn argument_from_row(row: sqlx::sqlite::SqliteRow) -> Result<ArgumentRow> {
    Ok(ArgumentRow {
        id: row.try_get("id")?,
        command_id: row.try_get("command_id")?,
        stable_id: row.try_get("stable_id")?,
        position: row.try_get("position")?,
        help: row.try_get("help")?,
        long_help: row.try_get("long_help")?,
        value_name: row.try_get("value_name")?,
        value_names: json_vec(&row, "value_names_json")?,
        value_hint: row.try_get("value_hint")?,
        required: integer_bool(row.try_get("required")?, "arguments.required")?,
        global: integer_bool(row.try_get("global_argument")?, "arguments.global_argument")?,
        multiple: integer_bool(row.try_get("multiple")?, "arguments.multiple")?,
        hidden: integer_bool(row.try_get("hidden")?, "arguments.hidden")?,
        hide_possible_values: integer_bool(
            row.try_get("hide_possible_values")?,
            "arguments.hide_possible_values",
        )?,
        value_delimiter: row.try_get("value_delimiter")?,
        value_terminator: row.try_get("value_terminator")?,
        default_value: row.try_get("default_value")?,
        default_missing_value: row.try_get("default_missing_value")?,
        require_equals: integer_bool(row.try_get("require_equals")?, "arguments.require_equals")?,
        allow_hyphen_values: integer_bool(
            row.try_get("allow_hyphen_values")?,
            "arguments.allow_hyphen_values",
        )?,
        allow_negative_numbers: integer_bool(
            row.try_get("allow_negative_numbers")?,
            "arguments.allow_negative_numbers",
        )?,
        exclusive: integer_bool(row.try_get("exclusive")?, "arguments.exclusive")?,
        last: integer_bool(row.try_get("last")?, "arguments.last")?,
        trailing_var_arg: integer_bool(
            row.try_get("trailing_var_arg")?,
            "arguments.trailing_var_arg",
        )?,
        requires: json_vec(&row, "requires_json")?,
        conflicts_with: json_vec(&row, "conflicts_with_json")?,
        overrides_with: json_vec(&row, "overrides_with_json")?,
        min_values: row.try_get("min_values")?,
        max_values: row.try_get("max_values")?,
    })
}

fn option_value_from_row(row: sqlx::sqlite::SqliteRow) -> Result<ValueRow> {
    value_from_row(&row, "option_id")
}

fn argument_value_from_row(row: sqlx::sqlite::SqliteRow) -> Result<ValueRow> {
    value_from_row(&row, "argument_id")
}

fn value_from_row(row: &sqlx::sqlite::SqliteRow, owner_column: &str) -> Result<ValueRow> {
    Ok(ValueRow {
        owner_id: row.try_get(owner_column)?,
        value_index: row.try_get("value_index")?,
        ordinal: row.try_get("ordinal")?,
        value: row.try_get("value")?,
        prefix: row.try_get("prefix")?,
        help: row.try_get("help")?,
        candidate_id: row.try_get("candidate_id")?,
        tag: row.try_get("tag")?,
        display_order: row.try_get("display_order")?,
        hidden: integer_bool(row.try_get("hidden")?, "candidate.hidden")?,
        value_kind: row.try_get("value_kind")?,
    })
}

fn completer_from_row(row: sqlx::sqlite::SqliteRow) -> Result<CompleterRow> {
    let owner_id = row
        .try_get::<Option<i64>, _>("option_id")
        .or_else(|_| row.try_get::<Option<i64>, _>("argument_id"))?
        .ok_or_else(|| anyhow!("completer row has no owner"))?;
    Ok(CompleterRow {
        owner_id,
        value_index: row.try_get("value_index")?,
        completer_kind: row.try_get("completer_kind")?,
        path_kind: row.try_get("path_kind")?,
        path_stdio: integer_bool(row.try_get("path_stdio")?, "path_stdio")?,
        path_current_dir: row.try_get("path_current_dir")?,
    })
}

fn command_candidate_from_row(row: sqlx::sqlite::SqliteRow) -> Result<CommandCandidateRow> {
    Ok(CommandCandidateRow {
        command_id: row.try_get("command_id")?,
        ordinal: row.try_get("ordinal")?,
        value: row.try_get("value")?,
        prefix: row.try_get("prefix")?,
        help: row.try_get("help")?,
        candidate_id: row.try_get("candidate_id")?,
        tag: row.try_get("tag")?,
        display_order: row.try_get("display_order")?,
        hidden: integer_bool(row.try_get("hidden")?, "command_candidate.hidden")?,
    })
}

fn json_vec(row: &sqlx::sqlite::SqliteRow, column: &str) -> Result<Vec<String>> {
    let json: String = row.try_get(column)?;
    serde_json::from_str(&json).with_context(|| format!("invalid JSON in {column}"))
}

fn integer_bool(value: i64, column: &str) -> Result<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        other => bail!("invalid boolean {other} in {column}"),
    }
}

pub fn json<T: serde::Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).context("serializing JSON database field")
}

pub fn json_value(value: &str) -> Result<Value> {
    serde_json::from_str(value).context("parsing JSON database field")
}

pub fn database_path_from_env_or_default(default: PathBuf) -> PathBuf {
    std::env::var_os("APOPHENIA_DB")
        .map(PathBuf::from)
        .unwrap_or(default)
}
