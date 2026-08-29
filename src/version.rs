use anyhow::{Result, bail};
use semver::{Version, VersionReq};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportKind {
    Wildcard,
    Exact,
    Range,
}

impl SupportKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Wildcard => "wildcard",
            Self::Exact => "exact",
            Self::Range => "range",
        }
    }

    pub const fn specificity(self) -> i64 {
        match self {
            Self::Wildcard => 100,
            Self::Range => 200,
            Self::Exact => 300,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifiedSupport {
    pub expression: String,
    pub normalized_expression: Option<String>,
    pub kind: SupportKind,
}

pub fn classify_support(expression: &str) -> Result<ClassifiedSupport> {
    let expression = expression.trim();
    if expression.is_empty() {
        bail!("supported_versions cannot contain an empty expression");
    }
    if expression == "*" {
        return Ok(ClassifiedSupport {
            expression: expression.to_owned(),
            normalized_expression: None,
            kind: SupportKind::Wildcard,
        });
    }

    // A complete semantic version is always an exact match. This check must
    // happen before VersionReq because VersionReq also accepts exact versions.
    if Version::parse(expression).is_ok() {
        return Ok(ClassifiedSupport {
            expression: expression.to_owned(),
            normalized_expression: Some(expression.to_owned()),
            kind: SupportKind::Exact,
        });
    }

    if looks_like_range(expression) {
        let requirement = VersionReq::parse(expression)
            .map_err(|error| anyhow::anyhow!("invalid semver range `{expression}`: {error}"))?;
        return Ok(ClassifiedSupport {
            expression: expression.to_owned(),
            normalized_expression: Some(requirement.to_string()),
            kind: SupportKind::Range,
        });
    }

    // Some target CLIs use versions such as "stable", "2024.04" or "v1".
    // Preserve those as opaque exact strings instead of forcing semver.
    Ok(ClassifiedSupport {
        expression: expression.to_owned(),
        normalized_expression: Some(expression.to_owned()),
        kind: SupportKind::Exact,
    })
}

fn looks_like_range(expression: &str) -> bool {
    expression.contains('<')
        || expression.contains('>')
        || expression.contains('^')
        || expression.contains('~')
        || expression.contains('|')
        || expression.contains(',')
        || expression.contains(" - ")
        || expression.contains('*')
        || expression
            .split(['.', '-', ' ', ',', '|'])
            .any(|component| component == "x" || component == "X")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppSelector {
    pub application: String,
    pub internal_version: String,
}

pub fn parse_app_selector(value: &str) -> Result<AppSelector> {
    let (application, internal_version) = value.split_once(':').ok_or_else(|| {
        anyhow::anyhow!("APOPHENIA_APP must have the form <application>:<internal-version>")
    })?;
    if application.is_empty() || internal_version.is_empty() || internal_version.contains(':') {
        bail!("APOPHENIA_APP must have non-empty application and internal-version");
    }
    Ok(AppSelector {
        application: application.to_owned(),
        internal_version: internal_version.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::{SupportKind, classify_support, parse_app_selector};

    #[test]
    fn classifies_build_time_support_rules() {
        assert_eq!(classify_support("*").unwrap().kind, SupportKind::Wildcard);
        assert_eq!(classify_support("1.2.3").unwrap().kind, SupportKind::Exact);
        assert_eq!(
            classify_support(">=1.0.0, <2.0.0").unwrap().kind,
            SupportKind::Range
        );
        assert_eq!(classify_support("1.x").unwrap().kind, SupportKind::Range);
        assert_eq!(classify_support("stable").unwrap().kind, SupportKind::Exact);
        assert_eq!(classify_support("box").unwrap().kind, SupportKind::Exact);
    }

    #[test]
    fn parses_the_runtime_selector() {
        let selector = parse_app_selector("mklink:1").unwrap();
        assert_eq!(selector.application, "mklink");
        assert_eq!(selector.internal_version, "1");
    }
}
