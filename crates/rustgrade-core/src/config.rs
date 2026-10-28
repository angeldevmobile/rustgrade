use serde::Deserialize;

use crate::finding::Severity;

/// Contents of `rustgrade.toml`. The CLI reads the file; this type only parses it.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Rule IDs to disable.
    pub allow: Vec<String>,
    /// Path prefixes treated as hot, so performance rules weigh more.
    // TODO: switch to real glob matching (globset) when `--hot <glob>` lands.
    pub hot: Vec<String>,
    pub min_severity: Severity,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            allow: Vec::new(),
            hot: Vec::new(),
            min_severity: Severity::Info,
        }
    }
}

impl Config {
    /// # Errors
    /// Returns the TOML error if `src` is not a valid `rustgrade.toml`.
    pub fn from_toml(src: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(src)
    }

    #[must_use]
    pub fn is_enabled(&self, rule_id: &str) -> bool {
        !self.allow.iter().any(|a| a == rule_id)
    }

    #[must_use]
    pub fn is_hot(&self, path: &str) -> bool {
        self.hot.iter().any(|h| path.starts_with(h.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_config() {
        let cfg = Config::from_toml(
            r#"
            allow = ["clone-in-loop"]
            hot = ["src/engine/"]
            min_severity = "medium"
            "#,
        )
        .unwrap();
        assert!(!cfg.is_enabled("clone-in-loop"));
        assert!(cfg.is_hot("src/engine/tick.rs"));
        assert_eq!(cfg.min_severity, Severity::Medium);
    }

    #[test]
    fn rejects_unknown_keys() {
        assert!(Config::from_toml("typo = 1").is_err());
    }
}
