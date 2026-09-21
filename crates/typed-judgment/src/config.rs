use crate::envelope::ProviderStatus;
use crate::policy::{JudgmentPolicy, POLICY_VERSION};
use crate::provider::{
    DeterministicMockJudgmentProvider, DisabledJudgmentProvider, JudgmentProvider,
    StaticStatusProvider,
};
use crate::typesafe::{HttpTypeSafeTransport, TypeSafeJudgmentProvider};
use std::sync::Arc;
use std::time::Duration;

pub const DEFAULT_MODEL: &str = "jev-latest";
pub const DEFAULT_BASE_URL: &str = "https://api.typesafe.ai";
pub const API_KEY_ENV: &str = "TYPESAFE_API_KEY";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderSelection {
    Disabled,
    Mock,
    TypeSafe,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JudgmentSettings {
    pub enabled: bool,
    pub provider: ProviderSelection,
    pub model: String,
    pub timeout: Duration,
    pub policy: JudgmentPolicy,
    pub base_url: String,
}

impl Default for JudgmentSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: ProviderSelection::Disabled,
            model: DEFAULT_MODEL.to_string(),
            timeout: Duration::from_secs(10),
            policy: JudgmentPolicy::default(),
            base_url: DEFAULT_BASE_URL.to_string(),
        }
    }
}

impl JudgmentSettings {
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub fn from_lookup(mut lookup: impl FnMut(&str) -> Option<String>) -> Self {
        let enabled = parse_bool(lookup("JUDGMENT_ENABLED").as_deref());
        let provider = if enabled {
            parse_provider(lookup("JUDGMENT_PROVIDER").as_deref())
        } else {
            ProviderSelection::Disabled
        };
        let model = lookup("JUDGMENT_MODEL")
            .or_else(|| lookup("TYPESAFE_DEFAULT_MODEL"))
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_MODEL.to_string());
        let timeout = lookup("JUDGMENT_TIMEOUT_MS")
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .map(Duration::from_millis)
            .unwrap_or_else(|| Duration::from_secs(10));
        let timeout = timeout.min(Duration::from_secs(120));
        let mut policy = JudgmentPolicy::default();
        if let Some(confidence) = lookup("JUDGMENT_MINIMUM_CONFIDENCE")
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| (0.0..=1.0).contains(value))
        {
            policy.minimum_confidence = confidence;
        }
        policy.version = lookup("JUDGMENT_POLICY_VERSION")
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| POLICY_VERSION.to_string());
        let base_url = lookup("TYPESAFE_BASE_URL")
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        Self {
            enabled,
            provider,
            model,
            timeout,
            policy,
            base_url,
        }
    }

    pub fn provider_name(&self) -> &'static str {
        match self.provider {
            ProviderSelection::Disabled => "disabled",
            ProviderSelection::Mock => "mock",
            ProviderSelection::TypeSafe => "typesafe",
            ProviderSelection::Unknown => "unknown",
        }
    }
}

pub fn startup_log_line(settings: &JudgmentSettings) -> String {
    format!(
        "typed judgment enabled={} provider={} model={} policy={} timeout_ms={}",
        settings.enabled,
        settings.provider_name(),
        settings.model,
        settings.policy.version,
        settings.timeout.as_millis()
    )
}

pub fn build_provider(settings: &JudgmentSettings) -> Arc<dyn JudgmentProvider> {
    if !settings.enabled {
        return Arc::new(DisabledJudgmentProvider);
    }
    match settings.provider {
        ProviderSelection::Disabled => Arc::new(DisabledJudgmentProvider),
        ProviderSelection::Mock => Arc::new(DeterministicMockJudgmentProvider::supported()),
        ProviderSelection::Unknown => {
            Arc::new(StaticStatusProvider::new(ProviderStatus::InvalidRequest))
        }
        ProviderSelection::TypeSafe => match read_api_key() {
            Some(api_key) => {
                match HttpTypeSafeTransport::new(&settings.base_url, api_key, settings.timeout) {
                    Ok(transport) => Arc::new(TypeSafeJudgmentProvider::new(
                        settings.model.clone(),
                        transport,
                    )),
                    Err(error) => {
                        tracing::warn!(error = %error, "typed judgment transport was not configured");
                        Arc::new(StaticStatusProvider::new(
                            ProviderStatus::MissingCredentials,
                        ))
                    }
                }
            }
            None => {
                tracing::warn!(
                    "typed judgment enabled without TYPESAFE_API_KEY; withholding remote calls"
                );
                Arc::new(StaticStatusProvider::new(
                    ProviderStatus::MissingCredentials,
                ))
            }
        },
    }
}

fn read_api_key() -> Option<String> {
    let key = std::env::var(API_KEY_ENV).ok()?;
    let trimmed = key.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_bool(value: Option<&str>) -> bool {
    matches!(
        value.map(str::trim).map(str::to_ascii_lowercase).as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

fn parse_provider(value: Option<&str>) -> ProviderSelection {
    match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
        Some("typesafe") => ProviderSelection::TypeSafe,
        Some("mock" | "deterministic_mock") => ProviderSelection::Mock,
        Some("disabled") | None => ProviderSelection::Disabled,
        Some(_) => ProviderSelection::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_judgment_defaults_off() {
        let settings = JudgmentSettings::from_lookup(|_| None);
        assert!(!settings.enabled);
        assert_eq!(settings.provider, ProviderSelection::Disabled);
        assert_eq!(settings.model, "jev-latest");
        assert!(startup_log_line(&settings).contains("enabled=false"));
    }

    #[test]
    fn explicit_opt_in_selects_typesafe_without_storing_a_key() {
        let settings = JudgmentSettings::from_lookup(|key| match key {
            "JUDGMENT_ENABLED" => Some("true".into()),
            "JUDGMENT_PROVIDER" => Some("typesafe".into()),
            "JUDGMENT_MODEL" => Some("jev-latest".into()),
            "JUDGMENT_TIMEOUT_MS" => Some("4000".into()),
            "JUDGMENT_MINIMUM_CONFIDENCE" => Some("0.8".into()),
            "JUDGMENT_POLICY_VERSION" => Some("pordenone.judgment.policy.v1".into()),
            _ => None,
        });
        assert!(settings.enabled);
        assert_eq!(settings.provider, ProviderSelection::TypeSafe);
        assert_eq!(settings.timeout, Duration::from_millis(4000));
        assert!((settings.policy.minimum_confidence - 0.8).abs() < f64::EPSILON);
        let rendered = format!("{settings:?}");
        assert!(!rendered.to_ascii_lowercase().contains("api_key"));
        assert!(!startup_log_line(&settings).contains("TYPESAFE_API_KEY"));
    }

    #[test]
    fn unknown_provider_fails_closed() {
        let settings = JudgmentSettings::from_lookup(|key| match key {
            "JUDGMENT_ENABLED" => Some("yes".into()),
            "JUDGMENT_PROVIDER" => Some("surprise-model".into()),
            _ => None,
        });
        assert_eq!(settings.provider, ProviderSelection::Unknown);
    }
}
