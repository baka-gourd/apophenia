-- SQLite query contract for apophenia-db-v1.
-- Runtime selection always binds (application, internal_version) and never
-- evaluates support_rules. The runtime loader executes these queries once,
-- then closes SQLite and completes from the in-memory clap::Command.

-- name: app_version_by_key
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
WHERE a.name = ?
  AND av.internal_version = ?
  AND a.enabled = 1;

-- name: application_versions_for_install
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
WHERE a.name = ?
  AND a.enabled = 1
ORDER BY av.internal_version;

-- name: all_application_versions_for_install
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
ORDER BY a.name, av.internal_version;

-- The following metadata queries are used by install, not by a completion
-- request.
-- name: support_rules_for_version
SELECT id, expression, kind, normalized_expression, specificity
FROM support_rules
WHERE app_version_id = ?
ORDER BY specificity DESC, id;

-- name: version_commands_for_version
SELECT ordinal, argv_json
FROM version_commands
WHERE app_version_id = ?
ORDER BY ordinal;

-- name: version_preprocessors_for_version
SELECT ordinal, engine, pattern, replacement, template
FROM version_preprocessors
WHERE app_version_id = ?
ORDER BY ordinal;

-- name: commands_for_version
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
ORDER BY length(path) - length(replace(path, '/', '')), path, position, name;

-- name: command_names_for_version
SELECT cn.command_id, cn.ordinal, cn.name, cn.name_kind
FROM command_names AS cn
JOIN commands AS c ON c.id = cn.command_id
WHERE c.app_version_id = ?
ORDER BY cn.command_id, cn.ordinal;

-- name: options_for_version
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
ORDER BY o.command_id, o.position, o.id;

-- name: option_names_for_version
SELECT n.option_id, n.ordinal, n.name, n.name_kind, n.token_kind
FROM option_names AS n
JOIN options AS o ON o.id = n.option_id
JOIN commands AS c ON c.id = o.command_id
WHERE c.app_version_id = ?
ORDER BY n.option_id, n.ordinal;

-- name: arguments_for_version
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
ORDER BY a.command_id, a.position, a.id;

-- name: option_values_for_version
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
ORDER BY v.option_id, v.value_index, v.ordinal;

-- name: argument_values_for_version
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
ORDER BY v.argument_id, v.value_index, v.ordinal;

-- name: option_completers_for_version
SELECT option_id, value_index, completer_kind, path_kind, path_stdio, path_current_dir
FROM option_completers
WHERE option_id IN (
    SELECT o.id
    FROM options AS o
    JOIN commands AS c ON c.id = o.command_id
    WHERE c.app_version_id = ?
)
ORDER BY option_id, value_index;

-- name: argument_completers_for_version
SELECT argument_id, value_index, completer_kind, path_kind, path_stdio, path_current_dir
FROM argument_completers
WHERE argument_id IN (
    SELECT a.id
    FROM arguments AS a
    JOIN commands AS c ON c.id = a.command_id
    WHERE c.app_version_id = ?
)
ORDER BY argument_id, value_index;

-- name: command_candidates_for_version
SELECT command_id, ordinal, value, prefix, help, candidate_id, tag, display_order, hidden
FROM command_candidates
WHERE command_id IN (SELECT id FROM commands WHERE app_version_id = ?)
ORDER BY command_id, ordinal;

-- Optional consumer queries for one command. The Rust loader currently loads
-- the complete flat version bundle and applies the same global inheritance
-- while rebuilding the clap command tree.
-- name: effective_options_for_command
WITH RECURSIVE ancestors(id, depth) AS (
    SELECT id, 0
    FROM commands
    WHERE id = ?
    UNION ALL
    SELECT c.parent_id, ancestors.depth + 1
    FROM commands AS c
    JOIN ancestors ON ancestors.id = c.id
    WHERE c.parent_id IS NOT NULL
)
SELECT o.*
FROM options AS o
JOIN ancestors ON ancestors.id = o.command_id
WHERE o.command_id = ? OR o.global_option = 1
ORDER BY CASE WHEN o.command_id = ? THEN 0 ELSE 1 END,
         ancestors.depth DESC,
         o.position,
         o.id;

-- name: effective_arguments_for_command
WITH RECURSIVE ancestors(id, depth) AS (
    SELECT id, 0
    FROM commands
    WHERE id = ?
    UNION ALL
    SELECT c.parent_id, ancestors.depth + 1
    FROM commands AS c
    JOIN ancestors ON ancestors.id = c.id
    WHERE c.parent_id IS NOT NULL
)
SELECT a.*
FROM arguments AS a
JOIN ancestors ON ancestors.id = a.command_id
WHERE a.command_id = ? OR a.global_argument = 1
ORDER BY CASE WHEN a.command_id = ? THEN 0 ELSE 1 END,
         a.position,
         a.id;
