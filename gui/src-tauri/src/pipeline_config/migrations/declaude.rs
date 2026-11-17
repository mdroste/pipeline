//! Upgrade the exact prior Declaude contract as a unit. Custom prompts keep
//! their old contract; cosmetic stock labels can still be renamed independently.
//! Model choices, disabled stages, and all other user settings remain intact.
use super::super::{
    declaude::declaude_profile, read_profile_file, restore_profile_bytes, ProfileData,
};
use sha2::{Digest, Sha256};
use std::path::Path;

pub(super) const MARKER: &str = ".declaude-editorial-contract-v1";

pub(super) fn upgrade(profile: &mut ProfileData) -> bool {
    let current = declaude_profile();
    let old_labels = [
        "Topic Referee 1",
        "Topic Referee 2",
        "Topic Referee 3",
        "Consolidate Prose Edits",
        "Verify Replacements & Write Report",
    ];
    let expected = HASHES;
    let exact = profile.steps.len() == current.steps.len()
        && profile
            .steps
            .iter()
            .zip(&current.steps)
            .zip(expected)
            .all(|((step, new), digest)| {
                let value =
                    serde_json::json!({"prompt":step.prompt, "output_schema":step.output_schema});
                let hash = format!(
                    "{:x}",
                    Sha256::digest(serde_json::to_vec(&value).expect("JSON value serializes"))
                );
                step.id == new.id
                    && hash == digest
                    && format!("{:x}", Sha256::digest(step.system_prompt.as_bytes())) == SYSTEM_HASH
            });
    let mut changed = false;
    for (new, old_label) in current.steps.iter().zip(old_labels) {
        if let Some(step) = profile.steps.iter_mut().find(|step| step.id == new.id) {
            if step.label == old_label {
                step.label = new.label.clone();
                changed = true;
            }
            if exact {
                step.prompt = new.prompt.clone();
                step.system_prompt = new.system_prompt.clone();
                step.output_schema = new.output_schema.clone();
                changed = true;
            }
        }
    }
    changed
}

pub(super) fn migrate(profiles: &Path) -> Result<(), String> {
    let marker = profiles.join(MARKER);
    if marker.exists() {
        return Ok(());
    }
    let path = profiles.join("declaude.json");
    if path.exists() {
        let content = read_profile_file(&path)?;
        let mut profile: ProfileData =
            serde_json::from_str(&content).map_err(|error| error.to_string())?;
        if upgrade(&mut profile)
            && super::validated_for_migration(&path, &profile, "Declaude editorial contract")
        {
            let content = serde_json::to_vec_pretty(&profile).map_err(|error| error.to_string())?;
            restore_profile_bytes(&path, &content).map_err(|error| error.to_string())?;
        }
    }
    std::fs::write(marker, b"structured-editorial-verification\n")
        .map_err(|error| error.to_string())
}

const SYSTEM_HASH: &str = "a01dd040344d81d90d246341fd95d6033654f2e9b8cdeac73e225ef129ca9555";
const HASHES: [&str; 5] = [
    "65f79aa19ff44044d91dd8bfd5ae7d3eef4c5d4f6f61bca5add64f43840d8a60",
    "dba423888bdb367016a331e208b49b0b6bc55be67551ffc0cf799ab550b8de64",
    "00abe4cd6dbec84a08125b156c86a910b06c2e1f8b319f6d1b7f481f046a456c",
    "b9debbf48a893305339967ee6f2cb5b7986eac345b15e1d7698400c823f41af1",
    "cd798141c1fa3944466ccab1605ce342492d6d23f0dc1ccbd126b81307e3be8f",
];

#[cfg(test)]
mod tests {
    use super::*;
    fn legacy() -> ProfileData {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/declaude-legacy-contract.json"
        ))
        .unwrap();
        let mut profile = declaude_profile();
        for (index, step) in profile.steps.iter_mut().enumerate() {
            step.prompt = fixture["steps"][index]["prompt"].as_str().unwrap().into();
            step.output_schema = if fixture["steps"][index]["output_schema"].is_null() {
                None
            } else {
                Some(fixture["steps"][index]["output_schema"].clone())
            };
            step.system_prompt = fixture["system_prompt"].as_str().unwrap().into();
            step.label = [
                "Topic Referee 1",
                "Topic Referee 2",
                "Topic Referee 3",
                "Consolidate Prose Edits",
                "Verify Replacements & Write Report",
            ][index]
                .into();
        }
        profile
    }

    #[test]
    fn declaude_migration_upgrades_contract_and_preserves_model_preferences() {
        let mut old = legacy();
        old.steps[0].model = "my-model".into();
        old.steps[1].label = "My specialist".into();
        assert!(upgrade(&mut old));
        assert_eq!(old.steps[0].model, "my-model");
        assert_eq!(old.steps[1].label, "My specialist");
        let current = declaude_profile();
        for (migrated, current) in old.steps.iter().zip(&current.steps) {
            assert_eq!(migrated.prompt, current.prompt);
            assert_eq!(migrated.output_schema, current.output_schema);
            assert_eq!(migrated.system_prompt, current.system_prompt);
        }
        assert!(!upgrade(&mut old));
    }

    #[test]
    fn declaude_migration_never_partially_upgrades_custom_contracts() {
        let mut old = legacy();
        old.steps[0].prompt.push_str("\nPreserve British spelling.");
        let before = old.clone();
        assert!(upgrade(&mut old)); // Only the old stock labels change.
        assert_eq!(old.steps[0].label, "Edit Document Prose");
        for (migrated, original) in old.steps.iter().zip(&before.steps) {
            assert_eq!(migrated.prompt, original.prompt);
            assert_eq!(migrated.output_schema, original.output_schema);
            assert_eq!(migrated.system_prompt, original.system_prompt);
        }
    }

    #[test]
    fn declaude_migration_is_atomic_and_idempotent_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("declaude.json");
        std::fs::write(&path, serde_json::to_vec(&legacy()).unwrap()).unwrap();
        migrate(dir.path()).unwrap();
        let upgraded = std::fs::read(&path).unwrap();
        let profile: ProfileData = serde_json::from_slice(&upgraded).unwrap();
        assert!(profile.steps[4].output_schema.is_some());
        assert!(dir.path().join(MARKER).exists());
        migrate(dir.path()).unwrap();
        assert_eq!(std::fs::read(path).unwrap(), upgraded);
    }
}
