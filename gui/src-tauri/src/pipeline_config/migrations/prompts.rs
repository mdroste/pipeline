//! Exact prompt digest transforms and compiled-default stock fingerprints.

use super::super::{full_review_profile, ProfileData};
use crate::prompts;

/// Overwrite the prompt-bearing fields that `ProfileData::new` and
/// `MergeConfig::default` fill via `prompts::load_prompt` — which honors
/// ~/.pipeline/prompts/ user overrides — with the compiled-in defaults.
/// Untouched-stock fingerprints must never depend on the user's current
/// prompt overrides: a merge.md or parallel_context.md override (a documented
/// feature) would otherwise silently prevent an untouched stock profile from
/// being recognized and upgraded. Real profile creation keeps honoring
/// overrides; only migration-time fingerprint construction is pinned.
pub(in crate::pipeline_config) fn pin_fingerprint_prompt_defaults(profile: &mut ProfileData) {
    if let Some(template) = prompts::compiled_default("parallel_context") {
        profile.parallel_context_template = template.to_string();
    }
    if let Some(merge) = prompts::compiled_default("merge") {
        profile.merge.prompt = merge.to_string();
    }
}

pub(in crate::pipeline_config) fn matches_prior_stock_full_review(
    profile: &ProfileData,
) -> Result<bool, String> {
    if profile.context_cache.enabled {
        return Ok(false);
    }
    let actual = serde_json::to_value(profile)
        .map_err(|error| format!("Failed to fingerprint Paper Review (Full): {error}"))?;
    for validate_enabled in [false, true] {
        let mut prior_stock = full_review_profile(validate_enabled);
        pin_fingerprint_prompt_defaults(&mut prior_stock);
        // full_review_profile's step prompts also come from load_prompt (via
        // prompt_step); each step id is exactly its shipped prompt name, so
        // pin those to the compiled defaults as well.
        for step in &mut prior_stock.steps {
            if let Some(default) = prompts::compiled_default(&step.id) {
                step.prompt = default.to_string();
            }
        }
        prior_stock.context_cache.enabled = false;
        let expected = serde_json::to_value(prior_stock)
            .map_err(|error| format!("Failed to fingerprint stock Full profile: {error}"))?;
        if actual == expected {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(in crate::pipeline_config) fn prompt_digest(prompt: &str) -> String {
    use sha2::{Digest as _, Sha256};
    hex::encode(Sha256::digest(prompt.as_bytes()))
}

pub(in crate::pipeline_config) fn migrate_shipped_prompt_defaults(
    profile: &mut ProfileData,
) -> bool {
    const OLD_EDITOR: &str = "2393f4d8f6f39641b5a5e609e132513abb3850b4995eb0522a3ae077fdef29b0";
    const OLD_EDITOR_ISSUES: &str =
        "01079fa2f1a5c0c44ba88861966b8383bbb5a4ac3b47b0e95ac3a8256bb36eeb";
    const OLD_MERGE: &str = "cd90280a68b6818ba075f293f3ec54629568dd2d3e13dc891bcfb3164cdb9b18";
    const ACADEMIC_MERGE: &str = "320a872359321270bd37632bd8c08720d558bdd81c0f8da29ec9fb5f68a09aea";
    let mut changed = false;
    for step in &mut profile.steps {
        let replacement = match prompt_digest(&step.prompt).as_str() {
            OLD_EDITOR => prompts::compiled_default("editor_synthesis"),
            OLD_EDITOR_ISSUES => prompts::compiled_default("editor_synthesis_issues"),
            _ => None,
        };
        if let Some(replacement) = replacement {
            step.prompt = replacement.to_string();
            changed = true;
        }
    }
    if matches!(
        prompt_digest(&profile.merge.prompt).as_str(),
        OLD_MERGE | ACADEMIC_MERGE
    ) {
        if let Some(replacement) = prompts::compiled_default("merge") {
            profile.merge.prompt = replacement.to_string();
            changed = true;
        }
    }
    changed
}

pub(in crate::pipeline_config) fn migrate_efficient_retrieval_defaults(
    profile: &mut ProfileData,
) -> bool {
    const OLD_PARALLEL_CONTEXT: &str =
        "2d3f253aba5a39e76a560cda448296647b42cd3b4d487bbb761af9e7172fbe65";
    const OLD_GENERIC_PARALLEL_CONTEXT: &str =
        "d2bc29faeb032cc35cb60f2aff561f17a737c18348a76140205777f8b3c33197";
    const OLD_VALIDATE_FEEDBACK: &str =
        "ad7af438fed68f08e4f2e85de9155314f9579d6d96a4c960fb4b53b5ff0e7555";

    let mut changed = false;
    let parallel_replacement = match prompt_digest(&profile.parallel_context_template).as_str() {
        OLD_PARALLEL_CONTEXT => prompts::compiled_default("parallel_context"),
        OLD_GENERIC_PARALLEL_CONTEXT => prompts::compiled_default("parallel_context_generic"),
        _ => None,
    };
    if let Some(replacement) = parallel_replacement {
        profile.parallel_context_template = replacement.to_string();
        changed = true;
    }

    for step in &mut profile.steps {
        if prompt_digest(&step.prompt) == OLD_VALIDATE_FEEDBACK {
            if let Some(replacement) = prompts::compiled_default("validate_feedback") {
                step.prompt = replacement.to_string();
                changed = true;
            }
        }
    }
    changed
}

pub(in crate::pipeline_config) fn migrate_review_quality_prompt_defaults(
    profile: &mut ProfileData,
) -> bool {
    const OLD_PARALLEL_CONTEXT: &str =
        "b5343b777ec44f21af434dbea2daff76e6ef352ac58cf2a928b16e204b04b709";
    const OLD_CONTRIBUTION: &str =
        "733b35d1e2137ea876ca4ef504532ae2d5875291f943584ce1eb4e42ad937e03";
    const OLD_TECHNICAL: &str = "e7574cc654ce76e21ea254d517e7cdd1e9991fd691249226b6c6a5745664e03e";
    const OLD_EMPIRICAL: &str = "b8823245b925a15775cde437ed7ad31fe9fc3766d61c9e2f56677a9b14b2998e";
    const OLD_EXPOSITION: &str = "15f0b9340bd28b4d4e0f3fc5d1b59cd01845c62854eb856eb24e9b052edd5914";
    const OLD_EDITOR_SYNTHESIS: &str =
        "5fd0eed34fddbc6ab32d9330b097b72895e58b91be9c013f42bf5636495ed5d7";
    const OLD_VALIDATE_FEEDBACK: &str =
        "fa49fa759b553c171ed721d2cc8696b1f083a891e237be01ace245188761a744";

    let mut changed = false;
    if prompt_digest(&profile.parallel_context_template) == OLD_PARALLEL_CONTEXT {
        if let Some(replacement) = prompts::compiled_default("parallel_context") {
            profile.parallel_context_template = replacement.to_string();
            changed = true;
        }
    }

    for step in &mut profile.steps {
        let replacement = match prompt_digest(&step.prompt).as_str() {
            OLD_CONTRIBUTION => prompts::compiled_default("contribution"),
            OLD_TECHNICAL => prompts::compiled_default("technical"),
            OLD_EMPIRICAL => prompts::compiled_default("empirical"),
            OLD_EXPOSITION => prompts::compiled_default("exposition"),
            OLD_EDITOR_SYNTHESIS => prompts::compiled_default("editor_synthesis"),
            OLD_VALIDATE_FEEDBACK => prompts::compiled_default("validate_feedback"),
            _ => None,
        };
        if let Some(replacement) = replacement {
            step.prompt = replacement.to_string();
            changed = true;
        }
    }
    changed
}
