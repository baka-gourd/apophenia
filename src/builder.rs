use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use tempfile::NamedTempFile;
use walkdir::WalkDir;

use crate::database::{Database, json};
use crate::model::{
    ArgumentSpec, CandidateSpec, CommandSpec, Manifest, OptionAction, OptionSpec, PathKind,
    ValueCompletionSpec, VersionPreprocessSpec,
};
use crate::version::classify_support;

#[derive(Debug)]
pub struct SourceDocument {
    pub application: String,
    pub internal_version: String,
    pub command_path: Vec<String>,
    pub file: PathBuf,
    pub manifest: Manifest,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct BuildStats {
    pub applications: usize,
    pub versions: usize,
    pub commands: usize,
    pub options: usize,
    pub arguments: usize,
    pub candidates: usize,
}

pub fn discover_manifests(root: &Path) -> Result<Vec<SourceDocument>> {
    if !root.is_dir() {
        bail!("command source root is not a directory: {}", root.display());
    }

    let mut documents = Vec::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry?;
        if !entry.file_type().is_file() || entry.file_name() != "main.toml" {
            continue;
        }
        let file = entry.path().to_owned();
        let relative = file
            .strip_prefix(root)
            .with_context(|| format!("finding relative path for {}", file.display()))?;
        let parts = relative
            .iter()
            .map(|part| part.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let (application, internal_version, command_path) = parse_source_path(&parts, &file)?;
        let text = fs::read_to_string(&file)
            .with_context(|| format!("reading plain manifest {}", file.display()))?;
        let manifest: Manifest = toml::from_str(&text)
            .with_context(|| format!("parsing TOML manifest {}", file.display()))?;
        manifest.validate(command_path.is_empty(), &file.display().to_string())?;
        if let Some(name) = command_path.last()
            && name != &manifest.command.name
        {
            bail!(
                "{}: command directory `{name}` must match command.name `{}`",
                file.display(),
                manifest.command.name
            );
        }
        documents.push(SourceDocument {
            application,
            internal_version,
            command_path,
            file,
            manifest,
        });
    }

    documents.sort_by(|left, right| {
        (
            &left.application,
            &left.internal_version,
            left.command_path.len(),
            &left.command_path,
        )
            .cmp(&(
                &right.application,
                &right.internal_version,
                right.command_path.len(),
                &right.command_path,
            ))
    });
    Ok(documents)
}

fn parse_source_path(parts: &[String], file: &Path) -> Result<(String, String, Vec<String>)> {
    if parts.len() < 3 || parts.last().is_none_or(|part| part != "main.toml") {
        bail!(
            "{}: expected <app>/<internal-version>/main.toml",
            file.display()
        );
    }
    let application = parts[0].clone();
    let internal_version = parts[1].clone();
    if application.is_empty() || internal_version.is_empty() {
        bail!(
            "{}: application and internal-version must be non-empty",
            file.display()
        );
    }
    if parts.len() == 3 {
        return Ok((application, internal_version, Vec::new()));
    }

    let mut command_path = Vec::new();
    let mut index = 2;
    while index < parts.len() - 1 {
        if parts[index] != "commands" || index + 1 >= parts.len() - 1 {
            bail!(
                "{}: nested manifests must use commands/<name>/main.toml",
                file.display()
            );
        }
        command_path.push(parts[index + 1].clone());
        index += 2;
    }
    if index != parts.len() - 1 {
        bail!("{}: malformed nested command path", file.display());
    }
    Ok((application, internal_version, command_path))
}

pub async fn build_database(source_root: &Path, output: &Path) -> Result<BuildStats> {
    let documents = discover_manifests(source_root)?;
    if documents.is_empty() {
        bail!(
            "no main.toml manifests found below {}",
            source_root.display()
        );
    }
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .with_context(|| format!("creating output directory {}", parent.display()))?;
    let temporary = NamedTempFile::new_in(parent)
        .with_context(|| format!("creating temporary database in {}", parent.display()))?;
    let temporary_path = temporary.into_temp_path();
    let database = Database::create(&temporary_path).await?;
    let result = populate_database(&database, documents).await;
    database.close().await;
    let stats = result?;

    if output.exists() {
        fs::remove_file(output)
            .with_context(|| format!("replacing existing database {}", output.display()))?;
    }
    fs::rename(&temporary_path, output).with_context(|| {
        format!(
            "moving temporary database {} to {}",
            temporary_path.display(),
            output.display()
        )
    })?;
    Ok(stats)
}

async fn populate_database(
    database: &Database,
    documents: Vec<SourceDocument>,
) -> Result<BuildStats> {
    let mut groups: BTreeMap<(String, String), Vec<SourceDocument>> = BTreeMap::new();
    for document in documents {
        groups
            .entry((
                document.application.clone(),
                document.internal_version.clone(),
            ))
            .or_default()
            .push(document);
    }

    let mut transaction = database.pool().begin().await?;
    let mut stats = BuildStats {
        applications: groups
            .keys()
            .map(|(application, _)| application)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        versions: groups.len(),
        ..BuildStats::default()
    };

    for ((application, internal_version), mut documents) in groups {
        documents.sort_by_key(|document| document.command_path.len());
        let root_index = documents
            .iter()
            .position(|document| document.command_path.is_empty())
            .ok_or_else(|| anyhow!("{application}/{internal_version}: missing root main.toml"))?;
        if documents
            .iter()
            .enumerate()
            .any(|(index, document)| document.command_path.is_empty() && index != root_index)
        {
            bail!("{application}/{internal_version}: multiple root manifests");
        }
        if documents
            .iter()
            .filter(|document| document.command_path.is_empty())
            .count()
            != 1
        {
            bail!("{application}/{internal_version}: multiple root manifests");
        }
        let root = documents.remove(root_index);
        let root_spec = &root.manifest.command;

        sqlx::query("INSERT INTO applications (name) VALUES (?) ON CONFLICT(name) DO UPDATE SET enabled = 1")
            .bind(&application)
            .execute(&mut *transaction)
            .await?;
        let application_id: i64 = sqlx::query_scalar("SELECT id FROM applications WHERE name = ?")
            .bind(&application)
            .fetch_one(&mut *transaction)
            .await?;
        let platforms_json = json(&root_spec.platforms)?;
        let version_result = sqlx::query(
            "INSERT INTO app_versions (application_id, internal_version, binary_name, description, long_description, platforms_json, source_path) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(application_id)
        .bind(&internal_version)
        .bind(root_spec.binary.as_deref().unwrap_or(&root_spec.name))
        .bind(&root_spec.description)
        .bind(&root_spec.long_description)
        .bind(platforms_json)
        .bind(root.file.display().to_string())
        .execute(&mut *transaction)
        .await
        .with_context(|| format!("inserting {application}:{internal_version}"))?;
        let app_version_id = version_result.last_insert_rowid();

        insert_version_metadata(&mut transaction, app_version_id, root_spec).await?;

        let root_command_id = insert_command(
            &mut transaction,
            app_version_id,
            None,
            &root_spec.name,
            root_spec,
        )
        .await?;
        stats.commands += 1;
        stats.options +=
            insert_options(&mut transaction, root_command_id, &root_spec.options).await?;
        stats.arguments +=
            insert_arguments(&mut transaction, root_command_id, &root_spec.arguments).await?;
        stats.candidates += candidate_count(root_spec);
        insert_command_candidates(
            &mut transaction,
            root_command_id,
            &root_spec.subcommand_candidates,
        )
        .await?;

        let mut command_ids = HashMap::new();
        command_ids.insert(Vec::<String>::new(), root_command_id);
        for document in documents {
            let parent_path = document
                .command_path
                .get(..document.command_path.len().saturating_sub(1))
                .unwrap_or_default()
                .to_vec();
            let parent_id = *command_ids.get(&parent_path).ok_or_else(|| {
                anyhow!(
                    "{}: parent command {} was not loaded",
                    document.file.display(),
                    parent_path.join("/")
                )
            })?;
            let spec = &document.manifest.command;
            let command_id = insert_command(
                &mut transaction,
                app_version_id,
                Some(parent_id),
                &spec.name,
                spec,
            )
            .await?;
            stats.commands += 1;
            stats.options += insert_options(&mut transaction, command_id, &spec.options).await?;
            stats.arguments +=
                insert_arguments(&mut transaction, command_id, &spec.arguments).await?;
            stats.candidates += candidate_count(spec);
            insert_command_candidates(&mut transaction, command_id, &spec.subcommand_candidates)
                .await?;
            command_ids.insert(document.command_path, command_id);
        }
    }
    transaction.commit().await?;
    Ok(stats)
}

async fn insert_version_metadata(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    app_version_id: i64,
    spec: &CommandSpec,
) -> Result<()> {
    for (ordinal, expression) in spec.supported_versions.iter().enumerate() {
        let rule = classify_support(expression)?;
        sqlx::query(
            "INSERT INTO support_rules (app_version_id, expression, kind, normalized_expression, specificity) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(app_version_id)
        .bind(rule.expression)
        .bind(rule.kind.as_str())
        .bind(rule.normalized_expression)
        .bind(rule.kind.specificity())
        .execute(&mut **transaction)
        .await
        .with_context(|| format!("inserting support rule {ordinal}"))?;
    }
    for (ordinal, argv) in spec.version_commands.iter().enumerate() {
        sqlx::query(
            "INSERT INTO version_commands (app_version_id, ordinal, argv_json) VALUES (?, ?, ?)",
        )
        .bind(app_version_id)
        .bind(ordinal as i64)
        .bind(json(argv)?)
        .execute(&mut **transaction)
        .await?;
    }
    for (ordinal, preprocess) in spec.version_preprocessors.iter().enumerate() {
        let (engine, pattern, replacement, template) = match preprocess {
            VersionPreprocessSpec::Regex { pattern, replace } => (
                "regex",
                Some(pattern.as_str()),
                Some(replace.as_str()),
                None,
            ),
            VersionPreprocessSpec::Minijinja { template } => {
                ("minijinja", None, None, Some(template.as_str()))
            }
        };
        sqlx::query(
            "INSERT INTO version_preprocessors (app_version_id, ordinal, engine, pattern, replacement, template) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(app_version_id)
        .bind(ordinal as i64)
        .bind(engine)
        .bind(pattern)
        .bind(replacement)
        .bind(template)
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
}

async fn insert_command(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    app_version_id: i64,
    parent_id: Option<i64>,
    path_name: &str,
    spec: &CommandSpec,
) -> Result<i64> {
    let path = if let Some(parent_id) = parent_id {
        let parent_path: String = sqlx::query_scalar("SELECT path FROM commands WHERE id = ?")
            .bind(parent_id)
            .fetch_one(&mut **transaction)
            .await?;
        format!("{parent_path}/{path_name}")
    } else {
        path_name.to_owned()
    };
    let result = sqlx::query(
        "INSERT INTO commands (app_version_id, parent_id, path, name, about, long_about, hidden, position, subcommand_required, arg_required_else_help, subcommand_precedence_over_arg, infer_subcommands, disable_help_subcommand, allow_external_subcommands, args_conflicts_with_subcommands, subcommand_negates_reqs, multicall, no_binary_name, disable_help_flag, disable_version_flag) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(app_version_id)
    .bind(parent_id)
    .bind(path)
    .bind(&spec.name)
    .bind(&spec.description)
    .bind(&spec.long_description)
    .bind(bool_int(spec.hidden))
    .bind(spec.position as i64)
    .bind(bool_int(spec.settings.subcommand_required))
    .bind(bool_int(spec.settings.arg_required_else_help))
    .bind(bool_int(spec.settings.subcommand_precedence_over_arg))
    .bind(bool_int(spec.settings.infer_subcommands))
    .bind(bool_int(spec.settings.disable_help_subcommand))
    .bind(bool_int(spec.settings.allow_external_subcommands))
    .bind(bool_int(spec.settings.args_conflicts_with_subcommands))
    .bind(bool_int(spec.settings.subcommand_negates_reqs))
    .bind(bool_int(spec.settings.multicall))
    .bind(bool_int(spec.settings.no_binary_name))
    .bind(bool_int(spec.settings.disable_help_flag))
    .bind(bool_int(spec.settings.disable_version_flag))
    .execute(&mut **transaction)
    .await?;
    let command_id = result.last_insert_rowid();

    let mut ordinal = 0_i64;
    insert_command_name(transaction, command_id, ordinal, &spec.name, "canonical").await?;
    ordinal += 1;
    for alias in &spec.aliases {
        insert_command_name(transaction, command_id, ordinal, alias, "alias").await?;
        ordinal += 1;
    }
    for alias in &spec.visible_aliases {
        insert_command_name(transaction, command_id, ordinal, alias, "visible_alias").await?;
        ordinal += 1;
    }
    Ok(command_id)
}

async fn insert_command_name(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    command_id: i64,
    ordinal: i64,
    name: &str,
    kind: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO command_names (command_id, ordinal, name, name_kind) VALUES (?, ?, ?, ?)",
    )
    .bind(command_id)
    .bind(ordinal)
    .bind(name)
    .bind(kind)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn insert_options(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    command_id: i64,
    options: &[OptionSpec],
) -> Result<usize> {
    for (position, option) in options.iter().enumerate() {
        let result = sqlx::query(
            "INSERT INTO options (command_id, stable_id, action, help, long_help, value_name, value_names_json, value_hint, required, global_option, multiple, hidden, hide_possible_values, value_delimiter, value_terminator, default_value, default_missing_value, require_equals, allow_hyphen_values, allow_negative_numbers, exclusive, last, trailing_var_arg, requires_json, conflicts_with_json, overrides_with_json, min_values, max_values, position) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(command_id)
        .bind(&option.id)
        .bind(option_action(option.action))
        .bind(&option.help)
        .bind(&option.long_help)
        .bind(&option.value_name)
        .bind(json(&option.value_names)?)
        .bind(value_hint(option.value_hint))
        .bind(bool_int(option.required))
        .bind(bool_int(option.global))
        .bind(bool_int(option.multiple))
        .bind(bool_int(option.hidden))
        .bind(bool_int(option.hide_possible_values))
        .bind(option.value_delimiter.as_deref())
        .bind(option.value_terminator.as_deref())
        .bind(&option.default_value)
        .bind(&option.default_missing_value)
        .bind(bool_int(option.require_equals))
        .bind(bool_int(option.allow_hyphen_values))
        .bind(bool_int(option.allow_negative_numbers))
        .bind(bool_int(option.exclusive))
        .bind(bool_int(option.last))
        .bind(bool_int(option.trailing_var_arg))
        .bind(json(&option.requires)?)
        .bind(json(&option.conflicts_with)?)
        .bind(json(&option.overrides_with)?)
        .bind(option.min_values.map(|value| value as i64))
        .bind(option.max_values.map(|value| value as i64))
        .bind(position as i64)
        .execute(&mut **transaction)
        .await?;
        let option_id = result.last_insert_rowid();

        let mut ordinal = 0_i64;
        let mut has_canonical_long = false;
        let mut has_canonical_short = false;
        for name in &option.names {
            let name_kind = match token_kind(name) {
                "long" if !has_canonical_long => {
                    has_canonical_long = true;
                    "canonical"
                }
                "short" if !has_canonical_short => {
                    has_canonical_short = true;
                    "canonical"
                }
                _ => "visible_alias",
            };
            insert_option_name(transaction, option_id, ordinal, name, name_kind).await?;
            ordinal += 1;
        }
        for name in &option.aliases {
            insert_option_name(transaction, option_id, ordinal, name, "alias").await?;
            ordinal += 1;
        }
        for name in &option.visible_aliases {
            insert_option_name(transaction, option_id, ordinal, name, "visible_alias").await?;
            ordinal += 1;
        }

        insert_possible_values(
            transaction,
            option_id,
            &option.possible_values,
            &option.possible_values_help,
            &option.possible_values_hidden,
            true,
        )
        .await?;
        insert_candidates(transaction, option_id, -1, &option.candidates, true).await?;
        insert_value_completers(
            transaction,
            option_id,
            &option.value_completers,
            option.path_completion.as_ref(),
            true,
        )
        .await?;
    }
    Ok(options.len())
}

async fn insert_option_name(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    option_id: i64,
    ordinal: i64,
    name: &str,
    kind: &str,
) -> Result<()> {
    sqlx::query("INSERT INTO option_names (option_id, ordinal, name, name_kind, token_kind) VALUES (?, ?, ?, ?, ?)")
        .bind(option_id)
        .bind(ordinal)
        .bind(name)
        .bind(kind)
        .bind(token_kind(name))
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

async fn insert_arguments(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    command_id: i64,
    arguments: &[ArgumentSpec],
) -> Result<usize> {
    for argument in arguments {
        let result = sqlx::query(
            "INSERT INTO arguments (command_id, stable_id, position, help, long_help, value_name, value_names_json, value_hint, required, global_argument, multiple, hidden, hide_possible_values, value_delimiter, value_terminator, default_value, default_missing_value, require_equals, allow_hyphen_values, allow_negative_numbers, exclusive, last, trailing_var_arg, requires_json, conflicts_with_json, overrides_with_json, min_values, max_values) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(command_id)
        .bind(&argument.id)
        .bind(argument.position as i64)
        .bind(&argument.help)
        .bind(&argument.long_help)
        .bind(&argument.value_name)
        .bind(json(&argument.value_names)?)
        .bind(value_hint(argument.value_hint))
        .bind(bool_int(argument.required))
        .bind(bool_int(argument.global))
        .bind(bool_int(argument.multiple))
        .bind(bool_int(argument.hidden))
        .bind(bool_int(argument.hide_possible_values))
        .bind(argument.value_delimiter.as_deref())
        .bind(argument.value_terminator.as_deref())
        .bind(&argument.default_value)
        .bind(&argument.default_missing_value)
        .bind(bool_int(argument.require_equals))
        .bind(bool_int(argument.allow_hyphen_values))
        .bind(bool_int(argument.allow_negative_numbers))
        .bind(bool_int(argument.exclusive))
        .bind(bool_int(argument.last))
        .bind(bool_int(argument.trailing_var_arg))
        .bind(json(&argument.requires)?)
        .bind(json(&argument.conflicts_with)?)
        .bind(json(&argument.overrides_with)?)
        .bind(argument.min_values.map(|value| value as i64))
        .bind(argument.max_values.map(|value| value as i64))
        .execute(&mut **transaction)
        .await?;
        let argument_id = result.last_insert_rowid();
        insert_possible_values(
            transaction,
            argument_id,
            &argument.possible_values,
            &argument.possible_values_help,
            &argument.possible_values_hidden,
            false,
        )
        .await?;
        insert_candidates(transaction, argument_id, -1, &argument.candidates, false).await?;
        insert_value_completers(
            transaction,
            argument_id,
            &argument.value_completers,
            argument.path_completion.as_ref(),
            false,
        )
        .await?;
    }
    Ok(arguments.len())
}

async fn insert_possible_values(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    owner_id: i64,
    values: &[String],
    helps: &[String],
    hidden: &[bool],
    option: bool,
) -> Result<()> {
    for (ordinal, value) in values.iter().enumerate() {
        let help = helps.get(ordinal);
        let hidden = hidden.get(ordinal).copied().unwrap_or(false);
        insert_value(
            transaction,
            owner_id,
            -1,
            ordinal as i64,
            value,
            None,
            help,
            None,
            None,
            None,
            hidden,
            "possible",
            option,
        )
        .await?;
    }
    Ok(())
}

async fn insert_candidates(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    owner_id: i64,
    value_index: i64,
    candidates: &[CandidateSpec],
    option: bool,
) -> Result<()> {
    for (ordinal, candidate) in candidates.iter().enumerate() {
        insert_value(
            transaction,
            owner_id,
            value_index,
            ordinal as i64,
            &candidate.value,
            candidate.prefix.as_ref(),
            candidate.help.as_ref(),
            candidate.id.as_ref(),
            candidate.tag.as_ref(),
            candidate.display_order.map(|value| value as i64),
            candidate.hidden,
            "candidate",
            option,
        )
        .await?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_value(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    owner_id: i64,
    value_index: i64,
    ordinal: i64,
    value: &str,
    prefix: Option<&String>,
    help: Option<&String>,
    candidate_id: Option<&String>,
    tag: Option<&String>,
    display_order: Option<i64>,
    hidden: bool,
    value_kind: &str,
    option: bool,
) -> Result<()> {
    let sql = if option {
        "INSERT INTO option_values (option_id, value_index, ordinal, value, prefix, help, candidate_id, tag, display_order, hidden, value_kind) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    } else {
        "INSERT INTO argument_values (argument_id, value_index, ordinal, value, prefix, help, candidate_id, tag, display_order, hidden, value_kind) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    };
    sqlx::query(sql)
        .bind(owner_id)
        .bind(value_index)
        .bind(ordinal)
        .bind(value)
        .bind(prefix)
        .bind(help)
        .bind(candidate_id)
        .bind(tag)
        .bind(display_order)
        .bind(bool_int(hidden))
        .bind(value_kind)
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

async fn insert_value_completers(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    owner_id: i64,
    completers: &[ValueCompletionSpec],
    path: Option<&crate::model::PathCompletionSpec>,
    option: bool,
) -> Result<()> {
    for completer in completers {
        match completer {
            ValueCompletionSpec::Candidates {
                arg_index,
                candidates,
            } => {
                insert_completer(
                    transaction,
                    owner_id,
                    arg_index.map(|value| value as i64).unwrap_or(-1),
                    "candidates",
                    None,
                    false,
                    None,
                    option,
                )
                .await?;
                insert_candidates(
                    transaction,
                    owner_id,
                    arg_index.map(|value| value as i64).unwrap_or(-1),
                    candidates,
                    option,
                )
                .await?;
            }
            ValueCompletionSpec::Path {
                arg_index,
                path_kind,
                stdio,
                current_dir,
            } => {
                insert_completer(
                    transaction,
                    owner_id,
                    arg_index.map(|value| value as i64).unwrap_or(-1),
                    "path",
                    Some(path_kind),
                    *stdio,
                    current_dir.as_deref(),
                    option,
                )
                .await?;
            }
        }
    }
    if let Some(path) = path {
        insert_completer(
            transaction,
            owner_id,
            -1,
            "path",
            Some(&path.kind),
            path.stdio,
            path.current_dir.as_deref(),
            option,
        )
        .await?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_completer(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    owner_id: i64,
    value_index: i64,
    kind: &str,
    path_kind: Option<&PathKind>,
    stdio: bool,
    current_dir: Option<&str>,
    option: bool,
) -> Result<()> {
    let sql = if option {
        "INSERT INTO option_completers (option_id, value_index, completer_kind, path_kind, path_stdio, path_current_dir) VALUES (?, ?, ?, ?, ?, ?)"
    } else {
        "INSERT INTO argument_completers (argument_id, value_index, completer_kind, path_kind, path_stdio, path_current_dir) VALUES (?, ?, ?, ?, ?, ?)"
    };
    sqlx::query(sql)
        .bind(owner_id)
        .bind(value_index)
        .bind(kind)
        .bind(path_kind.map(path_kind_name))
        .bind(bool_int(stdio))
        .bind(current_dir)
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

async fn insert_command_candidates(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    command_id: i64,
    candidates: &[CandidateSpec],
) -> Result<()> {
    for (ordinal, candidate) in candidates.iter().enumerate() {
        sqlx::query(
            "INSERT INTO command_candidates (command_id, ordinal, value, prefix, help, candidate_id, tag, display_order, hidden) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(command_id)
        .bind(ordinal as i64)
        .bind(&candidate.value)
        .bind(&candidate.prefix)
        .bind(&candidate.help)
        .bind(&candidate.id)
        .bind(&candidate.tag)
        .bind(candidate.display_order.map(|value| value as i64))
        .bind(bool_int(candidate.hidden))
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
}

fn bool_int(value: bool) -> i64 {
    i64::from(value)
}

fn token_kind(name: &str) -> &'static str {
    if name.starts_with("--") {
        "long"
    } else if name.starts_with('-') && name.chars().count() == 2 {
        "short"
    } else {
        "other"
    }
}

fn option_action(action: OptionAction) -> &'static str {
    match action {
        OptionAction::Flag => "flag",
        OptionAction::Value => "value",
        OptionAction::Append => "append",
        OptionAction::SetTrue => "set_true",
        OptionAction::SetFalse => "set_false",
        OptionAction::Count => "count",
        OptionAction::Help => "help",
        OptionAction::HelpShort => "help_short",
        OptionAction::HelpLong => "help_long",
    }
}

fn value_hint(hint: crate::model::ValueHintSpec) -> &'static str {
    match hint {
        crate::model::ValueHintSpec::Unknown => "unknown",
        crate::model::ValueHintSpec::Other => "other",
        crate::model::ValueHintSpec::AnyPath => "any_path",
        crate::model::ValueHintSpec::FilePath => "file_path",
        crate::model::ValueHintSpec::DirPath => "dir_path",
        crate::model::ValueHintSpec::ExecutablePath => "executable_path",
        crate::model::ValueHintSpec::CommandName => "command_name",
        crate::model::ValueHintSpec::CommandString => "command_string",
        crate::model::ValueHintSpec::CommandWithArguments => "command_with_arguments",
        crate::model::ValueHintSpec::Username => "username",
        crate::model::ValueHintSpec::Hostname => "hostname",
        crate::model::ValueHintSpec::Url => "url",
        crate::model::ValueHintSpec::EmailAddress => "email_address",
    }
}

fn path_kind_name(kind: &PathKind) -> &'static str {
    match kind {
        PathKind::Any => "any",
        PathKind::File => "file",
        PathKind::Dir => "dir",
    }
}

fn candidate_count(spec: &CommandSpec) -> usize {
    spec.subcommand_candidates.len()
        + spec
            .options
            .iter()
            .map(|option| {
                option.candidates.len()
                    + option
                        .value_completers
                        .iter()
                        .map(value_completer_candidate_count)
                        .sum::<usize>()
            })
            .sum::<usize>()
        + spec
            .arguments
            .iter()
            .map(|argument| {
                argument.candidates.len()
                    + argument
                        .value_completers
                        .iter()
                        .map(value_completer_candidate_count)
                        .sum::<usize>()
            })
            .sum::<usize>()
}

fn value_completer_candidate_count(completer: &ValueCompletionSpec) -> usize {
    match completer {
        ValueCompletionSpec::Candidates { candidates, .. } => candidates.len(),
        ValueCompletionSpec::Path { .. } => 0,
    }
}
