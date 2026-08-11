use super::MethodSpec;

macro_rules! method {
    ($id:literal, $label:literal, $description:literal, $exclusions:literal, $file:literal) => {
        MethodSpec {
            id: $id,
            label: $label,
            routing_description: $description,
            routing_exclusions: $exclusions,
            prompt: include_str!($file),
        }
    };
}

pub const METHODS: &[MethodSpec] = &[
    method!(
        "formal_proofs",
        "Method — Formal Proofs",
        "Central theorems, propositions, lemmas, or nontrivial formal derivations require proof verification.",
        "routine algebra, a purely conceptual argument, or results whose proof is not material to the main claims.",
        "../../../../prompts/auto_review/methods/formal_proofs.md"
    ),
    method!(
        "economic_model_logic",
        "Method — Economic Model Logic",
        "Economic assumptions, equilibrium, incentives, mechanisms, comparative statics, incidence, or welfare carry central claims.",
        "non-economic formal models or empirical economics with no material model-based mechanism.",
        "../../../../prompts/auto_review/methods/economic_model_logic.md"
    ),
    method!(
        "causal_identification",
        "Method — Causal Identification",
        "Observational or quasi-experimental variation is used for a material causal claim.",
        "descriptive association, prediction, randomized treatment assignment, or causal language confined to motivation.",
        "../../../../prompts/auto_review/methods/causal_identification.md"
    ),
    method!(
        "randomized_experiment",
        "Method — Randomized Experiment",
        "Random assignment, encouragement, or a randomized intervention supports a central conclusion.",
        "natural experiments, simulation experiments, or laboratory work without randomized assignment of the focal intervention.",
        "../../../../prompts/auto_review/methods/randomized_experiment.md"
    ),
    method!(
        "structural_estimation",
        "Method — Structural Estimation",
        "Estimated structural parameters or model-based counterfactuals are central.",
        "reduced-form estimation, calibration without estimation, or a theoretical structural model with no fitted parameters.",
        "../../../../prompts/auto_review/methods/structural_estimation.md"
    ),
    method!(
        "quantitative_computation",
        "Method — Quantitative Computation",
        "Calibration, numerical solution, dynamic computation, inverse problems, or model counterfactuals support central results.",
        "routine data processing or computation that does not affect the substantive result.",
        "../../../../prompts/auto_review/methods/quantitative_computation.md"
    ),
    method!(
        "statistical_validity",
        "Method — Statistical Validity",
        "Statistical inference, estimation, uncertainty, prediction, or sampling claims require dedicated scrutiny.",
        "a paper with no stochastic evidence, estimated quantities, or uncertainty claims.",
        "../../../../prompts/auto_review/methods/statistical_validity.md"
    ),
    method!(
        "measurement_data",
        "Method — Measurement & Data",
        "Data construction, measurement, sampling frames, linkage, coding, or descriptive facts materially support the paper.",
        "a paper that uses only standard public measures without a material measurement or data-construction claim.",
        "../../../../prompts/auto_review/methods/measurement_data.md"
    ),
    method!(
        "simulation_numerics",
        "Method — Simulation & Numerics",
        "Monte Carlo evidence, numerical approximation, discretization, simulation accuracy, or synthetic experiments are central.",
        "numerical implementation is routine and no conclusion depends on approximation or simulation behavior.",
        "../../../../prompts/auto_review/methods/simulation_numerics.md"
    ),
    method!(
        "algorithmic_ml",
        "Method — Algorithms & Machine Learning",
        "Algorithms, prediction systems, learning procedures, benchmarks, or computational complexity are central.",
        "a standard classifier or software package is merely used as a control or preprocessing device.",
        "../../../../prompts/auto_review/methods/algorithmic_ml.md"
    ),
    method!(
        "network_analysis",
        "Method — Network Analysis",
        "Relational data, graph construction, centrality, communities, diffusion, link prediction, or network dependence materially support a central claim.",
        "a graph is only a computational data structure or visual illustration with no substantive relational inference.",
        "../../../../prompts/auto_review/methods/network_analysis.md"
    ),
    method!(
        "computational_text_analysis",
        "Method — Computational Text Analysis",
        "Dictionary, topic, embedding, classifier, large-language-model, corpus, or other text-as-data measurements support substantive claims about documents, speakers, or discourse.",
        "the contribution is an NLP algorithm itself or a close reading that does not rely on automated text measurement.",
        "../../../../prompts/auto_review/methods/computational_text_analysis.md"
    ),
    method!(
        "qualitative_case_study",
        "Method — Qualitative & Case Evidence",
        "Interviews, ethnography, process tracing, qualitative coding, participant observation, or comparative cases carry central claims.",
        "archival source criticism alone, purely textual interpretation, or cases used only as illustrations.",
        "../../../../prompts/auto_review/methods/qualitative_case_study.md"
    ),
    method!(
        "conceptual_argument",
        "Method — Conceptual Argument",
        "Conceptual, synthetic, normative, or argumentative reasoning carries the central contribution.",
        "the central claims instead turn on formal proof, measured evidence, experiments, or source interpretation.",
        "../../../../prompts/auto_review/methods/conceptual_argument.md"
    ),
    method!(
        "laboratory_experiment",
        "Method — Laboratory Experiment",
        "Controlled laboratory manipulation, physical or biological assay, bench experiment, or apparatus-based experiment supplies central evidence.",
        "randomized social interventions, purely observational measurements, or computational simulations without physical experiments.",
        "../../../../prompts/auto_review/methods/laboratory_experiment.md"
    ),
    method!(
        "observational_science",
        "Method — Observational Science & Instrumentation",
        "Instrument-derived observational data, field measurements, surveys of natural systems, detection pipelines, or observational selection functions support central claims.",
        "designed laboratory interventions or social-science causal identification where the main issue is treatment assignment.",
        "../../../../prompts/auto_review/methods/observational_science.md"
    ),
    method!(
        "clinical_study",
        "Method — Clinical Study & Diagnostic Validity",
        "Patient cohorts, diagnostic or prognostic models, clinical interventions, medical endpoints, or translational claims are central.",
        "nonclinical laboratory biology or population research with no patient-facing or diagnostic inference.",
        "../../../../prompts/auto_review/methods/clinical_study.md"
    ),
    method!(
        "survey_research",
        "Method — Survey Research",
        "Questionnaire design, respondent sampling, weighting, nonresponse, reporting behavior, or survey experiments materially determine the evidence.",
        "administrative or sensor data with no survey instrument, or surveys used only for a minor control variable.",
        "../../../../prompts/auto_review/methods/survey_research.md"
    ),
    method!(
        "design_based_research",
        "Method — Design-Based & Practice Research",
        "Iterative design, research-through-design, design-based implementation, prototyping with users, or practice-based inquiry is itself the evidentiary strategy.",
        "ordinary product engineering, a one-shot usability test, or an intervention evaluated without a material iterative-design claim.",
        "../../../../prompts/auto_review/methods/design_based_research.md"
    ),
    method!(
        "creative_practice_research",
        "Method — Creative & Practice-Led Research",
        "Artistic, performative, curatorial, compositional, literary, or other creative practice is used to generate and substantiate a central research claim.",
        "a creative work is only the object of interpretation, or an artifact is evaluated primarily as a functional design intervention.",
        "../../../../prompts/auto_review/methods/creative_practice_research.md"
    ),
    method!(
        "participatory_community_research",
        "Method — Participatory & Community Research",
        "Community-based participatory research, action research, co-production, citizen science, or stakeholder-governed inquiry materially shapes the evidence and claims.",
        "participants merely provide data or feedback without shared agenda setting, interpretation, governance, or action.",
        "../../../../prompts/auto_review/methods/participatory_community_research.md"
    ),
    method!(
        "archival_source_criticism",
        "Method — Archival & Primary-Source Criticism",
        "Archival records, manuscripts, legal or administrative documents, material archives, or primary-source provenance carry historical claims.",
        "secondary-source synthesis or interviews and ethnography without a material archival evidentiary problem.",
        "../../../../prompts/auto_review/methods/archival_source_criticism.md"
    ),
    method!(
        "historical_comparative",
        "Method — Historical & Comparative Reasoning",
        "Periodization, sequence, path dependence, comparative cases, or process-based historical explanation supports the central argument.",
        "a single contemporaneous case with no historical or comparative explanatory claim.",
        "../../../../prompts/auto_review/methods/historical_comparative.md"
    ),
    method!(
        "textual_interpretive",
        "Method — Textual & Interpretive Analysis",
        "Close reading, hermeneutics, discourse analysis, rhetoric, translation, or interpretation of cultural texts carries the contribution.",
        "automated text measurement alone or texts used only as sources of factual observations.",
        "../../../../prompts/auto_review/methods/textual_interpretive.md"
    ),
    method!(
        "legal_doctrinal",
        "Method — Doctrinal Legal Reasoning",
        "Interpretation of cases, statutes, regulations, constitutional provisions, precedent, or institutional legal authority is central.",
        "empirical legal studies whose main claims do not depend on doctrinal interpretation.",
        "../../../../prompts/auto_review/methods/legal_doctrinal.md"
    ),
    method!(
        "geospatial_remote_sensing",
        "Method — Geospatial & Remote Sensing",
        "GIS construction, spatial resolution, remote-sensing retrievals, map projections, geolocation, or spatial dependence materially support results.",
        "a paper that merely includes a map or location fixed effects without a substantive geospatial measurement problem.",
        "../../../../prompts/auto_review/methods/geospatial_remote_sensing.md"
    ),
    method!(
        "systematic_review_meta_analysis",
        "Method — Systematic Review & Meta-analysis",
        "Evidence search, study inclusion, effect harmonization, evidence grading, or quantitative synthesis across studies is central.",
        "an ordinary narrative literature review or paper citing several prior estimates without systematic synthesis.",
        "../../../../prompts/auto_review/methods/systematic_review_meta_analysis.md"
    ),
    method!(
        "bibliometric_scientometric",
        "Method — Bibliometric & Scientometric Analysis",
        "Publication, citation, authorship, collaboration, patent, or scholarly-communication records are analyzed to make central claims about research systems or knowledge structure.",
        "citations are used only for literature positioning or a systematic review synthesizes study findings rather than publication-system patterns.",
        "../../../../prompts/auto_review/methods/bibliometric_scientometric.md"
    ),
    method!(
        "mixed_methods",
        "Method — Mixed-Methods Integration",
        "The contribution depends on integrating qualitative and quantitative evidence rather than presenting them as independent appendages.",
        "a paper with multiple methods whose conclusions do not rely on their integration.",
        "../../../../prompts/auto_review/methods/mixed_methods.md"
    ),
    method!(
        "reproducibility_software",
        "Method — Reproducibility & Research Software",
        "Custom software, computational workflows, data pipelines, package behavior, or reproducible artifacts materially support the scientific result.",
        "routine use of standard software with no software, workflow, or reproducibility claim.",
        "../../../../prompts/auto_review/methods/reproducibility_software.md"
    ),
    method!(
        "engineering_validation",
        "Method — Engineering Validation & Safety",
        "Prototype testing, tolerances, reliability, standards, verification, failure modes, scale-up, or safety margins support an engineering claim.",
        "basic scientific experiments with no design-performance, reliability, or safety claim.",
        "../../../../prompts/auto_review/methods/engineering_validation.md"
    ),
    method!(
        "research_ethics_governance",
        "Method — Research Ethics & Governance",
        "Consent, participant or animal welfare, community authority, data governance, conflicts, dual-use risk, or responsible deployment materially affects the validity or permissible scope of the research claim.",
        "ethics approval is routine, adequately documented, and not material to interpreting or disseminating the central findings.",
        "../../../../prompts/auto_review/methods/research_ethics_governance.md"
    ),
];
