use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use anyhow::{Result, anyhow, bail};
use clap::builder::{PossibleValue, PossibleValuesParser};
use clap::{Arg, ArgAction, Command, ValueHint};
use clap_complete::engine::{
    ArgValueCandidates, ArgValueCompleter, CompletionCandidate, PathCompleter,
    SubcommandCandidates, ValueCompleter,
};

use crate::database::{
    ArgumentRow, CommandCandidateRow, CommandNameRow, CommandRow, CompleterRow, LoadedBundle,
    OptionNameRow, OptionRow, ValueRow,
};

pub fn build_command(bundle: &LoadedBundle) -> Result<Command> {
    let root = bundle
        .commands
        .iter()
        .find(|command| command.parent_id.is_none())
        .ok_or_else(|| anyhow!("database version has no root command"))?;
    let commands = bundle
        .commands
        .iter()
        .map(|command| (command.id, command))
        .collect::<HashMap<_, _>>();
    let command_names = group_command_names(&bundle.command_names);
    let options = group_options(&bundle.options);
    let option_names = group_option_names(&bundle.option_names);
    let arguments = group_arguments(&bundle.arguments);
    let option_values = group_values(&bundle.option_values);
    let argument_values = group_values(&bundle.argument_values);
    let command_candidates = group_command_candidates(&bundle.command_candidates);
    let option_completers = group_completers(&bundle.option_completers);
    let argument_completers = group_completers(&bundle.argument_completers);

    build_command_node(
        root,
        &commands,
        &command_names,
        &options,
        &option_names,
        &arguments,
        &option_values,
        &argument_values,
        &option_completers,
        &argument_completers,
        &command_candidates,
        &bundle.version.binary_name,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_command_node(
    row: &CommandRow,
    commands: &HashMap<i64, &CommandRow>,
    command_names: &HashMap<i64, Vec<&CommandNameRow>>,
    options: &HashMap<i64, Vec<&OptionRow>>,
    option_names: &HashMap<i64, Vec<&OptionNameRow>>,
    arguments: &HashMap<i64, Vec<&ArgumentRow>>,
    option_values: &HashMap<i64, Vec<&ValueRow>>,
    argument_values: &HashMap<i64, Vec<&ValueRow>>,
    option_completers: &HashMap<i64, Vec<&CompleterRow>>,
    argument_completers: &HashMap<i64, Vec<&CompleterRow>>,
    command_candidates: &HashMap<i64, Vec<&CommandCandidateRow>>,
    binary_name: &str,
) -> Result<Command> {
    let mut command = Command::new(row.name.clone());
    if row.parent_id.is_none() {
        command = command.bin_name(binary_name.to_owned());
    }
    if let Some(about) = &row.about {
        command = command.about(about.clone());
    }
    if let Some(long_about) = &row.long_about {
        command = command.long_about(long_about.clone());
    }
    command = command
        .hide(row.hidden)
        .subcommand_required(row.subcommand_required)
        .arg_required_else_help(row.arg_required_else_help)
        .subcommand_precedence_over_arg(row.subcommand_precedence_over_arg)
        .infer_subcommands(row.infer_subcommands)
        .disable_help_subcommand(row.disable_help_subcommand)
        .allow_external_subcommands(row.allow_external_subcommands)
        .args_conflicts_with_subcommands(row.args_conflicts_with_subcommands)
        .subcommand_negates_reqs(row.subcommand_negates_reqs)
        .multicall(row.multicall)
        .no_binary_name(row.no_binary_name)
        .disable_help_flag(row.disable_help_flag)
        .disable_version_flag(row.disable_version_flag);

    for name in command_names.get(&row.id).into_iter().flatten() {
        match name.name_kind.as_str() {
            "canonical" => {}
            "alias" => command = command.alias(name.name.clone()),
            "visible_alias" => command = command.visible_alias(name.name.clone()),
            other => bail!("unknown command name kind `{other}`"),
        }
    }

    for option in options.get(&row.id).into_iter().flatten() {
        let names = option_names.get(&option.id).cloned().unwrap_or_default();
        let values = option_values.get(&option.id).cloned().unwrap_or_default();
        let completers = option_completers
            .get(&option.id)
            .cloned()
            .unwrap_or_default();
        command = command.arg(build_option(option, &names, &values, &completers)?);
    }
    for argument in arguments.get(&row.id).into_iter().flatten() {
        let values = argument_values
            .get(&argument.id)
            .cloned()
            .unwrap_or_default();
        let completers = argument_completers
            .get(&argument.id)
            .cloned()
            .unwrap_or_default();
        command = command.arg(build_argument(argument, &values, &completers)?);
    }

    let mut child_rows = commands
        .values()
        .filter(|child| child.parent_id == Some(row.id))
        .copied()
        .collect::<Vec<_>>();
    child_rows.sort_by_key(|child| (child.position, child.path.clone()));
    for child in child_rows {
        command = command.subcommand(build_command_node(
            child,
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
            binary_name,
        )?);
    }

    if let Some(candidates) = command_candidates.get(&row.id)
        && !candidates.is_empty()
    {
        let candidates = candidates
            .iter()
            .map(|candidate| candidate_data(candidate))
            .collect::<Vec<_>>();
        command = command.add(SubcommandCandidates::new(move || {
            candidates.iter().map(candidate_value).collect()
        }));
    }
    Ok(command)
}

fn build_option(
    row: &OptionRow,
    names: &[&OptionNameRow],
    values: &[&ValueRow],
    completers: &[&CompleterRow],
) -> Result<Arg> {
    let mut argument = Arg::new(row.stable_id.clone());
    argument = apply_option_names(argument, names)?;
    argument = apply_common_arg_fields(
        argument,
        row.help.as_deref(),
        row.long_help.as_deref(),
        row.value_name.as_deref(),
        &row.value_names,
        &row.value_hint,
        row.required,
        row.global,
        row.multiple,
        row.hidden,
        row.hide_possible_values,
        row.value_delimiter.as_deref(),
        row.value_terminator.as_deref(),
        row.default_value.as_deref(),
        row.default_missing_value.as_deref(),
        row.require_equals,
        row.allow_hyphen_values,
        row.allow_negative_numbers,
        row.exclusive,
        row.last,
        row.trailing_var_arg,
        row.min_values,
        row.max_values,
        &row.requires,
        &row.conflicts_with,
        &row.overrides_with,
    )?;
    argument = argument.action(arg_action(&row.action, row.multiple)?);
    apply_value_completion(argument, values, completers)
}

fn build_argument(
    row: &ArgumentRow,
    values: &[&ValueRow],
    completers: &[&CompleterRow],
) -> Result<Arg> {
    let mut argument = Arg::new(row.stable_id.clone()).index(row.position as usize);
    argument = apply_common_arg_fields(
        argument,
        row.help.as_deref(),
        row.long_help.as_deref(),
        row.value_name.as_deref(),
        &row.value_names,
        &row.value_hint,
        row.required,
        row.global,
        row.multiple,
        row.hidden,
        row.hide_possible_values,
        row.value_delimiter.as_deref(),
        row.value_terminator.as_deref(),
        row.default_value.as_deref(),
        row.default_missing_value.as_deref(),
        row.require_equals,
        row.allow_hyphen_values,
        row.allow_negative_numbers,
        row.exclusive,
        row.last,
        row.trailing_var_arg,
        row.min_values,
        row.max_values,
        &row.requires,
        &row.conflicts_with,
        &row.overrides_with,
    )?;
    argument = argument.action(if row.multiple {
        ArgAction::Append
    } else {
        ArgAction::Set
    });
    apply_value_completion(argument, values, completers)
}

fn apply_option_names(mut argument: Arg, names: &[&OptionNameRow]) -> Result<Arg> {
    let mut has_long = false;
    let mut has_short = false;
    for name in names {
        let token = parse_option_token(&name.name)?;
        match (name.name_kind.as_str(), token) {
            (_, OptionToken::Long(value)) if name.name_kind == "canonical" => {
                if has_long {
                    bail!("option has more than one canonical long name");
                }
                argument = argument.long(value);
                has_long = true;
            }
            (_, OptionToken::Short(value)) if name.name_kind == "canonical" => {
                if has_short {
                    bail!("option has more than one canonical short name");
                }
                argument = argument.short(value);
                has_short = true;
            }
            ("alias", OptionToken::Long(value)) => argument = argument.alias(value),
            ("visible_alias", OptionToken::Long(value)) => argument = argument.visible_alias(value),
            ("alias", OptionToken::Short(value)) => argument = argument.short_alias(value),
            ("visible_alias", OptionToken::Short(value)) => {
                argument = argument.visible_short_alias(value)
            }
            (_, OptionToken::Other(value)) => {
                bail!("option token `{value}` cannot be represented by clap")
            }
            (kind, _) => bail!("unknown option name kind `{kind}`"),
        }
    }
    Ok(argument)
}

enum OptionToken {
    Long(String),
    Short(char),
    Other(String),
}

fn parse_option_token(value: &str) -> Result<OptionToken> {
    if let Some(value) = value.strip_prefix("--")
        && !value.is_empty()
    {
        return Ok(OptionToken::Long(value.to_owned()));
    }
    if let Some(value) = value.strip_prefix('-')
        && value.chars().count() == 1
    {
        return Ok(OptionToken::Short(value.chars().next().expect("one char")));
    }
    Ok(OptionToken::Other(value.to_owned()))
}

#[allow(clippy::too_many_arguments)]
fn apply_common_arg_fields(
    mut argument: Arg,
    help: Option<&str>,
    long_help: Option<&str>,
    value_name: Option<&str>,
    value_names: &[String],
    value_hint: &str,
    required: bool,
    global: bool,
    multiple: bool,
    hidden: bool,
    hide_possible_values: bool,
    value_delimiter: Option<&str>,
    value_terminator: Option<&str>,
    default_value: Option<&str>,
    default_missing_value: Option<&str>,
    require_equals: bool,
    allow_hyphen_values: bool,
    allow_negative_numbers: bool,
    exclusive: bool,
    last: bool,
    trailing_var_arg: bool,
    min_values: Option<i64>,
    max_values: Option<i64>,
    requires: &[String],
    conflicts_with: &[String],
    overrides_with: &[String],
) -> Result<Arg> {
    if let Some(help) = help {
        argument = argument.help(help.to_owned());
    }
    if let Some(long_help) = long_help {
        argument = argument.long_help(long_help.to_owned());
    }
    if let Some(value_name) = value_name {
        argument = argument.value_name(value_name.to_owned());
    }
    if !value_names.is_empty() {
        argument = argument.value_names(value_names.iter().cloned());
    }
    argument = argument
        .value_hint(parse_value_hint(value_hint)?)
        .required(required)
        .global(global)
        .hide(hidden)
        .hide_possible_values(hide_possible_values)
        .require_equals(require_equals)
        .allow_hyphen_values(allow_hyphen_values)
        .allow_negative_numbers(allow_negative_numbers)
        .exclusive(exclusive)
        .last(last)
        .trailing_var_arg(trailing_var_arg);
    if let Some(delimiter) = value_delimiter {
        let character = one_char(delimiter, "value_delimiter")?;
        argument = argument.value_delimiter(character);
    }
    if let Some(terminator) = value_terminator {
        argument = argument.value_terminator(terminator.to_owned());
    }
    if let Some(default_value) = default_value {
        argument = argument.default_value(default_value.to_owned());
    }
    if let Some(default_missing_value) = default_missing_value {
        argument = argument.default_missing_value(default_missing_value.to_owned());
    }
    if let (Some(minimum), Some(maximum)) = (min_values, max_values) {
        argument =
            argument.num_args(to_usize(minimum, "min_values")?..=to_usize(maximum, "max_values")?);
    } else if let Some(minimum) = min_values {
        argument = argument.num_args(to_usize(minimum, "min_values")?..);
    } else if let Some(maximum) = max_values {
        argument = argument.num_args(..=to_usize(maximum, "max_values")?);
    }
    if !requires.is_empty() {
        argument = argument.requires_all(requires.iter().cloned());
    }
    if !conflicts_with.is_empty() {
        argument = argument.conflicts_with_all(conflicts_with.iter().cloned());
    }
    if !overrides_with.is_empty() {
        argument = argument.overrides_with_all(overrides_with.iter().cloned());
    }
    let _ = multiple;
    Ok(argument)
}

fn apply_value_completion(
    mut argument: Arg,
    values: &[&ValueRow],
    completers: &[&CompleterRow],
) -> Result<Arg> {
    let possible = values
        .iter()
        .filter(|value| value.value_index == -1 && value.value_kind == "possible")
        .map(|value| {
            let mut possible = PossibleValue::new(value.value.clone()).hide(value.hidden);
            if let Some(help) = &value.help {
                possible = possible.help(help.clone());
            }
            possible
        })
        .collect::<Vec<_>>();
    if !possible.is_empty() {
        argument = argument.value_parser(PossibleValuesParser::new(possible));
    }

    if completers.is_empty() {
        let candidates = values
            .iter()
            .filter(|value| value.value_index == -1 && value.value_kind == "candidate")
            .map(|value| candidate_data_from_value(value))
            .collect::<Vec<_>>();
        if !candidates.is_empty() {
            argument = argument.add(ArgValueCandidates::new(move || {
                candidates.iter().map(candidate_value).collect()
            }));
        }
        return Ok(argument);
    }

    let mut sources = BTreeMap::new();
    for completer in completers {
        let index = completer.value_index;
        let source = match completer.completer_kind.as_str() {
            "candidates" => CompletionSource::Candidates(
                values
                    .iter()
                    .filter(|value| value.value_index == index && value.value_kind == "candidate")
                    .map(|value| candidate_data_from_value(value))
                    .collect(),
            ),
            "path" => CompletionSource::Path(path_completer(completer)?),
            other => bail!("unknown completer kind `{other}`"),
        };
        sources.insert(index, source);
    }
    argument = argument.add(ArgValueCompleter::new(IndexedCompleter { sources }));
    Ok(argument)
}

enum CompletionSource {
    Candidates(Vec<CandidateData>),
    Path(PathCompleter),
}

struct IndexedCompleter {
    sources: BTreeMap<i64, CompletionSource>,
}

impl ValueCompleter for IndexedCompleter {
    fn complete(&self, current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
        match self.sources.get(&-1) {
            Some(CompletionSource::Candidates(candidates)) => {
                candidates.iter().map(candidate_value).collect()
            }
            Some(CompletionSource::Path(path)) => path.complete(current),
            None => Vec::new(),
        }
    }

    fn complete_at(&self, arg_index: usize, current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
        let source = self
            .sources
            .get(&(arg_index as i64))
            .or_else(|| self.sources.get(&-1));
        match source {
            Some(CompletionSource::Candidates(candidates)) => {
                candidates.iter().map(candidate_value).collect()
            }
            Some(CompletionSource::Path(path)) => path.complete(current),
            None => Vec::new(),
        }
    }
}

fn path_completer(row: &CompleterRow) -> Result<PathCompleter> {
    let mut completer = match row.path_kind.as_deref() {
        Some("any") => PathCompleter::any(),
        Some("file") => PathCompleter::file(),
        Some("dir") => PathCompleter::dir(),
        Some(other) => bail!("unknown path completer kind `{other}`"),
        None => bail!("path completer is missing path_kind"),
    };
    if row.path_stdio {
        completer = completer.stdio();
    }
    if let Some(current_dir) = &row.path_current_dir {
        completer = completer.current_dir(PathBuf::from(current_dir));
    }
    Ok(completer)
}

#[derive(Clone)]
struct CandidateData {
    value: String,
    prefix: Option<String>,
    help: Option<String>,
    id: Option<String>,
    tag: Option<String>,
    display_order: Option<usize>,
    hidden: bool,
}

fn candidate_data(row: &CommandCandidateRow) -> CandidateData {
    CandidateData {
        value: row.value.clone(),
        prefix: row.prefix.clone(),
        help: row.help.clone(),
        id: row.candidate_id.clone(),
        tag: row.tag.clone(),
        display_order: row
            .display_order
            .and_then(|value| usize::try_from(value).ok()),
        hidden: row.hidden,
    }
}

fn candidate_data_from_value(row: &ValueRow) -> CandidateData {
    CandidateData {
        value: row.value.clone(),
        prefix: row.prefix.clone(),
        help: row.help.clone(),
        id: row.candidate_id.clone(),
        tag: row.tag.clone(),
        display_order: row
            .display_order
            .and_then(|value| usize::try_from(value).ok()),
        hidden: row.hidden,
    }
}

fn candidate_value(candidate: &CandidateData) -> CompletionCandidate {
    let mut value = CompletionCandidate::new(candidate.value.clone())
        .id(candidate.id.clone())
        .tag(candidate.tag.clone().map(Into::into))
        .display_order(candidate.display_order)
        .hide(candidate.hidden);
    if let Some(prefix) = &candidate.prefix {
        value = value.add_prefix(prefix.clone());
    }
    if let Some(help) = &candidate.help {
        value = value.help(Some(help.clone().into()));
    }
    value
}

fn arg_action(action: &str, multiple: bool) -> Result<ArgAction> {
    Ok(match action {
        "flag" | "set_true" => ArgAction::SetTrue,
        "set_false" => ArgAction::SetFalse,
        "value" => {
            if multiple {
                ArgAction::Append
            } else {
                ArgAction::Set
            }
        }
        "append" => ArgAction::Append,
        "count" => ArgAction::Count,
        "help" => ArgAction::Help,
        "help_short" => ArgAction::HelpShort,
        "help_long" => ArgAction::HelpLong,
        other => bail!("unknown option action `{other}`"),
    })
}

fn parse_value_hint(value: &str) -> Result<ValueHint> {
    Ok(match value {
        "unknown" => ValueHint::Unknown,
        "other" => ValueHint::Other,
        "any_path" => ValueHint::AnyPath,
        "file_path" => ValueHint::FilePath,
        "dir_path" => ValueHint::DirPath,
        "executable_path" => ValueHint::ExecutablePath,
        "command_name" => ValueHint::CommandName,
        "command_string" => ValueHint::CommandString,
        "command_with_arguments" => ValueHint::CommandWithArguments,
        "username" => ValueHint::Username,
        "hostname" => ValueHint::Hostname,
        "url" => ValueHint::Url,
        "email_address" => ValueHint::EmailAddress,
        other => bail!("unknown value_hint `{other}`"),
    })
}

fn one_char(value: &str, field: &str) -> Result<char> {
    let mut chars = value.chars();
    let character = chars
        .next()
        .ok_or_else(|| anyhow!("{field} cannot be empty"))?;
    if chars.next().is_some() {
        bail!("{field} must contain one character");
    }
    Ok(character)
}

fn to_usize(value: i64, field: &str) -> Result<usize> {
    usize::try_from(value).map_err(|_| anyhow!("invalid {field} value {value}"))
}

fn group_command_names(rows: &[CommandNameRow]) -> HashMap<i64, Vec<&CommandNameRow>> {
    let mut grouped = HashMap::new();
    for row in rows {
        grouped
            .entry(row.command_id)
            .or_insert_with(Vec::new)
            .push(row);
    }
    grouped
}

fn group_options(rows: &[OptionRow]) -> HashMap<i64, Vec<&OptionRow>> {
    let mut grouped = HashMap::new();
    for row in rows {
        grouped
            .entry(row.command_id)
            .or_insert_with(Vec::new)
            .push(row);
    }
    grouped
}

fn group_option_names(rows: &[OptionNameRow]) -> HashMap<i64, Vec<&OptionNameRow>> {
    let mut grouped = HashMap::new();
    for row in rows {
        grouped
            .entry(row.option_id)
            .or_insert_with(Vec::new)
            .push(row);
    }
    grouped
}

fn group_arguments(rows: &[ArgumentRow]) -> HashMap<i64, Vec<&ArgumentRow>> {
    let mut grouped = HashMap::new();
    for row in rows {
        grouped
            .entry(row.command_id)
            .or_insert_with(Vec::new)
            .push(row);
    }
    grouped
}

fn group_values(rows: &[ValueRow]) -> HashMap<i64, Vec<&ValueRow>> {
    let mut grouped = HashMap::new();
    for row in rows {
        grouped
            .entry(row.owner_id)
            .or_insert_with(Vec::new)
            .push(row);
    }
    grouped
}

fn group_completers(rows: &[CompleterRow]) -> HashMap<i64, Vec<&CompleterRow>> {
    let mut grouped = HashMap::new();
    for row in rows {
        grouped
            .entry(row.owner_id)
            .or_insert_with(Vec::new)
            .push(row);
    }
    grouped
}

fn group_command_candidates(
    rows: &[CommandCandidateRow],
) -> HashMap<i64, Vec<&CommandCandidateRow>> {
    let mut grouped = HashMap::new();
    for row in rows {
        grouped
            .entry(row.command_id)
            .or_insert_with(Vec::new)
            .push(row);
    }
    grouped
}
