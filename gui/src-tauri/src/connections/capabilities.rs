//! Model evidence is separate from permissions. Missing metadata stays unknown;
//! only the owning runtime can grant tool, filesystem, or network access.
use crate::model_catalog::ModelCatalogEntry;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Support {
    Supported,
    Unsupported,
    Unknown,
}

pub fn feature(model: &ModelCatalogEntry, name: &str) -> Support {
    if model.capabilities.iter().any(|value| value == name) {
        Support::Supported
    } else if model
        .capabilities
        .iter()
        .any(|value| value == &format!("no_{name}"))
    {
        Support::Unsupported
    } else {
        Support::Unknown
    }
}

pub fn effort(model: &ModelCatalogEntry, level: &str) -> Support {
    if model.supported_efforts.iter().any(|value| value == level) {
        Support::Supported
    } else if model.supported_efforts.is_empty() {
        Support::Unknown
    } else {
        Support::Unsupported
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_metadata_never_becomes_a_grant_or_a_denial() {
        let mut model = ModelCatalogEntry::default();
        assert_eq!(feature(&model, "vision"), Support::Unknown);
        assert_eq!(effort(&model, "high"), Support::Unknown);
        model.capabilities = vec!["vision".into(), "no_tools".into()];
        model.supported_efforts = vec!["high".into()];
        assert_eq!(feature(&model, "vision"), Support::Supported);
        assert_eq!(feature(&model, "tools"), Support::Unsupported);
        assert_eq!(effort(&model, "high"), Support::Supported);
        assert_eq!(effort(&model, "low"), Support::Unsupported);
    }
}
