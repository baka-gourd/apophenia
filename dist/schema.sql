PRAGMA foreign_keys = ON;
PRAGMA user_version = 1;

CREATE TABLE db_meta (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
) WITHOUT ROWID;

INSERT INTO db_meta (key, value) VALUES
    ('schema_version', '1'),
    ('format', 'apophenia-db-v1');

CREATE TABLE applications (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1))
);

CREATE TABLE app_versions (
    id INTEGER PRIMARY KEY,
    application_id INTEGER NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    internal_version TEXT NOT NULL,
    binary_name TEXT NOT NULL,
    description TEXT,
    long_description TEXT,
    platforms_json TEXT NOT NULL DEFAULT '[]',
    source_path TEXT NOT NULL,
    UNIQUE (application_id, internal_version)
);

CREATE TABLE support_rules (
    id INTEGER PRIMARY KEY,
    app_version_id INTEGER NOT NULL REFERENCES app_versions(id) ON DELETE CASCADE,
    expression TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('wildcard', 'exact', 'range')),
    normalized_expression TEXT,
    specificity INTEGER NOT NULL CHECK (specificity >= 0),
    UNIQUE (app_version_id, expression)
);

CREATE TABLE version_commands (
    app_version_id INTEGER NOT NULL REFERENCES app_versions(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    argv_json TEXT NOT NULL,
    PRIMARY KEY (app_version_id, ordinal)
) WITHOUT ROWID;

CREATE TABLE version_preprocessors (
    app_version_id INTEGER NOT NULL REFERENCES app_versions(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    engine TEXT NOT NULL CHECK (engine IN ('regex', 'minijinja')),
    pattern TEXT,
    replacement TEXT,
    template TEXT,
    PRIMARY KEY (app_version_id, ordinal),
    CHECK (
        (engine = 'regex' AND pattern IS NOT NULL AND replacement IS NOT NULL AND template IS NULL)
        OR
        (engine = 'minijinja' AND pattern IS NULL AND replacement IS NULL AND template IS NOT NULL)
    )
) WITHOUT ROWID;

CREATE TABLE commands (
    id INTEGER PRIMARY KEY,
    app_version_id INTEGER NOT NULL REFERENCES app_versions(id) ON DELETE CASCADE,
    parent_id INTEGER REFERENCES commands(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    name TEXT NOT NULL,
    about TEXT,
    long_about TEXT,
    hidden INTEGER NOT NULL DEFAULT 0 CHECK (hidden IN (0, 1)),
    position INTEGER NOT NULL DEFAULT 0 CHECK (position >= 0),
    subcommand_required INTEGER NOT NULL DEFAULT 0 CHECK (subcommand_required IN (0, 1)),
    arg_required_else_help INTEGER NOT NULL DEFAULT 0 CHECK (arg_required_else_help IN (0, 1)),
    subcommand_precedence_over_arg INTEGER NOT NULL DEFAULT 0 CHECK (subcommand_precedence_over_arg IN (0, 1)),
    infer_subcommands INTEGER NOT NULL DEFAULT 0 CHECK (infer_subcommands IN (0, 1)),
    disable_help_subcommand INTEGER NOT NULL DEFAULT 0 CHECK (disable_help_subcommand IN (0, 1)),
    allow_external_subcommands INTEGER NOT NULL DEFAULT 0 CHECK (allow_external_subcommands IN (0, 1)),
    args_conflicts_with_subcommands INTEGER NOT NULL DEFAULT 0 CHECK (args_conflicts_with_subcommands IN (0, 1)),
    subcommand_negates_reqs INTEGER NOT NULL DEFAULT 0 CHECK (subcommand_negates_reqs IN (0, 1)),
    multicall INTEGER NOT NULL DEFAULT 0 CHECK (multicall IN (0, 1)),
    no_binary_name INTEGER NOT NULL DEFAULT 0 CHECK (no_binary_name IN (0, 1)),
    disable_help_flag INTEGER NOT NULL DEFAULT 0 CHECK (disable_help_flag IN (0, 1)),
    disable_version_flag INTEGER NOT NULL DEFAULT 0 CHECK (disable_version_flag IN (0, 1)),
    UNIQUE (app_version_id, path)
);

CREATE INDEX commands_by_parent ON commands(app_version_id, parent_id, position, name);

CREATE TABLE command_names (
    command_id INTEGER NOT NULL REFERENCES commands(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    name TEXT NOT NULL,
    name_kind TEXT NOT NULL CHECK (name_kind IN ('canonical', 'alias', 'visible_alias')),
    PRIMARY KEY (command_id, name),
    UNIQUE (command_id, ordinal)
) WITHOUT ROWID;

CREATE INDEX command_names_lookup ON command_names(name, command_id);

CREATE TABLE options (
    id INTEGER PRIMARY KEY,
    command_id INTEGER NOT NULL REFERENCES commands(id) ON DELETE CASCADE,
    stable_id TEXT NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('flag', 'value', 'append', 'set_true', 'set_false', 'count', 'help', 'help_short', 'help_long')),
    help TEXT,
    long_help TEXT,
    value_name TEXT,
    value_names_json TEXT NOT NULL DEFAULT '[]',
    value_hint TEXT NOT NULL DEFAULT 'unknown',
    required INTEGER NOT NULL DEFAULT 0 CHECK (required IN (0, 1)),
    global_option INTEGER NOT NULL DEFAULT 0 CHECK (global_option IN (0, 1)),
    multiple INTEGER NOT NULL DEFAULT 0 CHECK (multiple IN (0, 1)),
    hidden INTEGER NOT NULL DEFAULT 0 CHECK (hidden IN (0, 1)),
    hide_possible_values INTEGER NOT NULL DEFAULT 0 CHECK (hide_possible_values IN (0, 1)),
    value_delimiter TEXT,
    value_terminator TEXT,
    default_value TEXT,
    default_missing_value TEXT,
    require_equals INTEGER NOT NULL DEFAULT 0 CHECK (require_equals IN (0, 1)),
    allow_hyphen_values INTEGER NOT NULL DEFAULT 0 CHECK (allow_hyphen_values IN (0, 1)),
    allow_negative_numbers INTEGER NOT NULL DEFAULT 0 CHECK (allow_negative_numbers IN (0, 1)),
    exclusive INTEGER NOT NULL DEFAULT 0 CHECK (exclusive IN (0, 1)),
    last INTEGER NOT NULL DEFAULT 0 CHECK (last IN (0, 1)),
    trailing_var_arg INTEGER NOT NULL DEFAULT 0 CHECK (trailing_var_arg IN (0, 1)),
    requires_json TEXT NOT NULL DEFAULT '[]',
    conflicts_with_json TEXT NOT NULL DEFAULT '[]',
    overrides_with_json TEXT NOT NULL DEFAULT '[]',
    min_values INTEGER CHECK (min_values IS NULL OR min_values >= 0),
    max_values INTEGER CHECK (max_values IS NULL OR max_values >= 0),
    position INTEGER NOT NULL DEFAULT 0 CHECK (position >= 0),
    UNIQUE (command_id, stable_id)
);

CREATE TABLE option_names (
    option_id INTEGER NOT NULL REFERENCES options(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    name TEXT NOT NULL,
    name_kind TEXT NOT NULL CHECK (name_kind IN ('canonical', 'alias', 'visible_alias')),
    token_kind TEXT NOT NULL CHECK (token_kind IN ('long', 'short', 'other')),
    PRIMARY KEY (option_id, name),
    UNIQUE (option_id, ordinal)
) WITHOUT ROWID;

CREATE INDEX option_names_lookup ON option_names(name, option_id);

CREATE TABLE arguments (
    id INTEGER PRIMARY KEY,
    command_id INTEGER NOT NULL REFERENCES commands(id) ON DELETE CASCADE,
    stable_id TEXT NOT NULL,
    position INTEGER NOT NULL CHECK (position > 0),
    help TEXT,
    long_help TEXT,
    value_name TEXT,
    value_names_json TEXT NOT NULL DEFAULT '[]',
    value_hint TEXT NOT NULL DEFAULT 'unknown',
    required INTEGER NOT NULL DEFAULT 0 CHECK (required IN (0, 1)),
    global_argument INTEGER NOT NULL DEFAULT 0 CHECK (global_argument IN (0, 1)),
    multiple INTEGER NOT NULL DEFAULT 0 CHECK (multiple IN (0, 1)),
    hidden INTEGER NOT NULL DEFAULT 0 CHECK (hidden IN (0, 1)),
    hide_possible_values INTEGER NOT NULL DEFAULT 0 CHECK (hide_possible_values IN (0, 1)),
    value_delimiter TEXT,
    value_terminator TEXT,
    default_value TEXT,
    default_missing_value TEXT,
    require_equals INTEGER NOT NULL DEFAULT 0 CHECK (require_equals IN (0, 1)),
    allow_hyphen_values INTEGER NOT NULL DEFAULT 0 CHECK (allow_hyphen_values IN (0, 1)),
    allow_negative_numbers INTEGER NOT NULL DEFAULT 0 CHECK (allow_negative_numbers IN (0, 1)),
    exclusive INTEGER NOT NULL DEFAULT 0 CHECK (exclusive IN (0, 1)),
    last INTEGER NOT NULL DEFAULT 0 CHECK (last IN (0, 1)),
    trailing_var_arg INTEGER NOT NULL DEFAULT 0 CHECK (trailing_var_arg IN (0, 1)),
    requires_json TEXT NOT NULL DEFAULT '[]',
    conflicts_with_json TEXT NOT NULL DEFAULT '[]',
    overrides_with_json TEXT NOT NULL DEFAULT '[]',
    min_values INTEGER CHECK (min_values IS NULL OR min_values >= 0),
    max_values INTEGER CHECK (max_values IS NULL OR max_values >= 0),
    UNIQUE (command_id, stable_id),
    UNIQUE (command_id, position)
);

CREATE TABLE option_values (
    option_id INTEGER NOT NULL REFERENCES options(id) ON DELETE CASCADE,
    value_index INTEGER NOT NULL DEFAULT -1 CHECK (value_index >= -1),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    value TEXT NOT NULL,
    prefix TEXT,
    help TEXT,
    candidate_id TEXT,
    tag TEXT,
    display_order INTEGER,
    hidden INTEGER NOT NULL DEFAULT 0 CHECK (hidden IN (0, 1)),
    value_kind TEXT NOT NULL CHECK (value_kind IN ('possible', 'candidate')),
    PRIMARY KEY (option_id, value_index, value),
    UNIQUE (option_id, value_index, ordinal)
) WITHOUT ROWID;

CREATE TABLE argument_values (
    argument_id INTEGER NOT NULL REFERENCES arguments(id) ON DELETE CASCADE,
    value_index INTEGER NOT NULL DEFAULT -1 CHECK (value_index >= -1),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    value TEXT NOT NULL,
    prefix TEXT,
    help TEXT,
    candidate_id TEXT,
    tag TEXT,
    display_order INTEGER,
    hidden INTEGER NOT NULL DEFAULT 0 CHECK (hidden IN (0, 1)),
    value_kind TEXT NOT NULL CHECK (value_kind IN ('possible', 'candidate')),
    PRIMARY KEY (argument_id, value_index, value),
    UNIQUE (argument_id, value_index, ordinal)
) WITHOUT ROWID;

CREATE TABLE option_completers (
    option_id INTEGER NOT NULL REFERENCES options(id) ON DELETE CASCADE,
    value_index INTEGER NOT NULL DEFAULT -1 CHECK (value_index >= -1),
    completer_kind TEXT NOT NULL CHECK (completer_kind IN ('candidates', 'path')),
    path_kind TEXT CHECK (path_kind IN ('any', 'file', 'dir')),
    path_stdio INTEGER NOT NULL DEFAULT 0 CHECK (path_stdio IN (0, 1)),
    path_current_dir TEXT,
    PRIMARY KEY (option_id, value_index),
    CHECK (
        (completer_kind = 'candidates' AND path_kind IS NULL AND path_stdio = 0 AND path_current_dir IS NULL)
        OR
        (completer_kind = 'path' AND path_kind IS NOT NULL)
    )
) WITHOUT ROWID;

CREATE TABLE argument_completers (
    argument_id INTEGER NOT NULL REFERENCES arguments(id) ON DELETE CASCADE,
    value_index INTEGER NOT NULL DEFAULT -1 CHECK (value_index >= -1),
    completer_kind TEXT NOT NULL CHECK (completer_kind IN ('candidates', 'path')),
    path_kind TEXT CHECK (path_kind IN ('any', 'file', 'dir')),
    path_stdio INTEGER NOT NULL DEFAULT 0 CHECK (path_stdio IN (0, 1)),
    path_current_dir TEXT,
    PRIMARY KEY (argument_id, value_index),
    CHECK (
        (completer_kind = 'candidates' AND path_kind IS NULL AND path_stdio = 0 AND path_current_dir IS NULL)
        OR
        (completer_kind = 'path' AND path_kind IS NOT NULL)
    )
) WITHOUT ROWID;

CREATE TABLE command_candidates (
    command_id INTEGER NOT NULL REFERENCES commands(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    value TEXT NOT NULL,
    prefix TEXT,
    help TEXT,
    candidate_id TEXT,
    tag TEXT,
    display_order INTEGER,
    hidden INTEGER NOT NULL DEFAULT 0 CHECK (hidden IN (0, 1)),
    PRIMARY KEY (command_id, value),
    UNIQUE (command_id, ordinal)
) WITHOUT ROWID;
