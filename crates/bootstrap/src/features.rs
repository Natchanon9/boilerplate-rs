//! Optional features, switched on with `ENABLE_FEATURES` (runtime.md AC-4).

use std::fmt;

/// An optional feature that `ENABLE_FEATURES` can switch on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    /// The Redis cache-aside decorator.
    Cache,
    /// The Kafka consumer.
    Kafka,
}

impl Feature {
    /// Every feature, in the order `info features` lists them.
    pub const ALL: [Feature; 2] = [Feature::Cache, Feature::Kafka];

    pub fn name(self) -> &'static str {
        match self {
            Feature::Cache => "cache",
            Feature::Kafka => "kafka",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|feature| feature.name() == name)
    }
}

impl fmt::Display for Feature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Which features are enabled. Every feature is off unless `ENABLE_FEATURES` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Features {
    cache: bool,
    kafka: bool,
}

impl Features {
    /// Parses a comma-separated `ENABLE_FEATURES` value. Blank items are ignored, and an
    /// unknown name is an error that lists the supported names.
    pub fn parse_enabled(value: &str) -> Result<Self, String> {
        let mut features = Self::default();
        for name in value
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            match Feature::from_name(name) {
                Some(Feature::Cache) => features.cache = true,
                Some(Feature::Kafka) => features.kafka = true,
                None => {
                    let supported: Vec<&str> = Feature::ALL.iter().map(|f| f.name()).collect();
                    return Err(format!(
                        "unknown feature `{name}`; supported: {}",
                        supported.join(", ")
                    ));
                }
            }
        }
        Ok(features)
    }

    pub fn is_enabled(self, feature: Feature) -> bool {
        match feature {
            Feature::Cache => self.cache,
            Feature::Kafka => self.kafka,
        }
    }

    /// The enabled features, formatted as an `ENABLE_FEATURES` value.
    pub fn enabled_list(self) -> String {
        Feature::ALL
            .into_iter()
            .filter(|feature| self.is_enabled(*feature))
            .map(Feature::name)
            .collect::<Vec<_>>()
            .join(",")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_value_enables_nothing() {
        let features = Features::parse_enabled("").unwrap();
        assert!(!features.is_enabled(Feature::Cache));
        assert!(!features.is_enabled(Feature::Kafka));
        assert_eq!(features, Features::default());
        assert_eq!(features.enabled_list(), "");
    }

    #[test]
    fn listed_features_are_enabled_and_blanks_are_ignored() {
        let features = Features::parse_enabled(" cache , ,kafka,").unwrap();
        assert!(features.is_enabled(Feature::Cache));
        assert!(features.is_enabled(Feature::Kafka));
        assert_eq!(features.enabled_list(), "cache,kafka");
    }

    #[test]
    fn unknown_feature_is_rejected_with_the_supported_list() {
        let err = Features::parse_enabled("cache,metrics").unwrap_err();
        assert_eq!(err, "unknown feature `metrics`; supported: cache, kafka");
    }

    #[test]
    fn names_are_case_sensitive() {
        assert!(Features::parse_enabled("Cache").is_err());
    }
}
