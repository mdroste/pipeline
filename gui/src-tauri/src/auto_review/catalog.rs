//! Generated allowlisted roles and their read-only catalog identity.
use super::assets::{METHOD_REVIEW_BASE, SUBJECT_REVIEW_BASE};
use super::AUTO_REVIEW_CONTRACT;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubjectLevel {
    Discipline,
    Subfield,
}

#[derive(Debug, Clone, Copy)]
pub struct SubjectSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub discipline_id: &'static str,
    pub discipline_label: &'static str,
    pub level: SubjectLevel,
    pub routing_description: &'static str,
    pub routing_exclusions: &'static str,
    pub discipline_prompt: &'static str,
    pub review_focus: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodLevel {
    /// The family's broad fallback, selected only when no specific sibling
    /// fits or a genuinely separate general issue is central.
    Family,
    Specific,
}

#[derive(Debug, Clone, Copy)]
pub struct MethodSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub family_id: &'static str,
    pub family_label: &'static str,
    pub level: MethodLevel,
    pub routing_description: &'static str,
    pub routing_exclusions: &'static str,
    pub prompt: &'static str,
}

/// A document-genre classification, not a reviewer. Orientation classifies
/// the manuscript against this allowlist; the matching host-owned context
/// paragraph (`prompt`) is then injected into every materialized step so all
/// reviewers judge the paper by the standards of what it claims to be.
#[derive(Debug, Clone, Copy)]
pub struct GenreSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub routing_description: &'static str,
    pub routing_exclusions: &'static str,
    pub prompt: &'static str,
}

/// The `review_plan.genre` value for an ordinary research article: the
/// default classification, carrying no injected genre context.
pub const RESEARCH_ARTICLE_GENRE: &str = "research_article";

/// The catalog genre Slide Review pins in its orientation schema.
pub const SLIDE_DECK_GENRE: &str = "genre_slide_deck";

include!(concat!(env!("OUT_DIR"), "/auto_review_catalog.rs"));

pub fn catalog_revision() -> &'static str {
    use sha2::{Digest as _, Sha256};
    static REVISION: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        let mut digest = Sha256::new();
        digest.update(b"pipeline auto-review catalog v1\0");
        for shared_contract in [SUBJECT_REVIEW_BASE, METHOD_REVIEW_BASE] {
            digest.update(shared_contract.as_bytes());
            digest.update(b"\0");
        }
        for subject in SUBJECTS {
            for value in [
                subject.id,
                subject.label,
                subject.discipline_id,
                subject.discipline_label,
                subject.routing_description,
                subject.routing_exclusions,
                subject.discipline_prompt,
                subject.review_focus,
            ] {
                digest.update(value.as_bytes());
                digest.update(b"\0");
            }
        }
        for method in METHODS {
            for value in [
                method.id,
                method.label,
                method.family_id,
                method.family_label,
                method.routing_description,
                method.routing_exclusions,
                method.prompt,
            ] {
                digest.update(value.as_bytes());
                digest.update(b"\0");
            }
        }
        for genre in GENRES {
            for value in [
                genre.id,
                genre.label,
                genre.routing_description,
                genre.routing_exclusions,
                genre.prompt,
            ] {
                digest.update(value.as_bytes());
                digest.update(b"\0");
            }
        }
        format!("sha256:{}", hex::encode(digest.finalize()))
    });
    REVISION.as_str()
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogRole {
    pub id: &'static str,
    pub label: &'static str,
    pub level: &'static str,
    pub description: &'static str,
    pub exclusions: &'static str,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogGroup {
    pub id: &'static str,
    pub label: &'static str,
    pub roles: Vec<CatalogRole>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoReviewCatalog {
    pub contract: &'static str,
    pub revision: &'static str,
    pub subject_count: usize,
    pub method_count: usize,
    pub genre_count: usize,
    pub disciplines: Vec<CatalogGroup>,
    pub method_families: Vec<CatalogGroup>,
}

/// Read-only metadata for explaining the router in the UI. This is generated
/// from the same static entries used by the classifier and materializer.
pub fn catalog() -> AutoReviewCatalog {
    let mut disciplines = Vec::<CatalogGroup>::new();
    for subject in SUBJECTS {
        if disciplines
            .last()
            .is_none_or(|discipline| discipline.id != subject.discipline_id)
        {
            disciplines.push(CatalogGroup {
                id: subject.discipline_id,
                label: subject.discipline_label,
                roles: Vec::new(),
            });
        }
        disciplines.last_mut().unwrap().roles.push(CatalogRole {
            id: subject.id,
            label: subject.label,
            level: match subject.level {
                SubjectLevel::Discipline => "discipline",
                SubjectLevel::Subfield => "subfield",
            },
            description: subject.routing_description,
            exclusions: subject.routing_exclusions,
        });
    }
    let mut method_families = Vec::<CatalogGroup>::new();
    for method in METHODS {
        if method_families
            .last()
            .is_none_or(|family| family.id != method.family_id)
        {
            method_families.push(CatalogGroup {
                id: method.family_id,
                label: method.family_label,
                roles: Vec::new(),
            });
        }
        method_families.last_mut().unwrap().roles.push(CatalogRole {
            id: method.id,
            label: method.label,
            level: match method.level {
                MethodLevel::Family => "family",
                MethodLevel::Specific => "method",
            },
            description: method.routing_description,
            exclusions: method.routing_exclusions,
        });
    }
    AutoReviewCatalog {
        contract: AUTO_REVIEW_CONTRACT,
        revision: catalog_revision(),
        subject_count: SUBJECTS.len(),
        method_count: METHODS.len(),
        genre_count: GENRES.len() + 1,
        disciplines,
        method_families,
    }
}

pub fn specialist_count() -> usize {
    SUBJECTS.len() + METHODS.len()
}

pub fn label_for(id: &str) -> Option<&'static str> {
    SUBJECTS
        .iter()
        .find(|specialist| specialist.id == id)
        .map(|specialist| specialist.label)
        .or_else(|| {
            METHODS
                .iter()
                .find(|specialist| specialist.id == id)
                .map(|specialist| specialist.label)
        })
}
