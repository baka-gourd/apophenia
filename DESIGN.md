# Overview

The format serves as the source of truth for the Apophenia ecosystem. A dedicated **Builder** is responsible for consuming these files to perform type checking, default value injection, name expansion, and the mapping of semantic version ranges to internal database representations.

---

## 1. Project Structure and Manifests

The directory structure dictates the hierarchy of commands. The Builder uses the file layout to determine parent-child relationships.

### 1.1 Root Manifest

Located at `commands/<app>/<internal-version>/main.toml`. This file defines the entry point of an application.

```toml
schema_version = 1

[command]
name = "mklink"
binary = "mklink"
description = "Create a symbolic link, hard link, or junction"
platforms = ["windows"]
supported_versions = ["*"]
```

- `binary`: The display name for registration and completion logic (not the execution path).
- `platforms`: Metadata for filtering. The Builder handles platform-specific installation strategies; the command tree itself remains consistent for a given internal version.
- `supported_versions`: Only allowed in the root manifest.

### 1.2 Subcommand Manifest

Located at `commands/<app>/<internal-version>/commands/<subcommand>/main.toml`.

Subcommands focus strictly on local metadata. They inherit the versioning context from the root manifest. The command path (e.g., `git remote`) is automatically inferred from the directory structure.

---

## 2. Options and Arguments

Options (flags/named parameters) and Arguments (positional) are the building blocks of the CLI definition.

### 2.1 Common Attributes

- `id`: A stable, unique key within the command scope. Used as the primary key in the underlying database.
- `global`: When `true`, the option/argument is inherited by all descendant subcommands.
- `help`: A descriptive string for the UI.

### 2.2 Options

Options are defined in the `[[command.options]]` array.

- `names`: A list of triggers (e.g., `["--format", "-f"]`). The first short and first long tokens are treated as canonical names; others are treated as visible aliases.
- `action`: Defines the behavior.
  - No value: `flag`, `set_true`, `set_false`, `count`, `help`.
  - Requires value: `value`, `append`.
- `value_hint`: Provides semantic context for completion (e.g., `file_path`, `username`, `url`).

### 2.3 Positional Arguments

Defined in `[[command.arguments]]`.

- `position`: A 1-based index. Must be unique and contiguous within a command.
- `required`: Boolean indicating if the argument is mandatory.

---

## 3. Completion Logic

Apophenia provides a sophisticated completion engine that balances static suggestions with native shell capabilities.

### 3.1 Static Candidates

For simple enums, use `possible_values` and `possible_values_help`. For richer metadata, use the `candidates` table:

```toml
[[command.options.candidates]]
value = "prod"
help = "Production profile"
tag = "profiles"
display_order = 20
```

### 3.2 Contextual Value Completers

When a parameter accepts multiple values that require different suggestions based on their position (index), use `value_completers`:

```toml
[[command.options.value_completers]]
arg_index = 0
kind = "candidates"
candidates = [{ value = "origin" }, { value = "upstream" }]
```

### 3.3 Shell-Native Delegation

To ensure high-quality path completion (handling quotes, spaces, and separators), Apophenia delegates filesystem-related hints to the host shell.

If `value_hint` is set to `any_path`, `file_path`, or `dir_path`, the internal engine returns an empty candidate list, signaling the shell to trigger its native file-completion logic.

---

## 4. Versioning and Metadata

The system supports complex versioning requirements through semantic ranges and version probing.

### 4.1 Version Rules

- `Wildcard`: `supported_versions = ["*"]` covers all versions without probing.
- `Specific Ranges`: Requires `version_commands` to detect the installed version.
- `version_commands`: An array of arguments passed to the `binary` to probe version info (e.g., `[["--version"]]`).

### 4.2 Output Preprocessing

Since different tools output versions in various formats, Plain TOML supports normalization via a pipeline of preprocessors:

- `Regex Engine`: Uses Rust-style regex for extraction.
- `Minijinja Engine`: Provides logic-based text transformation (e.g., `{{ raw | trim }}`). Note: Templates are restricted to text transformation and cannot access the environment or filesystem.

---

## 5. Tooling and Validation

1. `Tombi`: Provides a language server for TOML 1.1, offering schema validation, linting, and autocompletion during editing.
2. `The Builder`: Performs deep semantic checks that generic TOML linters cannot, such as detecting duplicate IDs across the hierarchy, verifying positional argument continuity, and checking for global positional conflicts.
