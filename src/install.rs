use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::OnceLock;

#[cfg(unix)]
use std::fs;

use anyhow::{Result, anyhow, bail};
use clap::Command;
use clap::ValueEnum;
use clap_complete::env::{Bash, Elvish, EnvCompleter, Fish, Powershell, Zsh};
use minijinja::{Environment, context};
use regex::Regex;
use semver::{Version, VersionReq};
use sysinfo::{ProcessesToUpdate, System};

use crate::database::{InstallVersion, SupportRule};
use crate::{APP_SELECTOR_ENV, COMPLETION_ENV};

#[derive(Debug, Clone, Copy, ValueEnum)]
#[value(rename_all = "lower")]
pub enum Shell {
    Bash,
    Elvish,
    Fish,
    Powershell,
    Zsh,
}

#[derive(Debug, Default)]
pub struct InstallReport {
    pub matches: Vec<InstallVersion>,
    pub skipped: Vec<String>,
}

pub fn detect_shell() -> Result<Shell> {
    if let Some(value) = std::env::var_os("SHELL")
        .and_then(|value| {
            PathBuf::from(value)
                .file_stem()
                .map(|value| value.to_owned())
        })
        .and_then(|value| value.to_str().map(str::to_owned))
        && let Some(shell) = parse_shell_name(&value)
    {
        return Ok(shell);
    }
    if cfg!(windows) {
        if std::env::var_os("PSModulePath").is_some() {
            return Ok(Shell::Powershell);
        }
        return Ok(Shell::Powershell);
    }
    bail!("cannot detect shell; pass --shell (bash, elvish, fish, powershell, or zsh)")
}

fn parse_shell_name(value: &str) -> Option<Shell> {
    match value.to_ascii_lowercase().as_str() {
        "bash" => Some(Shell::Bash),
        "elvish" => Some(Shell::Elvish),
        "fish" => Some(Shell::Fish),
        "pwsh" | "powershell" | "powershell_ise" => Some(Shell::Powershell),
        "zsh" => Some(Shell::Zsh),
        _ => None,
    }
}

pub fn detect_installations(versions: &[InstallVersion]) -> Result<InstallReport> {
    let platform = current_platform();
    let mut grouped: BTreeMap<&str, Vec<&InstallVersion>> = BTreeMap::new();
    for version in versions {
        grouped
            .entry(version.application_name.as_str())
            .or_default()
            .push(version);
    }

    let mut report = InstallReport::default();
    for (application, versions) in grouped {
        let eligible = versions
            .into_iter()
            .filter(|version| platform_matches(&version.platforms, platform))
            .collect::<Vec<_>>();
        if eligible.is_empty() {
            report.skipped.push(format!(
                "{application}: no command data for platform `{platform}`"
            ));
            continue;
        }

        let mut command_cache = HashMap::new();
        let mut matches = Vec::new();
        for version in eligible {
            let availability = command_cache
                .entry(version.binary_name.clone())
                .or_insert_with(|| resolve_command(&version.binary_name))
                .clone();
            let Some(availability) = availability else {
                report.skipped.push(format!(
                    "{}:{}: command `{}` was not found",
                    version.application_name, version.internal_version, version.binary_name
                ));
                continue;
            };

            if version.rules.iter().any(|rule| rule.kind == "wildcard") {
                matches.push((version, 100_i64));
                continue;
            }
            if version.version_commands.is_empty() {
                report.skipped.push(format!(
                    "{}:{}: no version_commands",
                    version.application_name, version.internal_version
                ));
                continue;
            }
            if let ResolvedCommand::ShellBuiltin = &availability {
                report.skipped.push(format!(
                    "{}:{}: `{}` is a shell builtin and cannot be version-probed",
                    version.application_name, version.internal_version, version.binary_name
                ));
                continue;
            }
            match probe_version(version, &availability) {
                Ok(Some(detected)) => {
                    if let Some(specificity) = version
                        .rules
                        .iter()
                        .filter(|rule| version_matches(rule, &detected))
                        .map(|rule| rule.specificity)
                        .max()
                    {
                        matches.push((version, specificity));
                    }
                }
                Ok(None) => report.skipped.push(format!(
                    "{}:{}: detected version did not match supported_versions",
                    version.application_name, version.internal_version
                )),
                Err(error) => report.skipped.push(format!(
                    "{}:{}: {error}",
                    version.application_name, version.internal_version
                )),
            }
        }

        matches.sort_by(|left, right| {
            right
                .1
                .cmp(&left.1)
                .then_with(|| right.0.internal_version.cmp(&left.0.internal_version))
        });
        if let Some((version, _)) = matches.first() {
            report.matches.push((*version).clone());
        } else {
            report.skipped.push(format!(
                "{application}: no compatible version was detected on `{platform}`"
            ));
        }
    }

    if report.matches.is_empty() {
        bail!("no installed command matches the current platform `{platform}`");
    }
    report
        .matches
        .sort_by(|left, right| left.application_name.cmp(&right.application_name));
    Ok(report)
}

#[derive(Clone, Debug)]
enum ResolvedCommand {
    External(PathBuf),
    PowerShellScript(PathBuf),
    ShellBuiltin,
}

fn resolve_command(binary: &str) -> Option<ResolvedCommand> {
    let binary_path = Path::new(binary);
    if binary_path.components().count() > 1 || binary_path.is_absolute() {
        return is_executable(binary_path).then(|| resolved_command(binary_path.to_owned()));
    }

    for directory in std::env::var_os("PATH")
        .into_iter()
        .flat_map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
    {
        for candidate in executable_candidates(&directory, binary) {
            if is_executable(&candidate) {
                return Some(resolved_command(candidate));
            }
        }
    }

    if windows_builtin(binary) {
        Some(ResolvedCommand::ShellBuiltin)
    } else {
        None
    }
}

fn resolved_command(path: PathBuf) -> ResolvedCommand {
    if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("ps1"))
    {
        return ResolvedCommand::PowerShellScript(path);
    }
    ResolvedCommand::External(path)
}

fn executable_candidates(directory: &Path, binary: &str) -> Vec<PathBuf> {
    let candidate = directory.join(binary);
    let has_extension = candidate.extension().is_some();
    let mut candidates = vec![candidate];
    if has_extension {
        return candidates;
    }

    #[cfg(windows)]
    {
        let mut extensions = std::env::var_os("PATHEXT")
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".to_owned());
        if !extensions
            .split(';')
            .any(|extension| extension.trim().eq_ignore_ascii_case(".PS1"))
        {
            if !extensions.is_empty() {
                extensions.push(';');
            }
            extensions.push_str(".PS1");
        }
        candidates.extend(
            extensions
                .split(';')
                .map(str::trim)
                .filter(|extension| !extension.is_empty())
                .map(|extension| directory.join(format!("{binary}{extension}"))),
        );
    }

    if !candidates.iter().any(|candidate| {
        candidate
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ps1"))
    }) {
        candidates.push(directory.join(format!("{binary}.ps1")));
    }

    candidates
}

fn is_executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path)
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(windows)]
fn windows_builtin(binary: &str) -> bool {
    const BUILTINS: &[&str] = &[
        "assoc", "break", "call", "cd", "chdir", "cls", "color", "copy", "date", "del", "dir",
        "echo", "endlocal", "erase", "exit", "for", "ftype", "goto", "if", "md", "mkdir", "mklink",
        "move", "path", "pause", "popd", "prompt", "pushd", "rd", "ren", "rename", "rmdir", "set",
        "setlocal", "shift", "start", "time", "title", "type", "ver", "verify", "vol",
    ];
    BUILTINS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(binary))
}

#[cfg(not(windows))]
fn windows_builtin(_binary: &str) -> bool {
    false
}

fn probe_version(version: &InstallVersion, command: &ResolvedCommand) -> Result<Option<String>> {
    let mut errors = Vec::new();
    for argv in &version.version_commands {
        let output =
            process_for_command(command).and_then(|mut process| process.args(argv).output());
        let output = match output {
            Ok(output) => output,
            Err(error) => {
                errors.push(format!("{} {:?}: {error}", command_display(command), argv));
                continue;
            }
        };
        if !output.status.success() {
            errors.push(format!(
                "{} {:?} exited with {}",
                command_display(command),
                argv,
                output.status
            ));
            continue;
        }
        let raw = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let normalized = apply_preprocessors(&raw, &version.preprocessors)?;
        if version
            .rules
            .iter()
            .any(|rule| version_matches(rule, &normalized))
        {
            return Ok(Some(normalized));
        }
    }
    if errors.is_empty() {
        Ok(None)
    } else {
        Err(anyhow!(errors.join("; ")))
    }
}

fn process_for_command(command: &ResolvedCommand) -> std::io::Result<ProcessCommand> {
    match command {
        ResolvedCommand::External(path) => Ok(ProcessCommand::new(path)),
        ResolvedCommand::PowerShellScript(path) => {
            let Some(host) = powershell_host() else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "no supported PowerShell host was found",
                ));
            };
            let mut process = ProcessCommand::new(&host.path);
            process
                .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-File"])
                .arg(path);
            Ok(process)
        }
        ResolvedCommand::ShellBuiltin => {
            unreachable!("shell builtins are not passed to version probing")
        }
    }
}

fn command_display(command: &ResolvedCommand) -> String {
    match command {
        ResolvedCommand::External(path) => path.display().to_string(),
        ResolvedCommand::PowerShellScript(path) => {
            if let Some(host) = powershell_host() {
                format!(
                    "{} (PowerShell {}) -File {}",
                    host.path.display(),
                    host.version,
                    path.display()
                )
            } else {
                format!("PowerShell -File {}", path.display())
            }
        }
        ResolvedCommand::ShellBuiltin => "shell builtin".to_owned(),
    }
}

#[derive(Clone, Debug)]
struct PowerShellHost {
    path: PathBuf,
    version: String,
}

fn powershell_host() -> Option<&'static PowerShellHost> {
    static HOST: OnceLock<Option<PowerShellHost>> = OnceLock::new();
    HOST.get_or_init(discover_powershell_host).as_ref()
}

fn discover_powershell_host() -> Option<PowerShellHost> {
    powershell_candidates()
        .into_iter()
        .filter_map(|path| {
            let version = query_powershell_version(&path)?;
            powershell_version_supported(&version).then_some(PowerShellHost { path, version })
        })
        .next()
}

fn powershell_candidates() -> Vec<PathBuf> {
    let mut candidates = current_shell_powershell_candidates();
    let names = powershell_program_names();
    for directory in std::env::var_os("PATH")
        .into_iter()
        .flat_map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
    {
        for name in names {
            let path = directory.join(name);
            if path.is_file() && !candidates.iter().any(|candidate| candidate == &path) {
                candidates.push(path);
            }
        }
    }
    candidates
}

fn powershell_program_names() -> &'static [&'static str] {
    #[cfg(windows)]
    {
        &["pwsh.exe", "powershell.exe", "pwsh", "powershell"]
    }
    #[cfg(not(windows))]
    {
        &["pwsh"]
    }
}

fn current_shell_powershell_candidates() -> Vec<PathBuf> {
    let Ok(current_pid) = sysinfo::get_current_pid() else {
        return Vec::new();
    };
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);

    let Some(current_process) = system.process(current_pid) else {
        return Vec::new();
    };
    let mut next_pid = current_process.parent();
    let mut candidates = Vec::new();
    while let Some(pid) = next_pid {
        let Some(process) = system.process(pid) else {
            break;
        };
        if is_powershell_process(process)
            && let Some(path) = process.exe()
            && !candidates.iter().any(|candidate| candidate == path)
        {
            candidates.push(path.to_owned());
        }
        next_pid = process.parent();
    }
    candidates
}

fn is_powershell_process(process: &sysinfo::Process) -> bool {
    let name = process.exe().and_then(Path::file_stem).or_else(|| {
        process
            .name()
            .to_str()
            .map(Path::new)
            .and_then(Path::file_stem)
    });
    name.is_some_and(|name| {
        name.to_string_lossy().eq_ignore_ascii_case("pwsh")
            || name.to_string_lossy().eq_ignore_ascii_case("powershell")
    })
}

fn query_powershell_version(path: &Path) -> Option<String> {
    let output = ProcessCommand::new(path)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$PSVersionTable.PSVersion.ToString()",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(parse_powershell_version)
}

fn parse_powershell_version(value: &str) -> Option<String> {
    let value = value.trim();
    let components = value.split('.').collect::<Vec<_>>();
    if components.len() < 2
        || components
            .iter()
            .any(|component| component.is_empty() || component.parse::<u64>().is_err())
    {
        return None;
    }
    Some(value.to_owned())
}

fn powershell_version_supported(version: &str) -> bool {
    let Some(major) = version
        .split('.')
        .next()
        .and_then(|value| value.parse::<u64>().ok())
    else {
        return false;
    };
    if cfg!(windows) {
        major >= 5
    } else {
        major >= 6
    }
}

fn apply_preprocessors(
    raw: &str,
    preprocessors: &[crate::database::VersionPreprocessor],
) -> Result<String> {
    let mut value = raw.trim().to_owned();
    for preprocessor in preprocessors {
        match preprocessor.engine.as_str() {
            "regex" => {
                let pattern = preprocessor
                    .pattern
                    .as_deref()
                    .ok_or_else(|| anyhow!("regex preprocessor has no pattern"))?;
                let replacement = preprocessor
                    .replacement
                    .as_deref()
                    .ok_or_else(|| anyhow!("regex preprocessor has no replacement"))?;
                value = Regex::new(pattern)?
                    .replace_all(&value, replacement)
                    .into_owned();
            }
            "minijinja" => {
                let template = preprocessor
                    .template
                    .as_deref()
                    .ok_or_else(|| anyhow!("minijinja preprocessor has no template"))?;
                let environment = Environment::new();
                value = environment.render_str(template, context!(raw => value))?;
            }
            other => bail!("unknown version preprocessor `{other}`"),
        }
        value = value.trim().to_owned();
    }
    Ok(value)
}

fn version_matches(rule: &SupportRule, detected: &str) -> bool {
    let detected = detected.trim();
    match rule.kind.as_str() {
        "wildcard" => true,
        "exact" => {
            rule.expression == detected || rule.normalized_expression.as_deref() == Some(detected)
        }
        "range" => {
            let Some(normalized) = rule.normalized_expression.as_deref() else {
                return false;
            };
            let Ok(requirement) = VersionReq::parse(normalized) else {
                return false;
            };
            let Ok(version) = Version::parse(detected) else {
                return false;
            };
            requirement.matches(&version)
        }
        _ => false,
    }
}

fn current_platform() -> &'static str {
    std::env::consts::OS
}

fn platform_matches(platforms: &[String], current: &str) -> bool {
    if platforms.is_empty() {
        return true;
    }
    platforms.iter().any(|platform| {
        let platform = platform.to_ascii_lowercase();
        platform == "*"
            || platform == "all"
            || platform == current
            || (platform == "unix" && current != "windows")
            || (platform == "mac" && current == "macos")
    })
}

pub fn completion_registration(
    shell: Shell,
    selector: &str,
    command: &Command,
    executable: &Path,
) -> Result<String> {
    let mut output = Vec::new();
    let name = command.get_name();
    let bin = command.get_bin_name().unwrap_or(name);
    let completer = executable.to_string_lossy();
    let generator: &dyn EnvCompleter = match shell {
        Shell::Bash => &Bash,
        Shell::Elvish => &Elvish,
        Shell::Fish => &Fish,
        Shell::Powershell => &Powershell,
        Shell::Zsh => &Zsh,
    };
    generator.write_registration(COMPLETION_ENV, name, bin, &completer, &mut output)?;
    let registration = String::from_utf8(output)?;
    add_local_selector(shell, selector, &registration)
}

fn sh_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn add_local_selector(shell: Shell, selector: &str, registration: &str) -> Result<String> {
    let app = match shell {
        Shell::Bash | Shell::Zsh => sh_quote(selector),
        Shell::Elvish => elvish_quote(selector),
        Shell::Fish => fish_quote_for_eval(selector),
        Shell::Powershell => powershell_quote(selector),
    };
    match shell {
        Shell::Bash => inject_process_env(
            registration,
            &format!("        {COMPLETION_ENV}=\"bash\" \\\n"),
            &format!("        {COMPLETION_ENV}=\"bash\" \\\n        {APP_SELECTOR_ENV}={app} \\\n"),
            "bash",
        ),
        Shell::Zsh => inject_process_env(
            registration,
            &format!("        {COMPLETION_ENV}=\"zsh\" \\\n"),
            &format!("        {COMPLETION_ENV}=\"zsh\" \\\n        {APP_SELECTOR_ENV}={app} \\\n"),
            "zsh",
        ),
        Shell::Elvish => inject_process_env(
            registration,
            &format!("{COMPLETION_ENV}=\"elvish\" "),
            &format!("{COMPLETION_ENV}=\"elvish\" {APP_SELECTOR_ENV}={app} "),
            "elvish",
        ),
        Shell::Fish => inject_process_env(
            registration,
            &format!("{COMPLETION_ENV}=fish "),
            &format!("{COMPLETION_ENV}=fish {APP_SELECTOR_ENV}={app} "),
            "fish",
        ),
        Shell::Powershell => inject_powershell_env(registration, app),
    }
}

fn inject_process_env(
    registration: &str,
    needle: &str,
    replacement: &str,
    shell: &str,
) -> Result<String> {
    replace_once(registration, needle, replacement, shell)
}

fn inject_powershell_env(registration: &str, app: String) -> Result<String> {
    let setup_marker = format!("    $prev = $env:{COMPLETION_ENV};\n");
    let setup = format!(
        "{setup_marker}    $prevApp = $env:{APP_SELECTOR_ENV};\n    $prevConsoleInputEncoding = [Console]::InputEncoding;\n    $prevConsoleOutputEncoding = [Console]::OutputEncoding;\n    $prevOutputEncoding = $OutputEncoding;\n    [Console]::InputEncoding = [System.Text.UTF8Encoding]::new($false);\n    [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false);\n    $OutputEncoding = [System.Text.UTF8Encoding]::new($false);\n    $env:{APP_SELECTOR_ENV} = {app};\n"
    );
    let registration = replace_once(registration, &setup_marker, &setup, "powershell setup")?;
    let results_marker = "    $results | ForEach-Object";
    let restore = format!(
        "    [Console]::InputEncoding = $prevConsoleInputEncoding;\n    [Console]::OutputEncoding = $prevConsoleOutputEncoding;\n    $OutputEncoding = $prevOutputEncoding;\n    if ($null -eq $prevApp) {{\n        Remove-Item Env:\\{APP_SELECTOR_ENV};\n    }} else {{\n        $env:{APP_SELECTOR_ENV} = $prevApp;\n    }}\n{results_marker}"
    );
    replace_once(
        &registration,
        results_marker,
        &restore,
        "powershell restore",
    )
}

fn replace_once(
    registration: &str,
    needle: &str,
    replacement: &str,
    label: &str,
) -> Result<String> {
    if registration.matches(needle).count() != 1 {
        bail!("clap {label} registration template changed; cannot add local app environment");
    }
    Ok(registration.replacen(needle, replacement, 1))
}

fn elvish_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn fish_quote_for_eval(value: &str) -> String {
    if !fish_needs_quoting(value) {
        return value.to_owned();
    }
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('\'');
    for character in value.chars() {
        match character {
            '\\' => quoted.push_str(r"\\\\"),
            '\'' => quoted.push_str(r"\\'"),
            '"' => quoted.push_str(r#"\""#),
            '$' => quoted.push_str(r"\$"),
            _ => quoted.push(character),
        }
    }
    quoted.push('\'');
    quoted
}

fn fish_needs_quoting(value: &str) -> bool {
    value.is_empty()
        || value.chars().any(|character| {
            !(character.is_ascii_alphanumeric()
                || matches!(
                    character,
                    '/' | '_' | '-' | '.' | ',' | '+' | '=' | ':' | '@'
                ))
        })
}

fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::{Shell, completion_registration, detect_installations};
    use crate::database::{InstallVersion, SupportRule};

    #[test]
    fn uses_clap_powershell_registration_template() {
        let command = clap::Command::new("rustup").bin_name("rustup");
        let script = completion_registration(
            Shell::Powershell,
            "rustup:1",
            &command,
            std::path::Path::new("apophenia.exe"),
        )
        .unwrap();
        assert!(script.contains("$prevApp = $env:APOPHENIA_APP;"));
        assert!(script.contains("$env:APOPHENIA_APP = 'rustup:1';"));
        assert!(!script.starts_with("$env:APOPHENIA_APP"));
        assert!(script.contains("Register-ArgumentCompleter -Native -CommandName rustup"));
        assert!(script.contains("$env:APOPHENIA_COMPLETE = \"powershell\""));
        assert!(script.contains("$prevConsoleOutputEncoding = [Console]::OutputEncoding;"));
        assert!(
            script.contains("[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false);")
        );
        assert!(script.contains("[Console]::OutputEncoding = $prevConsoleOutputEncoding;"));
        assert!(script.contains("Remove-Item Env:\\APOPHENIA_APP;"));
    }

    #[test]
    fn scopes_selector_in_each_non_powershell_completer() {
        let command = clap::Command::new("tool").bin_name("tool");
        for shell in [Shell::Bash, Shell::Elvish, Shell::Fish, Shell::Zsh] {
            let script = completion_registration(
                shell,
                "tool:1",
                &command,
                std::path::Path::new("apophenia"),
            )
            .unwrap();
            assert!(script.contains("APOPHENIA_APP="));
            assert!(!script.starts_with("export APOPHENIA_APP"));
            assert!(!script.starts_with("set -gx APOPHENIA_APP"));
        }
    }

    #[test]
    fn recognizes_powershell_script_shims() {
        let candidates = super::executable_candidates(std::path::Path::new("tools"), "tool");
        assert!(candidates.iter().any(|candidate| {
            candidate
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("tool.ps1"))
        }));

        assert!(matches!(
            super::resolved_command(std::path::PathBuf::from("tool.PS1")),
            super::ResolvedCommand::PowerShellScript(_)
        ));
    }

    #[test]
    fn parses_both_power_shell_version_shapes() {
        assert_eq!(
            super::parse_powershell_version("7.6.5"),
            Some("7.6.5".to_owned())
        );
        assert_eq!(
            super::parse_powershell_version("5.1.26100.9223"),
            Some("5.1.26100.9223".to_owned())
        );
    }

    #[test]
    fn detects_one_current_version_for_each_available_application() {
        let executable = std::env::current_exe()
            .expect("test executable")
            .to_string_lossy()
            .into_owned();
        let versions = vec![
            test_version("alpha", "1", &executable),
            test_version("alpha", "2", &executable),
            test_version("missing", "1", "apophenia-command-that-does-not-exist"),
        ];

        let report = detect_installations(&versions).unwrap();
        assert_eq!(report.matches.len(), 1);
        assert_eq!(report.matches[0].application_name, "alpha");
        assert_eq!(report.matches[0].internal_version, "2");
        assert!(
            report
                .skipped
                .iter()
                .any(|reason| reason.contains("missing"))
        );
    }

    fn test_version(
        application: &str,
        internal_version: &str,
        binary_name: &str,
    ) -> InstallVersion {
        InstallVersion {
            id: 1,
            application_id: 1,
            application_name: application.to_owned(),
            internal_version: internal_version.to_owned(),
            binary_name: binary_name.to_owned(),
            description: None,
            long_description: None,
            platforms: vec!["*".to_owned()],
            source_path: "test".to_owned(),
            rules: vec![SupportRule {
                id: 1,
                expression: "*".to_owned(),
                kind: "wildcard".to_owned(),
                normalized_expression: None,
                specificity: 100,
            }],
            version_commands: Vec::new(),
            preprocessors: Vec::new(),
        }
    }
}
