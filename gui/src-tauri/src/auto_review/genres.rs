use super::GenreSpec;

// Document-genre classifications, not reviewers. Orientation classifies the
// manuscript against this allowlist (defaulting to an ordinary research
// article, which has no entry here); the matching host-owned context
// paragraph is injected into every materialized step so the whole review
// panel judges the paper by the standards of what it claims to be.
macro_rules! genre {
    ($id:literal, $label:literal, $description:literal, $exclusions:literal) => {
        GenreSpec {
            id: $id,
            label: $label,
            routing_description: $description,
            routing_exclusions: $exclusions,
            prompt: include_str!(concat!(
                "../../../../prompts/auto_review/genres/",
                $id,
                ".md"
            )),
        }
    };
}

pub const GENRES: &[GenreSpec] = &[
    genre!(
        "genre_replication_study",
        "Replication & Reanalysis",
        "The paper's central contribution is replicating, reproducing, or reanalyzing identified prior published work.",
        "an original study that merely includes a replication arm or robustness section within a novel contribution."
    ),
    genre!(
        "genre_comment_reply",
        "Comment & Reply",
        "The paper is a comment on, correction of, or reply to one or more identified papers.",
        "ordinary disagreement with the literature inside an original contribution, or a stand-alone paper that merely revisits a topic."
    ),
    genre!(
        "genre_survey_review",
        "Survey & Review Article",
        "The paper is a narrative survey or review article whose contribution is organizing and synthesizing a literature.",
        "a formal systematic review or meta-analysis with search and pooling methodology, which the systematic-review method role audits."
    ),
    genre!(
        "genre_data_descriptor",
        "Data & Resource Descriptor",
        "The paper's contribution is a dataset, corpus, database, or comparable reusable research resource, described for reuse.",
        "a substantive study that introduces data only to answer its own research question."
    ),
    genre!(
        "genre_methods_tool",
        "Methods & Tool Paper",
        "The paper's contribution is a new method, estimator, protocol, instrument, or analytic technique offered for adoption by other researchers.",
        "routine application of existing methods, or software presented as the contribution, which the software-paper genre covers."
    ),
    genre!(
        "genre_registered_report",
        "Registered Report & Preregistration",
        "The paper is a registered report or its confirmatory claims rest on a preregistration reproduced or cited in the manuscript.",
        "a passing preregistration mention that the paper's claims do not depend on."
    ),
    genre!(
        "genre_null_results",
        "Null & Negative Results",
        "The paper's headline contribution is a null, negative, or failed-to-detect finding.",
        "a positive-result paper that also reports some insignificant estimates."
    ),
    genre!(
        "genre_clinical_case_report",
        "Case Report & Case Series",
        "The paper is a clinical or applied case report or small case series whose evidence is one documented instance or a handful.",
        "a powered clinical study, trial, or cohort analysis, which the clinical-study method role audits."
    ),
    genre!(
        "genre_software_paper",
        "Research Software Paper",
        "The paper presents research software as its contribution, with claims about the implementation's correctness, performance, or capability.",
        "a paper whose software merely implements its analysis, or a methods paper where the algorithm rather than the implementation is the contribution."
    ),
];
