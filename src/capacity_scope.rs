use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapacityScopeKind {
    Repository,
    ProviderAccount,
}

impl std::fmt::Display for CapacityScopeKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Repository => write!(formatter, "repository"),
            Self::ProviderAccount => write!(formatter, "provider_account"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapacityScope {
    pub kind: CapacityScopeKind,
    pub key: String,
}

impl CapacityScope {
    pub fn repository(key: impl Into<String>) -> Self {
        Self {
            kind: CapacityScopeKind::Repository,
            key: key.into(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(!self.key.trim().is_empty(), "capacity scope key is required");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_requires_nonempty_key() {
        assert!(CapacityScope::repository("owner/repo").validate().is_ok());
        assert!(CapacityScope::repository(" ").validate().is_err());
    }
}
