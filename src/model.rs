use std::collections::HashSet;

use anyhow::{Context, Result, bail};
use minijinja::Environment;
use regex::Regex;
use serde::Deserialize;

pub const PLAIN_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Deserialize)]
pub struct Manifest {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub command: CommandSpec,
}

#[derive(Debug, Deserialize)]
pub struct CommandSpec {
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub visible_aliases: Vec<String>,
    #[serde(default)]
    pub binary: Option<String>,
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub long_description: Option<String>,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub position: usize,
    #[serde(default, deserialize_with = "deserialize_string_or_strings")]
    pub supported_versions: Vec<String>,
    #[serde(default)]
    pub version_commands: Vec<Vec<String>>,
    #[serde(default)]
    pub version_preprocessors: Vec<VersionPreprocessSpec>,
    #[serde(default)]
    pub options: Vec<OptionSpec>,
    #[serde(default)]
    pub arguments: Vec<ArgumentSpec>,
    #[serde(default)]
    pub subcommand_candidates: Vec<CandidateSpec>,
    #[serde(flatten)]
    pub settings: CommandSettings,
}

#[derive(Debug, Default, Deserialize)]
pub struct CommandSettings {
    #[serde(default)]
    pub subcommand_required: bool,
    #[serde(default)]
    pub arg_required_else_help: bool,
    #[serde(default)]
    pub subcommand_precedence_over_arg: bool,
    #[serde(default)]
    pub infer_subcommands: bool,
    #[serde(default)]
    pub disable_help_subcommand: bool,
    #[serde(default)]
    pub allow_external_subcommands: bool,
    #[serde(default)]
    pub args_conflicts_with_subcommands: bool,
    #[serde(default)]
    pub subcommand_negates_reqs: bool,
    #[serde(default)]
    pub multicall: bool,
    #[serde(default)]
    pub no_binary_name: bool,
    #[serde(default)]
    pub disable_help_flag: bool,
    #[serde(default)]
    pub disable_version_flag: bool,
}

#[derive(Debug, Deserialize)]
pub struct OptionSpec {
    pub id: String,
    pub names: Vec<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub visible_aliases: Vec<String>,
    #[serde(default)]
    pub action: OptionAction,
    #[serde(default)]
    pub help: Option<String>,
    #[serde(default)]
    pub long_help: Option<String>,
    #[serde(default)]
    pub value_name: Option<String>,
    #[serde(default)]
    pub value_names: Vec<String>,
    #[serde(default)]
    pub value_hint: ValueHintSpec,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub global: bool,
    #[serde(default)]
    pub multiple: bool,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub hide_possible_values: bool,
    #[serde(default)]
    pub value_delimiter: Option<String>,
    #[serde(default)]
    pub value_terminator: Option<String>,
    #[serde(default)]
    pub default_value: Option<String>,
    #[serde(default)]
    pub default_missing_value: Option<String>,
    #[serde(default)]
    pub require_equals: bool,
    #[serde(default)]
    pub allow_hyphen_values: bool,
    #[serde(default)]
    pub allow_negative_numbers: bool,
    #[serde(default)]
    pub exclusive: bool,
    #[serde(default)]
    pub last: bool,
    #[serde(default)]
    pub trailing_var_arg: bool,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub conflicts_with: Vec<String>,
    #[serde(default)]
    pub overrides_with: Vec<String>,
    #[serde(default)]
    pub min_values: Option<usize>,
    #[serde(default)]
    pub max_values: Option<usize>,
    #[serde(default)]
    pub possible_values: Vec<String>,
    #[serde(default)]
    pub possible_values_help: Vec<String>,
    #[serde(default)]
    pub possible_values_hidden: Vec<bool>,
    #[serde(default)]
    pub candidates: Vec<CandidateSpec>,
    #[serde(default)]
    pub value_completers: Vec<ValueCompletionSpec>,
    #[serde(default)]
    pub path_completion: Option<PathCompletionSpec>,
}

#[derive(Debug, Deserialize)]
pub struct ArgumentSpec {
    pub id: String,
    pub position: usize,
    #[serde(default)]
    pub help: Option<String>,
    #[serde(default)]
    pub long_help: Option<String>,
    #[serde(default)]
    pub value_name: Option<String>,
    #[serde(default)]
    pub value_names: Vec<String>,
    #[serde(default)]
    pub value_hint: ValueHintSpec,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub global: bool,
    #[serde(default)]
    pub multiple: bool,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub hide_possible_values: bool,
    #[serde(default)]
    pub value_delimiter: Option<String>,
    #[serde(default)]
    pub value_terminator: Option<String>,
    #[serde(default)]
    pub default_value: Option<String>,
    #[serde(default)]
    pub default_missing_value: Option<String>,
    #[serde(default)]
    pub require_equals: bool,
    #[serde(default)]
    pub allow_hyphen_values: bool,
    #[serde(default)]
    pub allow_negative_numbers: bool,
    #[serde(default)]
    pub exclusive: bool,
    #[serde(default)]
    pub last: bool,
    #[serde(default)]
    pub trailing_var_arg: bool,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub conflicts_with: Vec<String>,
    #[serde(default)]
    pub overrides_with: Vec<String>,
    #[serde(default)]
    pub min_values: Option<usize>,
    #[serde(default)]
    pub max_values: Option<usize>,
    #[serde(default)]
    pub possible_values: Vec<String>,
    #[serde(default)]
    pub possible_values_help: Vec<String>,
    #[serde(default)]
    pub possible_values_hidden: Vec<bool>,
    #[serde(default)]
    pub candidates: Vec<CandidateSpec>,
    #[serde(default)]
    pub value_completers: Vec<ValueCompletionSpec>,
    #[serde(default)]
    pub path_completion: Option<PathCompletionSpec>,
}

#[derive(Debug, Default, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum OptionAction {
    #[default]
    Flag,
    Value,
    Append,
    SetTrue,
    SetFalse,
    Count,
    Help,
    HelpShort,
    HelpLong,
}

impl OptionAction {
    pub fn takes_value(self) -> bool {
        matches!(self, Self::Value | Self::Append)
    }
}

#[derive(Debug, Default, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum ValueHintSpec {
    #[default]
    Unknown,
    Other,
    AnyPath,
    FilePath,
    DirPath,
    ExecutablePath,
    CommandName,
    CommandString,
    CommandWithArguments,
    Username,
    Hostname,
    Url,
    EmailAddress,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CandidateSpec {
    pub value: String,
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub help: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub display_order: Option<usize>,
    #[serde(default)]
    pub hidden: bool,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ValueCompletionSpec {
    Candidates {
        #[serde(default)]
        arg_index: Option<usize>,
        #[serde(default)]
        candidates: Vec<CandidateSpec>,
    },
    Path {
        #[serde(default)]
        arg_index: Option<usize>,
        path_kind: PathKind,
        #[serde(default)]
        stdio: bool,
        #[serde(default)]
        current_dir: Option<String>,
    },
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum PathKind {
    Any,
    File,
    Dir,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "engine", rename_all = "lowercase")]
pub enum VersionPreprocessSpec {
    Regex { pattern: String, replace: String },
    Minijinja { template: String },
}

fn default_schema_version() -> u32 {
    PLAIN_SCHEMA_VERSION
}

fn deserialize_string_or_strings<'de, D>(
    deserializer: D,
) -> std::result::Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(String),
        Many(Vec<String>),
    }

    OneOrMany::deserialize(deserializer).map(|value| match value {
        OneOrMany::One(value) => vec![value],
        OneOrMany::Many(value) => value,
    })
}

impl Manifest {
    pub fn validate(&self, root: bool, source: &str) -> Result<()> {
        if self.schema_version != PLAIN_SCHEMA_VERSION {
            bail!(
                "{source}: unsupported schema_version {}, expected {}",
                self.schema_version,
                PLAIN_SCHEMA_VERSION
            );
        }
        self.command.validate(root, source)
    }
}

impl CommandSpec {
    fn validate(&self, root: bool, source: &str) -> Result<()> {
        validate_identifier(&self.name, "command.name", source)?;
        validate_names(&self.aliases, "command.aliases", source)?;
        validate_names(&self.visible_aliases, "command.visible_aliases", source)?;
        ensure_disjoint(
            std::iter::once(self.name.as_str())
                .chain(self.aliases.iter().map(String::as_str))
                .chain(self.visible_aliases.iter().map(String::as_str)),
            "command names",
            source,
        )?;

        if root {
            if self.supported_versions.is_empty() {
                bail!("{source}: root command.supported_versions must not be empty");
            }
            let wildcard_count = self
                .supported_versions
                .iter()
                .filter(|version| version.as_str() == "*")
                .count();
            if wildcard_count > 0 {
                if self.supported_versions.len() != 1 {
                    bail!("{source}: `*` must be the only supported_versions entry");
                }
            } else if self.version_commands.is_empty() {
                bail!("{source}: version_commands is required when supported_versions is not `*`");
            }
            for (index, command) in self.version_commands.iter().enumerate() {
                if command.is_empty() || command.iter().any(String::is_empty) {
                    bail!("{source}: version_commands[{index}] must contain non-empty argv");
                }
            }
            validate_version_preprocessors(&self.version_preprocessors, source)?;
        } else if !self.supported_versions.is_empty()
            || !self.version_commands.is_empty()
            || !self.version_preprocessors.is_empty()
        {
            bail!(
                "{source}: supported_versions/version_commands/version_preprocessors are only valid on root manifests"
            );
        }

        validate_options(&self.options, source)?;
        validate_arguments(&self.arguments, source)?;
        validate_argument_ids(&self.options, &self.arguments, source)?;
        validate_candidates(&self.subcommand_candidates, "subcommand_candidates", source)?;
        Ok(())
    }
}

fn validate_options(options: &[OptionSpec], source: &str) -> Result<()> {
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for option in options {
        validate_identifier(&option.id, "option.id", source)?;
        if !ids.insert(&option.id) {
            bail!("{source}: duplicate option id `{}`", option.id);
        }
        if option.names.is_empty() {
            bail!(
                "{source}: option `{}` must have at least one name",
                option.id
            );
        }
        validate_option_names(
            &option.names,
            &option.aliases,
            &option.visible_aliases,
            source,
        )?;
        for name in option
            .names
            .iter()
            .chain(option.aliases.iter())
            .chain(option.visible_aliases.iter())
        {
            if !names.insert(name) {
                bail!("{source}: duplicate option name `{name}`");
            }
        }
        validate_value_fields(
            option.action,
            option.value_name.as_deref(),
            option.value_names.len(),
            &option.possible_values,
            &option.possible_values_help,
            &option.possible_values_hidden,
            &option.candidates,
            &option.value_completers,
            option.path_completion.as_ref(),
            &option.value_delimiter,
            &option.value_terminator,
            &option.default_value,
            &option.default_missing_value,
            option.require_equals,
            option.allow_hyphen_values,
            option.allow_negative_numbers,
            option.min_values,
            option.max_values,
            source,
            &option.id,
        )?;
        if !option.action.takes_value() && !matches!(option.value_hint, ValueHintSpec::Unknown) {
            bail!(
                "{source}: flag `{}` cannot declare a non-unknown value_hint",
                option.id
            );
        }
        if option.global && option.required {
            bail!("{source}: global option `{}` cannot be required", option.id);
        }
        if option.last || option.trailing_var_arg {
            bail!(
                "{source}: option `{}` cannot use last or trailing_var_arg",
                option.id
            );
        }
        validate_relations(
            &option.requires,
            &option.conflicts_with,
            &option.overrides_with,
            source,
            &option.id,
        )?;
    }
    Ok(())
}

fn validate_arguments(arguments: &[ArgumentSpec], source: &str) -> Result<()> {
    let mut ids = HashSet::new();
    let mut positions = HashSet::new();
    let highest_position = arguments.iter().map(|argument| argument.position).max();
    for argument in arguments {
        validate_identifier(&argument.id, "argument.id", source)?;
        if !ids.insert(&argument.id) {
            bail!("{source}: duplicate argument id `{}`", argument.id);
        }
        if argument.position == 0 || !positions.insert(argument.position) {
            bail!(
                "{source}: argument `{}` has duplicate or zero position",
                argument.id
            );
        }
        validate_value_fields(
            OptionAction::Value,
            argument.value_name.as_deref(),
            argument.value_names.len(),
            &argument.possible_values,
            &argument.possible_values_help,
            &argument.possible_values_hidden,
            &argument.candidates,
            &argument.value_completers,
            argument.path_completion.as_ref(),
            &argument.value_delimiter,
            &argument.value_terminator,
            &argument.default_value,
            &argument.default_missing_value,
            argument.require_equals,
            argument.allow_hyphen_values,
            argument.allow_negative_numbers,
            argument.min_values,
            argument.max_values,
            source,
            &argument.id,
        )?;
        if argument.global && argument.required {
            bail!(
                "{source}: global argument `{}` cannot be required",
                argument.id
            );
        }
        if argument.last && argument.trailing_var_arg {
            bail!(
                "{source}: argument `{}` cannot use both last and trailing_var_arg",
                argument.id
            );
        }
        if argument.trailing_var_arg && Some(argument.position) != highest_position {
            bail!(
                "{source}: trailing_var_arg must be set on the highest positional argument (`{}`)",
                argument.id
            );
        }
        if matches!(argument.value_hint, ValueHintSpec::CommandWithArguments)
            && (!argument.multiple || (!argument.last && !argument.trailing_var_arg))
        {
            bail!(
                "{source}: argument `{}` with value_hint command_with_arguments must be multiple and use last or trailing_var_arg",
                argument.id
            );
        }
        validate_relations(
            &argument.requires,
            &argument.conflicts_with,
            &argument.overrides_with,
            source,
            &argument.id,
        )?;
    }
    if let Some(highest) = highest_position
        && highest != positions.len()
    {
        bail!("{source}: positional argument positions must be continuous from 1 to {highest}");
    }
    Ok(())
}

fn validate_argument_ids(
    options: &[OptionSpec],
    arguments: &[ArgumentSpec],
    source: &str,
) -> Result<()> {
    let mut ids = HashSet::new();
    for id in options.iter().map(|option| &option.id) {
        ids.insert(id);
    }
    for argument in arguments {
        if !ids.insert(&argument.id) {
            bail!(
                "{source}: option and argument ids must be unique; `{}` is duplicated",
                argument.id
            );
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_value_fields(
    action: OptionAction,
    value_name: Option<&str>,
    value_name_count: usize,
    possible_values: &[String],
    possible_values_help: &[String],
    possible_values_hidden: &[bool],
    candidates: &[CandidateSpec],
    value_completers: &[ValueCompletionSpec],
    path_completion: Option<&PathCompletionSpec>,
    value_delimiter: &Option<String>,
    value_terminator: &Option<String>,
    default_value: &Option<String>,
    default_missing_value: &Option<String>,
    require_equals: bool,
    allow_hyphen_values: bool,
    allow_negative_numbers: bool,
    min_values: Option<usize>,
    max_values: Option<usize>,
    source: &str,
    id: &str,
) -> Result<()> {
    if let (Some(minimum), Some(maximum)) = (min_values, max_values)
        && minimum > maximum
    {
        bail!("{source}: `{id}` min_values cannot exceed max_values");
    }
    if !action.takes_value()
        && (value_name.is_some()
            || value_name_count > 0
            || value_delimiter.is_some()
            || value_terminator.is_some()
            || default_value.is_some()
            || default_missing_value.is_some()
            || require_equals
            || allow_hyphen_values
            || allow_negative_numbers
            || min_values.is_some()
            || max_values.is_some())
    {
        bail!("{source}: flag `{id}` cannot declare value-only settings");
    }
    if (!possible_values_help.is_empty() && possible_values_help.len() != possible_values.len())
        || (!possible_values_hidden.is_empty()
            && possible_values_hidden.len() != possible_values.len())
    {
        bail!("{source}: possible_values help/hidden arrays for `{id}` must match possible_values");
    }
    if !possible_values.is_empty() && (!candidates.is_empty() || !value_completers.is_empty()) {
        bail!("{source}: `{id}` cannot combine possible_values with candidates/value_completers");
    }
    if !candidates.is_empty() && !value_completers.is_empty() {
        bail!("{source}: `{id}` cannot combine candidates with value_completers");
    }
    if path_completion.is_some() && (!candidates.is_empty() || !value_completers.is_empty()) {
        bail!("{source}: `{id}` cannot combine path_completion with candidates/value_completers");
    }
    if !action.takes_value()
        && (!possible_values.is_empty()
            || !candidates.is_empty()
            || !value_completers.is_empty()
            || path_completion.is_some())
    {
        bail!("{source}: flag `{id}` cannot declare value completion");
    }
    if let Some(delimiter) = value_delimiter {
        let mut chars = delimiter.chars();
        if chars.next().is_none() || chars.next().is_some() {
            bail!("{source}: `{id}` value_delimiter must be exactly one character");
        }
    }
    let mut indexes = HashSet::new();
    for completer in value_completers {
        let index = match completer {
            ValueCompletionSpec::Candidates { arg_index, .. }
            | ValueCompletionSpec::Path { arg_index, .. } => *arg_index,
        };
        if !indexes.insert(index) {
            bail!("{source}: `{id}` has duplicate value completer index {index:?}");
        }
        if let ValueCompletionSpec::Candidates { candidates, .. } = completer {
            validate_candidates(candidates, "value_completers.candidates", source)?;
        }
    }
    validate_candidates(candidates, "candidates", source)
}

fn validate_relations(
    requires: &[String],
    conflicts_with: &[String],
    overrides_with: &[String],
    source: &str,
    id: &str,
) -> Result<()> {
    for relation in requires
        .iter()
        .chain(conflicts_with.iter())
        .chain(overrides_with.iter())
    {
        if relation.is_empty() {
            bail!("{source}: `{id}` contains an empty relation id");
        }
    }
    Ok(())
}

fn validate_option_names(
    names: &[String],
    aliases: &[String],
    visible_aliases: &[String],
    source: &str,
) -> Result<()> {
    validate_names(names, "option.names", source)?;
    validate_names(aliases, "option.aliases", source)?;
    validate_names(visible_aliases, "option.visible_aliases", source)?;
    ensure_disjoint(
        names
            .iter()
            .chain(aliases.iter())
            .chain(visible_aliases.iter())
            .map(String::as_str),
        "option names",
        source,
    )
}

fn validate_names(names: &[String], field: &str, source: &str) -> Result<()> {
    for name in names {
        if name.is_empty() {
            bail!("{source}: {field} contains an empty name");
        }
    }
    Ok(())
}

fn validate_identifier(value: &str, field: &str, source: &str) -> Result<()> {
    if value.is_empty() || value.contains('/') || value.contains('\\') {
        bail!("{source}: {field} must be a non-empty path-safe string");
    }
    Ok(())
}

fn ensure_disjoint<'a>(
    values: impl IntoIterator<Item = &'a str>,
    field: &str,
    source: &str,
) -> Result<()> {
    let mut seen = HashSet::new();
    for value in values {
        if !seen.insert(value) {
            bail!("{source}: duplicate {field} entry `{value}`");
        }
    }
    Ok(())
}

fn validate_candidates(candidates: &[CandidateSpec], field: &str, source: &str) -> Result<()> {
    let mut values = HashSet::new();
    for candidate in candidates {
        if candidate.value.is_empty() {
            bail!("{source}: {field} contains an empty value");
        }
        if !values.insert(&candidate.value) {
            bail!("{source}: duplicate {field} value `{}`", candidate.value);
        }
    }
    Ok(())
}

fn validate_version_preprocessors(rules: &[VersionPreprocessSpec], source: &str) -> Result<()> {
    for (index, rule) in rules.iter().enumerate() {
        match rule {
            VersionPreprocessSpec::Regex { pattern, .. } => {
                Regex::new(pattern)
                    .with_context(|| format!("{source}: invalid regex at preprocess[{index}]"))?;
            }
            VersionPreprocessSpec::Minijinja { template } => {
                let environment = Environment::new();
                environment.template_from_str(template).with_context(|| {
                    format!("{source}: invalid minijinja template at preprocess[{index}]")
                })?;
            }
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct PathCompletionSpec {
    pub kind: PathKind,
    #[serde(default)]
    pub stdio: bool,
    #[serde(default)]
    pub current_dir: Option<String>,
}
