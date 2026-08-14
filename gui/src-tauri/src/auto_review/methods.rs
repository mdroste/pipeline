use super::{MethodLevel, MethodSpec};

// Families group related method roles for routing and the catalog browser.
// Entries must stay contiguous by family; a `Family`-level role is that
// family's broad fallback and is chosen only when no specific sibling fits.
macro_rules! method {
    ($id:literal, $label:literal, $family:ident, $level:ident,
     $description:literal, $exclusions:literal) => {
        MethodSpec {
            id: $id,
            label: $label,
            family_id: $family.0,
            family_label: $family.1,
            level: MethodLevel::$level,
            routing_description: $description,
            routing_exclusions: $exclusions,
            prompt: include_str!(concat!(
                "../../../../prompts/auto_review/methods/",
                $id,
                ".md"
            )),
        }
    };
}

const FORMAL_CONCEPTUAL: (&str, &str) =
    ("formal_conceptual", "Formal Theory & Conceptual Analysis");
const CAUSAL_INFERENCE: (&str, &str) = ("causal_inference", "Causal Inference & Policy Evaluation");
const EXPERIMENTS: (&str, &str) = ("experiments", "Experiments & Trials");
const STATISTICAL_INFERENCE: (&str, &str) =
    ("statistical_inference", "Statistical Modeling & Inference");
const MEASUREMENT_RECORDS: (&str, &str) = ("measurement_records", "Measurement, Data & Records");
const COMPUTATIONAL_MODELING: (&str, &str) = (
    "computational_modeling",
    "Computation, Simulation & Modeling",
);
const ML_AI: (&str, &str) = ("ml_ai", "Machine Learning & AI");
const NETWORKS_TEXT_TRACES: (&str, &str) =
    ("networks_text_traces", "Networks, Text & Digital Traces");
const HEALTH_CLINICAL: (&str, &str) = ("health_clinical", "Health & Clinical Research");
const LAB_INSTRUMENTATION: (&str, &str) = ("lab_instrumentation", "Laboratory & Instrumentation");
const QUALITATIVE_INTERPRETIVE: (&str, &str) =
    ("qualitative_interpretive", "Qualitative & Interpretive");
const SOURCES_HISTORY_LAW: (&str, &str) = ("sources_history_law", "Sources, History & Law");
const SYNTHESIS_META: (&str, &str) = ("synthesis_meta", "Synthesis, Evaluation & Meta-research");
const ENGINEERING_DESIGN: (&str, &str) = (
    "engineering_design",
    "Engineering, Design & Applied Evaluation",
);
const INTEGRITY_GOVERNANCE: (&str, &str) = (
    "integrity_governance",
    "Reproducibility, Integrity & Governance",
);

pub const METHODS: &[MethodSpec] = &[
    // Formal Theory & Conceptual Analysis
    method!(
        "formal_proofs",
        "Method — Formal Proofs",
        FORMAL_CONCEPTUAL, Specific,
        "Central theorems, propositions, lemmas, or nontrivial formal derivations require proof verification.",
        "routine algebra, a purely conceptual argument, or results whose proof is not material to the main claims."
    ),
    method!(
        "economic_model_logic",
        "Method — Economic Model Logic",
        FORMAL_CONCEPTUAL, Specific,
        "Economic assumptions, equilibrium, incentives, mechanisms, comparative statics, incidence, or welfare carry central claims.",
        "non-economic formal models or empirical economics with no material model-based mechanism."
    ),
    method!(
        "conceptual_argument",
        "Method — Conceptual Argument",
        FORMAL_CONCEPTUAL, Specific,
        "Conceptual, synthetic, normative, or argumentative reasoning carries the central contribution.",
        "the central claims instead turn on formal proof, measured evidence, experiments, or source interpretation."
    ),
    // Causal Inference & Policy Evaluation
    method!(
        "causal_identification",
        "Method — Causal Identification",
        CAUSAL_INFERENCE, Family,
        "Observational or quasi-experimental variation is used for a material causal claim that spans designs or does not fit a listed design-specific role.",
        "descriptive association, prediction, randomized assignment, causal language confined to motivation, or a single listed design-specific role that fully covers the identifying strategy."
    ),
    method!(
        "regression_discontinuity",
        "Method — Regression Discontinuity",
        CAUSAL_INFERENCE, Specific,
        "A cutoff, threshold rule, score, or geographic boundary supplies local identifying variation for a central causal claim.",
        "a discontinuity used only descriptively, an interrupted time series without cross-sectional score continuity, or an arbitrary threshold selected after seeing outcomes."
    ),
    method!(
        "difference_in_differences",
        "Method — Difference-in-Differences & Event Studies",
        CAUSAL_INFERENCE, Specific,
        "Group-time treatment adoption, policy timing, or event-study contrasts identify a material causal effect through untreated potential-outcome trends.",
        "a purely descriptive before-after comparison, a single aggregate time-series break, or an event study used only to display dynamics without a causal claim."
    ),
    method!(
        "instrumental_variables",
        "Method — Instrumental Variables",
        CAUSAL_INFERENCE, Specific,
        "An instrument, encouragement, judge or examiner assignment, shift-share exposure, or other excluded variation identifies a central causal parameter.",
        "the variable is merely a control, a randomized treatment is analyzed directly, or the paper makes no exclusion-based causal interpretation."
    ),
    method!(
        "matching_weighting",
        "Method — Matching, Weighting & Selection on Observables",
        CAUSAL_INFERENCE, Specific,
        "Matching, propensity scores, inverse-probability weights, balancing weights, standardization, or doubly robust adjustment supports a central causal comparison under selection on observables.",
        "ordinary regression adjustment without a material design or weighting claim, randomized assignment, or a design identified by discontinuity, timing, or an instrument."
    ),
    method!(
        "synthetic_control",
        "Method — Synthetic Control & Comparative Time Series",
        CAUSAL_INFERENCE, Specific,
        "A weighted donor pool, matrix-completion counterfactual, comparative interrupted time series, or related synthetic-control design identifies a central intervention effect.",
        "a generic forecasting exercise, a standard difference-in-differences design without synthetic comparison construction, or a descriptive index assembled from comparison units."
    ),
    method!(
        "mediation_mechanism",
        "Method — Mediation & Mechanism Analysis",
        CAUSAL_INFERENCE, Specific,
        "Direct and indirect effects, causal mediation, decomposition, mechanism experiments, or intermediate outcomes are used to substantiate how an effect operates.",
        "an intermediate outcome is merely reported, a structural model defines a channel without a mediation estimand, or the paper makes no claim that the mediator carries the effect."
    ),
    method!(
        "bunching_estimators",
        "Method — Bunching & Notch Designs",
        CAUSAL_INFERENCE, Specific,
        "Bunching at a kink, notch, or threshold in a schedule identifies an elasticity, behavioral response, or structural parameter central to the paper.",
        "a discontinuity design comparing outcomes across a cutoff, or descriptive heaping without a recovered parameter."
    ),
    method!(
        "shift_share_designs",
        "Method — Shift-Share & Exposure Designs",
        CAUSAL_INFERENCE, Specific,
        "A shift-share or Bartik-style interaction of aggregate shocks with local exposure shares identifies a central causal effect.",
        "an ordinary excluded instrument without exposure-share structure, or exposure used only as a descriptive covariate."
    ),
    method!(
        "dynamic_treatment_g_methods",
        "Method — Time-Varying Treatments & G-Methods",
        CAUSAL_INFERENCE, Specific,
        "Sustained or time-varying treatment strategies with treatment-confounder feedback are analyzed via marginal structural models, the g-formula, or emulated target trials.",
        "point-in-time treatments handled by a standard design, or a randomized trial analyzed by intention to treat."
    ),
    method!(
        "heterogeneous_effects_ml",
        "Method — ML-Based Heterogeneous Effects",
        CAUSAL_INFERENCE, Specific,
        "Machine-learning estimation of heterogeneous treatment effects, causal forests, debiased ML, or learned targeting policies carries a central claim.",
        "a prespecified subgroup interaction in a conventional model, or predictive ML without a causal-effect claim."
    ),
    method!(
        "mendelian_randomization",
        "Method — Mendelian Randomization",
        CAUSAL_INFERENCE, Specific,
        "Genetic variants serve as instruments for a central exposure-outcome causal claim.",
        "GWAS discovery or polygenic prediction without an instrumented causal contrast, or a non-genetic instrument."
    ),
    method!(
        "finance_event_study",
        "Method — Financial Event Studies",
        CAUSAL_INFERENCE, Specific,
        "Abnormal security-price or market reactions around events, measured against an expected-return benchmark, carry a central claim.",
        "a difference-in-differences event study of non-price outcomes, or price data used only descriptively."
    ),
    method!(
        "external_validity_transport",
        "Method — External Validity & Transportability",
        CAUSAL_INFERENCE, Specific,
        "Carrying an estimate to another population, scale, or setting — scaling pilots, transporting trial results, or generalizing local estimates — is itself central to the contribution.",
        "a routine external-validity caveat, or a paper whose claims stay within the studied population."
    ),
    method!(
        "interference_spillovers",
        "Method — Interference & Spillovers",
        CAUSAL_INFERENCE, Specific,
        "Spillovers, network interference, displacement, or general-equilibrium contamination materially shape the causal estimand or its interpretation.",
        "interference is implausible at the design's scale, or spillovers are mentioned only as a limitation."
    ),
    method!(
        "sufficient_statistics_welfare",
        "Method — Sufficient Statistics & Welfare Analysis",
        CAUSAL_INFERENCE, Specific,
        "Welfare, deadweight-loss, or optimal-policy conclusions are computed by plugging reduced-form elasticities into envelope-based theoretical formulas.",
        "structural-model counterfactuals, or theoretical welfare analysis with no empirical inputs."
    ),
    // Experiments & Trials
    method!(
        "randomized_experiment",
        "Method — Randomized Experiment",
        EXPERIMENTS, Specific,
        "Random assignment, encouragement, or a randomized intervention supports a central conclusion.",
        "natural experiments, simulation experiments, or laboratory work without randomized assignment of the focal intervention."
    ),
    method!(
        "field_experiment",
        "Method — Field Experiment",
        EXPERIMENTS, Specific,
        "A manipulated intervention in a natural, organizational, ecological, educational, or market setting supplies central evidence.",
        "a laboratory experiment, a randomized policy evaluation already fully covered by randomized-experiment review, or passive field observation without researcher manipulation."
    ),
    method!(
        "laboratory_experiment",
        "Method — Laboratory Experiment",
        EXPERIMENTS, Specific,
        "Controlled laboratory manipulation, bench experiment, or apparatus-based experiment supplies central evidence not exhausted by a listed assay or characterization role.",
        "randomized social interventions, purely observational measurements, computational simulations, or evidence fully covered by imaging, omics, sensor, or materials-characterization review."
    ),
    method!(
        "survey_experiments_conjoint",
        "Method — Survey Experiments & Conjoint Designs",
        EXPERIMENTS, Specific,
        "Vignette, framing, list, endorsement, or conjoint experiments embedded in surveys support a central causal claim about preferences or attitudes.",
        "a field or lab experiment with behavioral outcomes, or a descriptive survey without randomized content."
    ),
    method!(
        "audit_correspondence_studies",
        "Method — Audit & Correspondence Studies",
        EXPERIMENTS, Specific,
        "Matched fictitious applications, testers, or solicitations measure discrimination or differential treatment as a central claim.",
        "observational disparity estimates without a manipulated signal, or a general field experiment not built on matched signals."
    ),
    // Statistical Modeling & Inference
    method!(
        "statistical_validity",
        "Method — Statistical Validity",
        STATISTICAL_INFERENCE, Family,
        "Statistical inference, estimation, uncertainty, prediction, or sampling claims require broad scrutiny not exhausted by a listed specialized statistical role.",
        "a paper with no stochastic evidence or uncertainty claims, or a narrow listed statistical role that fully covers the material inferential question."
    ),
    method!(
        "bayesian_inference",
        "Method — Bayesian Inference",
        STATISTICAL_INFERENCE, Specific,
        "Prior distributions, hierarchical Bayes, posterior computation, Bayesian model comparison, or posterior decision statements materially support the conclusions.",
        "Bayesian terminology appears only in motivation, or a standard posterior is used as a minor computational input with no claim sensitive to prior, hierarchy, or posterior approximation."
    ),
    method!(
        "time_series_forecasting",
        "Method — Time Series & Forecasting",
        STATISTICAL_INFERENCE, Specific,
        "Temporal dependence, dynamic models, forecasting, nowcasting, spectral analysis, or time-series decomposition materially supports a central claim.",
        "panel timing used only for causal treatment comparisons, or repeated observations whose serial structure is incidental and handled routinely."
    ),
    method!(
        "panel_longitudinal",
        "Method — Panel & Longitudinal Analysis",
        STATISTICAL_INFERENCE, Specific,
        "Repeated observations on people, firms, places, organisms, or other units are used to identify within-unit change, trajectories, transitions, or dynamic heterogeneity.",
        "a two-period policy contrast whose main logic is difference-in-differences, or a single aggregate time series without repeated cross-sectional units."
    ),
    method!(
        "spatial_dependence",
        "Method — Spatial Dependence & Spatial Econometrics",
        STATISTICAL_INFERENCE, Specific,
        "Spatial correlation, interference, spillovers, neighborhood structure, boundary effects, or spatial statistical models materially determine inference.",
        "a map is only descriptive, coordinates are used only for routine geocoding, or remote-sensing measurement rather than spatial dependence is the central methodological issue."
    ),
    method!(
        "survival_event_history",
        "Method — Survival & Event-History Analysis",
        STATISTICAL_INFERENCE, Specific,
        "Time-to-event, hazard, duration, recurrent-event, competing-risk, multistate, or event-history methods support a central conclusion.",
        "a binary endpoint is observed at a fixed horizon with negligible censoring, or event timing appears only as a descriptive summary."
    ),
    method!(
        "missing_data_attrition",
        "Method — Missing Data & Attrition",
        STATISTICAL_INFERENCE, Specific,
        "Missing outcomes, attrition, nonresponse, censoring, incomplete covariates, imputation, or missing-not-at-random assumptions materially affect a central result.",
        "missingness is negligible, fully mechanical, and demonstrably unrelated to the conclusions, or only incidental reporting fields are incomplete."
    ),
    method!(
        "multiple_testing_selective_reporting",
        "Method — Multiplicity & Selective Reporting",
        STATISTICAL_INFERENCE, Specific,
        "Many outcomes, subgroups, models, hypotheses, researcher choices, or sequential analyses create a material multiplicity or selective-reporting problem.",
        "the paper has one prespecified primary analysis and no material model, outcome, subgroup, or stopping multiplicity."
    ),
    method!(
        "high_dimensional_regularization",
        "Method — High-Dimensional Estimation & Regularization",
        STATISTICAL_INFERENCE, Specific,
        "Regularization, variable selection, many controls, high-dimensional parameters, sparse models, or post-selection inference materially supports a claim.",
        "a small conventional regression or an algorithmic prediction contribution whose main question is benchmark performance rather than statistical inference after regularization."
    ),
    method!(
        "latent_variable_psychometrics",
        "Method — Latent Variables & Psychometrics",
        STATISTICAL_INFERENCE, Specific,
        "Scales, tests, latent traits, factor models, item-response models, structural equation models, or measurement invariance support substantive conclusions.",
        "a directly observed outcome or an unsupervised representation used only for prediction without a latent-construct interpretation."
    ),
    method!(
        "uncertainty_sensitivity_analysis",
        "Method — Uncertainty & Sensitivity Analysis",
        STATISTICAL_INFERENCE, Specific,
        "Global or local sensitivity, uncertainty propagation, robustness envelopes, partial identification, scenario uncertainty, or probabilistic risk analysis is central to the claims.",
        "routine standard errors alone, or a few conventional robustness specifications with no substantive uncertainty-analysis contribution."
    ),
    method!(
        "partial_identification_bounds",
        "Method — Partial Identification & Bounds",
        STATISTICAL_INFERENCE, Specific,
        "Bounds, breakdown points, or formal sensitivity regions — rather than point estimates — carry a central conclusion.",
        "a routine robustness table around a point estimate, or sensitivity analysis of a physical model's parameters."
    ),
    method!(
        "quantile_distributional_effects",
        "Method — Quantile & Distributional Effects",
        STATISTICAL_INFERENCE, Specific,
        "Quantile treatment effects, distribution regression, or distributional decompositions carry a central claim about effects beyond the mean.",
        "quantile regression used incidentally, or inequality description without an effect or decomposition claim."
    ),
    method!(
        "extreme_value_tail_risk",
        "Method — Extreme Values & Tail Risk",
        STATISTICAL_INFERENCE, Specific,
        "Extreme-value theory, return levels, exceedance probabilities, or tail-risk estimation carries a central claim about rare events.",
        "tail behavior mentioned descriptively, or risk measures computed from the body of the distribution with standard methods."
    ),
    // Measurement, Data & Records
    method!(
        "measurement_data",
        "Method — Measurement & Data",
        MEASUREMENT_RECORDS, Family,
        "Data construction, measurement, sampling frames, coding, or descriptive facts materially support the paper outside a narrower data-source or instrument role.",
        "a paper that uses only standard public measures, or a material problem fully covered by administrative-data/linkage, survey, geospatial, imaging, omics, sensor, or materials-characterization review."
    ),
    method!(
        "survey_research",
        "Method — Survey Research",
        MEASUREMENT_RECORDS, Specific,
        "Questionnaire design, respondent sampling, weighting, nonresponse, reporting behavior, or survey experiments materially determine the evidence.",
        "administrative or sensor data with no survey instrument, or surveys used only for a minor control variable."
    ),
    method!(
        "database_record_linkage",
        "Method — Administrative Data & Record Linkage",
        MEASUREMENT_RECORDS, Specific,
        "Administrative records, registries, transactions, electronic records, entity resolution, probabilistic linkage, or longitudinal database construction materially supports the evidence.",
        "a clean public dataset is analyzed without a material coverage, linkage, entity, provenance, or changing-definition problem."
    ),
    method!(
        "ml_derived_measures_inference",
        "Method — ML-Derived Measures & Downstream Inference",
        MEASUREMENT_RECORDS, Specific,
        "Model-generated variables — predicted attributes, imputed classes, or extracted measures — enter downstream estimation that carries central claims.",
        "the ML system itself is the contribution, or generated measures are validated incidentals that no central estimate depends on."
    ),
    // Computation, Simulation & Modeling
    method!(
        "quantitative_computation",
        "Method — Quantitative Computation",
        COMPUTATIONAL_MODELING, Family,
        "Calibration, numerical solution of quantitative models, dynamic computation, or model counterfactuals support central results outside a narrower computational role.",
        "routine data processing, or a claim fully covered by optimization/control, inverse-problem/data-assimilation, finite-element/discretization, or simulation-numerics review."
    ),
    method!(
        "structural_estimation",
        "Method — Structural Estimation",
        COMPUTATIONAL_MODELING, Specific,
        "Estimated structural parameters or model-based counterfactuals are central.",
        "reduced-form estimation, calibration without estimation, or a theoretical structural model with no fitted parameters."
    ),
    method!(
        "optimization_control",
        "Method — Optimization & Control",
        COMPUTATIONAL_MODELING, Specific,
        "An optimization problem, control policy, operations-research formulation, optimal design, or constrained decision rule carries a central result.",
        "optimization is only a routine estimation subroutine, or a theoretical optimum is stated without a computational, control, or decision-performance claim."
    ),
    method!(
        "inverse_problems_data_assimilation",
        "Method — Inverse Problems & Data Assimilation",
        COMPUTATIONAL_MODELING, Specific,
        "Latent states or physical parameters are recovered from indirect measurements through inversion, regularization, filtering, or data assimilation.",
        "ordinary parameter estimation with directly observed outcomes, or a forward simulation with no material inverse or state-reconstruction problem."
    ),
    method!(
        "finite_element_discretization",
        "Method — Finite Elements & Discretization",
        COMPUTATIONAL_MODELING, Specific,
        "Finite-element, finite-volume, finite-difference, spectral, meshfree, or related discretization methods materially support a scientific or engineering conclusion.",
        "a black-box solver is used routinely and the claimed result is demonstrably insensitive to discretization, mesh, boundary treatment, and solver behavior."
    ),
    method!(
        "simulation_numerics",
        "Method — Simulation & Numerics",
        COMPUTATIONAL_MODELING, Specific,
        "Monte Carlo evidence, numerical approximation, simulation accuracy, or synthetic experiments outside a listed narrower computational role are central.",
        "numerical implementation is routine, or finite-element/discretization, optimization/control, or inverse-problem behavior fully covers the material numerical claim."
    ),
    method!(
        "agent_based_modeling",
        "Method — Agent-Based Modeling",
        COMPUTATIONAL_MODELING, Specific,
        "Agent-based or individual-based simulation generates the evidence for a central mechanism, pattern, or policy claim.",
        "equation-based simulation without heterogeneous interacting agents, or an ABM used only as an illustrative appendix."
    ),
    method!(
        "microsimulation_policy",
        "Method — Policy Microsimulation",
        COMPUTATIONAL_MODELING, Specific,
        "Tax-benefit, health-policy, or demographic microsimulation over micro-unit data produces central distributional or budgetary claims.",
        "aggregate calibrated models without micro-unit simulation, or a structural estimation exercise."
    ),
    method!(
        "electronic_structure_computation",
        "Method — Electronic-Structure Computation",
        COMPUTATIONAL_MODELING, Specific,
        "Density-functional or wavefunction electronic-structure calculations materially support a chemical, materials, or physical claim.",
        "classical force-field simulation, or electronic-structure results quoted from prior literature."
    ),
    method!(
        "atomistic_simulation",
        "Method — Atomistic & Molecular Simulation",
        COMPUTATIONAL_MODELING, Specific,
        "Molecular-dynamics or Monte Carlo simulation of atomistic systems materially supports a structural, thermodynamic, or mechanistic claim.",
        "quantum electronic-structure calculation without dynamics or sampling, or coarse-grained models far above atomistic resolution."
    ),
    method!(
        "climate_model_projection",
        "Method — Climate Modeling & Projection",
        COMPUTATIONAL_MODELING, Specific,
        "Climate or Earth-system model output — projections, attribution, downscaling, or scenarios — carries a central claim.",
        "observational climate analysis without model output, or impact modeling that takes climate projections as given inputs."
    ),
    method!(
        "energy_systems_iam",
        "Method — Energy Systems & Integrated Assessment",
        COMPUTATIONAL_MODELING, Specific,
        "Energy-system optimization or integrated assessment modeling produces central pathway, cost, or climate-economy claims.",
        "an econometric energy study, or climate-physics modeling without a technology or economy representation."
    ),
    method!(
        "scientific_ml_surrogates",
        "Method — Scientific ML & Surrogate Models",
        COMPUTATIONAL_MODELING, Specific,
        "Learned surrogates, emulators, physics-informed networks, or ML force fields replace simulation or measurement in support of a scientific claim.",
        "ML methodology evaluated on benchmarks without a scientific conclusion, or conventional simulation without a learned component."
    ),
    // Machine Learning & AI
    method!(
        "algorithmic_ml",
        "Method — Algorithms & Machine Learning",
        ML_AI, Family,
        "Algorithms, prediction systems, learning procedures, benchmarks, or computational complexity are central.",
        "a standard classifier or software package is merely used as a control or preprocessing device."
    ),
    method!(
        "llm_evaluation",
        "Method — LLM & Foundation-Model Evaluation",
        ML_AI, Specific,
        "Claims about the capabilities, behavior, or comparative performance of large language or foundation models rest on benchmark or task evaluations.",
        "an ML contribution unrelated to foundation models, or a study using an LLM only as a tool to measure something else."
    ),
    method!(
        "llm_research_instruments",
        "Method — LLMs as Research Instruments",
        ML_AI, Specific,
        "Language models act as annotators, simulated participants, data generators, or analysts inside a study whose conclusions are about something else.",
        "papers evaluating the models themselves, or conventional NLP measurement covered by computational text analysis."
    ),
    method!(
        "prediction_model_validation",
        "Method — Prediction-Model Development & Validation",
        ML_AI, Specific,
        "An individual-level prediction model — a clinical risk score or comparable tool — is developed or validated as a central contribution.",
        "ML benchmark work without an applied prediction target, or causal-effect estimation dressed in predictive form."
    ),
    method!(
        "fairness_algorithm_audit",
        "Method — Algorithmic Fairness & Audits",
        ML_AI, Specific,
        "Fairness measurement, disparity analysis, or an audit of an algorithmic system carries a central claim.",
        "human-discrimination audit designs with fictitious applications, or ethics discussion without measured system behavior."
    ),
    method!(
        "differential_privacy_formal",
        "Method — Formal Privacy & Disclosure",
        ML_AI, Specific,
        "Differential privacy or comparable formal privacy and disclosure guarantees, with their utility tradeoffs, are central to the contribution.",
        "security mechanisms without a formal privacy definition, or privacy discussed only as an ethical consideration."
    ),
    method!(
        "benchmark_dataset_construction",
        "Method — Benchmark & Dataset Construction",
        ML_AI, Specific,
        "The contribution is a benchmark, evaluation suite, or dataset built to measure other systems, with construction and validation choices to audit.",
        "a substantive dataset introduced only to answer the paper's own question, which the data-descriptor genre or measurement roles cover."
    ),
    method!(
        "quantum_advantage_claims",
        "Method — Quantum-Advantage Claims",
        ML_AI, Specific,
        "A claim of quantum computational advantage, speedup, or utility against classical baselines is central.",
        "quantum hardware physics without a computational-advantage claim, or quantum algorithm theory with no comparative performance assertion."
    ),
    // Networks, Text & Digital Traces
    method!(
        "network_analysis",
        "Method — Network Analysis",
        NETWORKS_TEXT_TRACES, Specific,
        "Relational data, graph construction, centrality, communities, diffusion, link prediction, or network dependence materially support a central claim.",
        "a graph is only a computational data structure or visual illustration with no substantive relational inference."
    ),
    method!(
        "computational_text_analysis",
        "Method — Computational Text Analysis",
        NETWORKS_TEXT_TRACES, Specific,
        "Dictionary, topic, embedding, classifier, large-language-model, corpus, or other text-as-data measurements support substantive claims about documents, speakers, or discourse.",
        "the contribution is an NLP algorithm itself or a close reading that does not rely on automated text measurement."
    ),
    method!(
        "bibliometric_scientometric",
        "Method — Bibliometric & Scientometric Analysis",
        NETWORKS_TEXT_TRACES, Specific,
        "Publication, citation, authorship, collaboration, patent, or scholarly-communication records are analyzed to make central claims about research systems or knowledge structure.",
        "citations are used only for literature positioning or a systematic review synthesizes study findings rather than publication-system patterns."
    ),
    method!(
        "digital_trace_social_media",
        "Method — Digital Traces & Platform Data",
        NETWORKS_TEXT_TRACES, Specific,
        "Platform or digital-trace data — social media, search, app logs — supports substantive claims about behavior, attitudes, or populations.",
        "text content analysis covered by computational text analysis, or platform data used only to recruit participants."
    ),
    method!(
        "corpus_linguistics",
        "Method — Corpus Linguistics",
        NETWORKS_TEXT_TRACES, Specific,
        "Corpus frequencies, distributions, or attestation patterns support central linguistic claims.",
        "automated text measurement of social phenomena, or theoretical linguistics without corpus evidence."
    ),
    // Health & Clinical Research
    method!(
        "clinical_study",
        "Method — Clinical Study & Diagnostic Validity",
        HEALTH_CLINICAL, Specific,
        "Patient cohorts, diagnostic or prognostic models, clinical interventions, medical endpoints, or translational claims are central.",
        "nonclinical laboratory biology or population research with no patient-facing or diagnostic inference."
    ),
    method!(
        "epidemiologic_observational",
        "Method — Observational Epidemiology",
        HEALTH_CLINICAL, Specific,
        "Cohort, case-control, cross-sectional, registry, surveillance, or population-exposure evidence supports etiologic, prognostic, or burden claims.",
        "a randomized clinical trial, nonhuman laboratory study, or administrative-data analysis with no population-health or disease inference."
    ),
    method!(
        "genomic_omics_assays",
        "Method — Genomic & Omics Assays",
        HEALTH_CLINICAL, Specific,
        "Sequencing, genotyping, transcriptomic, epigenomic, proteomic, metabolomic, single-cell, or multi-omics assays materially support biological conclusions.",
        "a small targeted assay with routine interpretation, or genomic data used only as a benchmark for a general computational method."
    ),
    method!(
        "preclinical_animal_studies",
        "Method — Preclinical Animal Studies",
        HEALTH_CLINICAL, Specific,
        "In vivo animal experiments — disease models, interventions, or mechanism tests — carry central claims.",
        "human studies, in vitro work without animals, or behavioral lab research covered by laboratory-experiment review."
    ),
    method!(
        "neuroimaging_analysis",
        "Method — Neuroimaging Analysis",
        HEALTH_CLINICAL, Specific,
        "fMRI, EEG, MEG, PET, or similar brain-measurement analyses carry central brain-behavior or clinical claims.",
        "clinical radiology diagnosis, microscopy of neural tissue, or behavior-only studies without brain measurement."
    ),
    method!(
        "pkpd_dose_response",
        "Method — PK/PD & Dose-Response",
        HEALTH_CLINICAL, Specific,
        "Pharmacokinetic, pharmacodynamic, or dose-response modeling supports central dosing, exposure, or toxicity claims.",
        "clinical-outcome trials without exposure modeling, or biochemical assays without a dose-response inference."
    ),
    method!(
        "transmission_dynamics_modeling",
        "Method — Infectious-Disease Transmission Modeling",
        HEALTH_CLINICAL, Specific,
        "Epidemic transmission models, reproduction-number estimation, or intervention counterfactuals for infectious disease carry central claims.",
        "descriptive epidemiology without a transmission model, or non-infectious simulation covered by other computational roles."
    ),
    method!(
        "health_economic_evaluation",
        "Method — Health Economic Evaluation",
        HEALTH_CLINICAL, Specific,
        "Cost-effectiveness, cost-utility, or budget-impact analysis carries a central health-economic claim.",
        "clinical effectiveness without economic modeling, or general welfare analysis outside health covered by economic roles."
    ),
    method!(
        "implementation_process_evaluation",
        "Method — Implementation & Process Evaluation",
        HEALTH_CLINICAL, Specific,
        "Fidelity, adaptation, mechanism, and context evidence about how an intervention was delivered carries central claims.",
        "outcome-effectiveness estimation covered by trial or causal roles, or program theory evaluation as the central frame."
    ),
    method!(
        "gwas_polygenic_scores",
        "Method — GWAS & Polygenic Scores",
        HEALTH_CLINICAL, Specific,
        "Genome-wide association analysis or polygenic-score construction and application carries central claims.",
        "instrumented causal contrasts covered by Mendelian randomization, or molecular assays covered by genomic and omics review."
    ),
    // Laboratory & Instrumentation
    method!(
        "observational_science",
        "Method — Observational Science & Instrumentation",
        LAB_INSTRUMENTATION, Family,
        "Instrument-derived observational data, field measurements, surveys of natural systems, detection pipelines, or observational selection functions support claims outside a narrower instrument or population role.",
        "designed laboratory interventions, social-science causal identification, or evidence fully covered by epidemiologic, geospatial, imaging, sensor, or materials-characterization review."
    ),
    method!(
        "microscopy_imaging",
        "Method — Microscopy & Scientific Imaging",
        LAB_INSTRUMENTATION, Specific,
        "Microscopy, tomography, scientific image acquisition, segmentation, registration, reconstruction, or quantitative image analysis materially supports a scientific claim.",
        "clinical diagnostic imaging whose main issue is patient-level validity, or images used only as qualitative illustrations with no measurement claim."
    ),
    method!(
        "sensor_signal_processing",
        "Method — Sensors & Signal Processing",
        LAB_INSTRUMENTATION, Specific,
        "Sensor calibration, filtering, detection, spectral or waveform analysis, feature extraction, synchronization, or signal reconstruction materially supports a result.",
        "standard instrument outputs are accepted without a signal-processing claim, or image and remote-sensing pipelines are the central methodological object."
    ),
    method!(
        "materials_characterization",
        "Method — Materials Characterization",
        LAB_INSTRUMENTATION, Specific,
        "Spectroscopy, diffraction, microscopy, thermal analysis, mechanical testing, surface analysis, or complementary characterization establishes material identity, structure, or properties.",
        "routine confirmation of a known material, or device performance and safety rather than material characterization carries the main claim."
    ),
    method!(
        "geospatial_remote_sensing",
        "Method — Geospatial & Remote Sensing",
        LAB_INSTRUMENTATION, Specific,
        "GIS construction, spatial resolution, remote-sensing retrievals, map projections, geolocation, or geographic measurement materially support results.",
        "a paper that merely includes a map or location fixed effects, or a claim whose material issue is spatial statistical dependence rather than geospatial measurement."
    ),
    method!(
        "analytical_chemistry_validation",
        "Method — Analytical-Chemistry Validation",
        LAB_INSTRUMENTATION, Specific,
        "Quantitative chemical measurement — chromatography, mass spectrometry, spectroscopy — with calibration and validation carries central claims.",
        "materials-property characterization, or synthetic-route evidence covered by synthesis and characterization review."
    ),
    method!(
        "synthesis_compound_characterization",
        "Method — Synthesis & Compound Characterization",
        LAB_INSTRUMENTATION, Specific,
        "New compounds, synthetic routes, yields, and their spectroscopic characterization carry central claims.",
        "quantitative analysis of known analytes, or biological activity claims covered by assay and preclinical roles."
    ),
    method!(
        "crystallography_structure",
        "Method — Crystallography & Structure Determination",
        LAB_INSTRUMENTATION, Specific,
        "Diffraction or cryo-EM structure determination and the mechanistic conclusions drawn from structural models carry central claims.",
        "microscopy imaging without atomic-model refinement, or structures quoted from depositions without new determination."
    ),
    // Qualitative & Interpretive
    method!(
        "qualitative_case_study",
        "Method — Qualitative & Case Evidence",
        QUALITATIVE_INTERPRETIVE, Family,
        "Qualitative or case evidence carries central claims but spans approaches or does not fit a listed ethnographic, interview, process-tracing, or configurational role.",
        "archival source criticism alone, purely textual interpretation, cases used only as illustrations, or a single listed qualitative role that fully covers the inferential strategy."
    ),
    method!(
        "ethnography",
        "Method — Ethnography & Participant Observation",
        QUALITATIVE_INTERPRETIVE, Specific,
        "Sustained fieldwork, participant observation, fieldnotes, situated interaction, or multisited ethnography supplies central evidence.",
        "interviews without sustained observation or field immersion, archival research, or cases reconstructed entirely from documents."
    ),
    method!(
        "interviews_focus_groups",
        "Method — Interviews & Focus Groups",
        QUALITATIVE_INTERPRETIVE, Specific,
        "Semistructured, structured, life-history, elite, expert, or focus-group interviews materially support the conclusions.",
        "informal conversations incidental to ethnography, questionnaire items analyzed quantitatively, or quotations used only as illustrations."
    ),
    method!(
        "process_tracing",
        "Method — Process Tracing",
        QUALITATIVE_INTERPRETIVE, Specific,
        "Within-case sequences, causal-process observations, hoop or smoking-gun tests, or mechanism-centered case reconstruction substantiate a central causal explanation.",
        "chronological narrative without explicit mechanism tests, cross-case covariation alone, or archival source criticism with no within-case causal inference."
    ),
    method!(
        "comparative_configurational",
        "Method — Comparative & Configurational Analysis",
        QUALITATIVE_INTERPRETIVE, Specific,
        "Small- or medium-N comparison, qualitative comparative analysis, set-theoretic methods, typologies, or necessary and sufficient configurations support a central claim.",
        "a large-sample regression with country indicators, a single case, or paired examples used only for exposition."
    ),
    method!(
        "textual_interpretive",
        "Method — Textual & Interpretive Analysis",
        QUALITATIVE_INTERPRETIVE, Specific,
        "Close reading, hermeneutics, discourse analysis, rhetoric, translation, or interpretation of cultural texts carries the contribution.",
        "automated text measurement alone or texts used only as sources of factual observations."
    ),
    method!(
        "mixed_methods",
        "Method — Mixed-Methods Integration",
        QUALITATIVE_INTERPRETIVE, Specific,
        "The contribution depends on integrating qualitative and quantitative evidence rather than presenting them as independent appendages.",
        "a paper with multiple methods whose conclusions do not rely on their integration."
    ),
    method!(
        "participatory_community_research",
        "Method — Participatory & Community Research",
        QUALITATIVE_INTERPRETIVE, Specific,
        "Community-based participatory research, action research, co-production, citizen science, or stakeholder-governed inquiry materially shapes the evidence and claims.",
        "participants merely provide data or feedback without shared agenda setting, interpretation, governance, or action."
    ),
    method!(
        "creative_practice_research",
        "Method — Creative & Practice-Led Research",
        QUALITATIVE_INTERPRETIVE, Specific,
        "Artistic, performative, curatorial, compositional, literary, or other creative practice is used to generate and substantiate a central research claim.",
        "a creative work is only the object of interpretation, or an artifact is evaluated primarily as a functional design intervention."
    ),
    method!(
        "discourse_conversation_analysis",
        "Method — Discourse & Conversation Analysis",
        QUALITATIVE_INTERPRETIVE, Specific,
        "Sequential analysis of talk or fine-grained discourse analysis of interaction carries central claims about practice.",
        "thematic interview analysis covered by interview review, or literary text interpretation covered by textual-interpretive review."
    ),
    // Sources, History & Law
    method!(
        "archival_source_criticism",
        "Method — Archival & Primary-Source Criticism",
        SOURCES_HISTORY_LAW, Specific,
        "Archival records, manuscripts, legal or administrative documents, material archives, or primary-source provenance carry historical claims.",
        "secondary-source synthesis or interviews and ethnography without a material archival evidentiary problem."
    ),
    method!(
        "historical_comparative",
        "Method — Historical & Comparative Reasoning",
        SOURCES_HISTORY_LAW, Specific,
        "Periodization, sequence, path dependence, comparative cases, or process-based historical explanation supports the central argument.",
        "a single contemporaneous case with no historical or comparative explanatory claim."
    ),
    method!(
        "legal_doctrinal",
        "Method — Doctrinal Legal Reasoning",
        SOURCES_HISTORY_LAW, Specific,
        "Interpretation of cases, statutes, regulations, constitutional provisions, precedent, or institutional legal authority is central.",
        "empirical legal studies whose main claims do not depend on doctrinal interpretation."
    ),
    method!(
        "textual_criticism_editions",
        "Method — Textual Criticism & Critical Editions",
        SOURCES_HISTORY_LAW, Specific,
        "Editions, stemmata, attributions, datings, or emendations of texts as artifacts carry central claims.",
        "interpretation of established texts, or archival evidence about events covered by source criticism."
    ),
    method!(
        "archaeological_field_methods",
        "Method — Archaeological Field Methods",
        SOURCES_HISTORY_LAW, Specific,
        "Stratigraphy, excavation data, dating programs, or material assemblages carry central claims about past activity.",
        "documentary history, heritage-policy discussion, or laboratory analyses reviewed by their own instrument roles."
    ),
    // Synthesis, Evaluation & Meta-research
    method!(
        "systematic_review_meta_analysis",
        "Method — Systematic Review & Meta-analysis",
        SYNTHESIS_META, Specific,
        "Evidence search, study inclusion, effect harmonization, evidence grading, or quantitative synthesis across studies is central.",
        "an ordinary narrative literature review or paper citing several prior estimates without systematic synthesis."
    ),
    method!(
        "metascience_replication",
        "Method — Metascience & Large-Scale Replication",
        SYNTHESIS_META, Specific,
        "Multi-lab replication projects, reproducibility assessments, or meta-research about literatures and scientific practice carry central claims.",
        "a paper replicating one identified study, which the replication genre reviews, or a standard systematic review of substantive findings."
    ),
    method!(
        "program_theory_evaluation",
        "Method — Theory-Based Program Evaluation",
        SYNTHESIS_META, Specific,
        "Theory-of-change, realist, or contribution-analysis evaluation logic carries the central claim that and how a program worked.",
        "effect estimation by experimental or quasi-experimental design, or implementation measurement without a contribution claim."
    ),
    method!(
        "expert_elicitation_delphi",
        "Method — Expert Elicitation & Delphi",
        SYNTHESIS_META, Specific,
        "Structured expert judgment — Delphi rounds, elicited probabilities, consensus processes — supplies central quantities or conclusions.",
        "ordinary author judgment, surveys of non-expert populations, or stakeholder consultation without structured elicitation."
    ),
    // Engineering, Design & Applied Evaluation
    method!(
        "engineering_validation",
        "Method — Engineering Validation & Safety",
        ENGINEERING_DESIGN, Specific,
        "Prototype testing, tolerances, reliability, standards, verification, failure modes, scale-up, or safety margins support an engineering claim.",
        "basic scientific experiments with no design-performance, reliability, or safety claim."
    ),
    method!(
        "design_based_research",
        "Method — Design-Based & Practice Research",
        ENGINEERING_DESIGN, Specific,
        "Iterative design, research-through-design, design-based implementation, prototyping with users, or practice-based inquiry is itself the evidentiary strategy.",
        "ordinary product engineering, a one-shot usability test, or an intervention evaluated without a material iterative-design claim."
    ),
    method!(
        "hci_user_studies",
        "Method — HCI & Usability Studies",
        ENGINEERING_DESIGN, Specific,
        "User studies of interfaces, systems, or interaction techniques — lab or deployed — carry central usability or comparative claims.",
        "psychological lab experiments about cognition, or survey research without an evaluated system."
    ),
    method!(
        "life_cycle_assessment",
        "Method — Life-Cycle Assessment",
        ENGINEERING_DESIGN, Specific,
        "Life-cycle assessment with functional units, system boundaries, and impact categories carries central environmental claims.",
        "environmental measurement without life-cycle accounting, or energy-system pathway modeling."
    ),
    // Reproducibility, Integrity & Governance
    method!(
        "reproducibility_software",
        "Method — Reproducibility & Research Software",
        INTEGRITY_GOVERNANCE, Specific,
        "Custom software, computational workflows, data pipelines, package behavior, or reproducible artifacts materially support the scientific result.",
        "routine use of standard software with no software, workflow, or reproducibility claim."
    ),
    method!(
        "research_ethics_governance",
        "Method — Research Ethics & Governance",
        INTEGRITY_GOVERNANCE, Specific,
        "Consent, participant or animal welfare, community authority, data governance, conflicts, dual-use risk, or responsible deployment materially affects the validity or permissible scope of the research claim.",
        "ethics approval is routine, adequately documented, and not material to interpreting or disseminating the central findings."
    ),
    method!(
        "statistical_reporting_consistency",
        "Method — Statistical-Reporting Consistency",
        INTEGRITY_GOVERNANCE, Specific,
        "The paper's reported statistics are dense enough that recomputing test statistics, intervals, and table arithmetic can materially check its claims.",
        "papers with few reported statistics, or statistical-design concerns covered by the statistical-inference family."
    ),
    method!(
        "image_data_integrity",
        "Method — Image & Data Integrity Signals",
        INTEGRITY_GOVERNANCE, Specific,
        "Figures and data presentations are central evidence and can be checked for duplication, splicing, or figure-text inconsistency from the provided assets.",
        "papers whose figures are schematic illustrations, or image-analysis methodology covered by imaging roles."
    ),
    method!(
        "citation_accuracy",
        "Method — Citation Accuracy",
        INTEGRITY_GOVERNANCE, Specific,
        "The paper's motivation, premises, or positioning rest on characterizations of cited work that can be spot-checked.",
        "papers whose claims are self-contained, or literature synthesis quality covered by survey-genre or systematic-review roles."
    ),
    method!(
        "replication_package_audit",
        "Method — Replication-Package Audit",
        INTEGRITY_GOVERNANCE, Specific,
        "The paper describes or includes a data-and-code package whose completeness and traceability to the headline results can be audited.",
        "papers without a computational pipeline, or software presented as the contribution, which the software-paper genre reviews."
    ),
];
