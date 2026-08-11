use super::{SubjectLevel, SubjectSpec};

const PHYSICS: &str = include_str!("../../../../prompts/auto_review/subjects/physics.md");
const MATHEMATICS: &str = include_str!("../../../../prompts/auto_review/subjects/mathematics.md");
const STATISTICS: &str = include_str!("../../../../prompts/auto_review/subjects/statistics.md");
const COMPUTER_SCIENCE: &str =
    include_str!("../../../../prompts/auto_review/subjects/computer_science.md");
const ENGINEERING: &str = include_str!("../../../../prompts/auto_review/subjects/engineering.md");
const BIOLOGY: &str = include_str!("../../../../prompts/auto_review/subjects/biology.md");
const CHEMISTRY: &str = include_str!("../../../../prompts/auto_review/subjects/chemistry.md");
const EARTH_ENVIRONMENT: &str =
    include_str!("../../../../prompts/auto_review/subjects/earth_environment.md");
const MEDICINE_HEALTH: &str =
    include_str!("../../../../prompts/auto_review/subjects/medicine_health.md");
const SOCIOLOGY: &str = include_str!("../../../../prompts/auto_review/subjects/sociology.md");
const POLITICAL_SCIENCE: &str =
    include_str!("../../../../prompts/auto_review/subjects/political_science.md");
const HISTORY: &str = include_str!("../../../../prompts/auto_review/subjects/history.md");
const ECONOMICS: &str = include_str!("../../../../prompts/auto_review/subjects/economics.md");
const PSYCHOLOGY: &str = include_str!("../../../../prompts/auto_review/subjects/psychology.md");
const ANTHROPOLOGY: &str = include_str!("../../../../prompts/auto_review/subjects/anthropology.md");
const GEOGRAPHY: &str = include_str!("../../../../prompts/auto_review/subjects/geography.md");
const PHILOSOPHY: &str = include_str!("../../../../prompts/auto_review/subjects/philosophy.md");
const LINGUISTICS: &str = include_str!("../../../../prompts/auto_review/subjects/linguistics.md");
const EDUCATION: &str = include_str!("../../../../prompts/auto_review/subjects/education.md");
const LAW: &str = include_str!("../../../../prompts/auto_review/subjects/law.md");
const BUSINESS: &str = include_str!("../../../../prompts/auto_review/subjects/business.md");
const HUMANITIES: &str = include_str!("../../../../prompts/auto_review/subjects/humanities.md");
const AGRICULTURE_VETERINARY: &str =
    include_str!("../../../../prompts/auto_review/subjects/agriculture_veterinary.md");
const COMMUNICATION_INFORMATION: &str =
    include_str!("../../../../prompts/auto_review/subjects/communication_information.md");
const ARCHITECTURE_DESIGN: &str =
    include_str!("../../../../prompts/auto_review/subjects/architecture_design.md");
const SOCIAL_WORK_POLICY: &str =
    include_str!("../../../../prompts/auto_review/subjects/social_work_policy.md");
const HEALTH_PROFESSIONS: &str =
    include_str!("../../../../prompts/auto_review/subjects/health_professions.md");
const INTERDISCIPLINARY_STUDIES: &str =
    include_str!("../../../../prompts/auto_review/subjects/interdisciplinary_studies.md");

macro_rules! subject {
    ($id:literal, $label:literal, $discipline:literal, $discipline_label:literal,
     $level:ident, $description:literal, $exclusions:literal, $base:ident, $focus:literal) => {
        SubjectSpec {
            id: $id,
            label: $label,
            discipline_id: $discipline,
            discipline_label: $discipline_label,
            level: SubjectLevel::$level,
            routing_description: $description,
            routing_exclusions: $exclusions,
            discipline_prompt: $base,
            review_focus: $focus,
        }
    };
}

pub const SUBJECTS: &[SubjectSpec] = &[
    // Physics
    subject!(
        "subject_physics_general", "Physics — General", "physics", "Physics", Discipline,
        "Broad physics research that does not fit a more specific physics subfield in this catalog.",
        "a listed physics subfield clearly carries the main contribution.", PHYSICS,
        "Evaluate the paper's physical question, governing scales, conservation principles, approximations, and connection between theoretical objects and observable quantities. Use this fallback only when the contribution spans several physics areas or is genuinely outside the more specific lenses below."
    ),
    subject!(
        "subject_physics_theoretical_mathematical", "Physics — Theoretical & Mathematical", "physics", "Physics", Subfield,
        "Theoretical or mathematical physics centered on formal physical models, symmetries, fields, or exact structure.",
        "the result is primarily a pure mathematical theorem without a material physical interpretation.", PHYSICS,
        "Focus on whether the mathematical formulation represents the claimed physical system, whether symmetries and limiting regimes are used consistently, and whether formal results yield the stated physical content. Separate a mathematically valid construction from an argument that it describes the relevant physics."
    ),
    subject!(
        "subject_physics_particle_nuclear", "Physics — Particle & Nuclear", "physics", "Physics", Subfield,
        "Particle, high-energy, nuclear, hadronic, accelerator, or fundamental-interaction research.",
        "astrophysical use of particle models is central but laboratory or nuclear physics is not.", PHYSICS,
        "Examine the interaction model, quantum numbers, kinematic regimes, backgrounds, detector acceptance, effective-theory scale, and relation between measured observables and the claimed particle or nuclear parameter. Check whether exclusions and discovery claims respect the stated model dependence."
    ),
    subject!(
        "subject_physics_condensed_materials", "Physics — Condensed Matter & Materials", "physics", "Physics", Subfield,
        "Condensed-matter, soft-matter, mesoscopic, many-body, or materials-physics research.",
        "the primary contribution is materials synthesis or engineering performance rather than physical mechanism.", PHYSICS,
        "Assess the phase, excitation, transport, ordering mechanism, finite-size and disorder effects, sample regime, and connection between microscopic assumptions and macroscopic observables. Ask whether alternative mechanisms or material heterogeneity could produce the same signature."
    ),
    subject!(
        "subject_physics_amo_quantum", "Physics — AMO & Quantum Optics", "physics", "Physics", Subfield,
        "Atomic, molecular, optical, photonic, ultracold-matter, or quantum-optics research.",
        "the paper is chiefly about quantum algorithms or information protocols rather than a physical AMO platform.", PHYSICS,
        "Review state preparation, coherence and decoherence, level structure, light-matter interaction, control sequence, readout, and experimentally accessible parameter regimes. Check whether idealized quantum dynamics survive the noise, loss, and calibration structure of the claimed platform."
    ),
    subject!(
        "subject_physics_statistical_complex", "Physics — Statistical & Complex Systems", "physics", "Physics", Subfield,
        "Statistical mechanics, nonlinear dynamics, complex systems, networks in physics, or nonequilibrium phenomena.",
        "network analysis is primarily social or computational and lacks a physical statistical-mechanics claim.", PHYSICS,
        "Focus on ensembles, thermodynamic or large-system limits, universality, phase transitions, ergodicity, fluctuations, scaling, and sensitivity to microscopic dynamics. Determine whether evidence actually distinguishes collective behavior from finite-size or fitting artifacts."
    ),
    subject!(
        "subject_physics_astrophysics_cosmology", "Physics — Astrophysics & Cosmology", "physics", "Physics", Subfield,
        "Astrophysics, astronomy, cosmology, gravitation, or large-scale-universe research.",
        "the main object is terrestrial geophysics or the work uses astronomy data only as an incidental application.", PHYSICS,
        "Examine source populations, selection functions, foregrounds, distance and time scales, cosmological assumptions, gravitational dynamics, and degeneracies between astrophysical processes and fundamental parameters. Check whether observational reach supports the claimed cosmic scope."
    ),
    subject!(
        "subject_physics_plasma_fluid", "Physics — Plasma & Fluid", "physics", "Physics", Subfield,
        "Plasma physics, fluid dynamics, magnetohydrodynamics, turbulence, or continuum-flow research.",
        "the main contribution is an engineering device with fluid behavior only as an input.", PHYSICS,
        "Review the relevant nondimensional regimes, closure assumptions, boundary conditions, stability, turbulence or transport mechanism, kinetic versus continuum approximation, and conservation properties. Check that simulations or experiments occupy the regime used in the interpretation."
    ),
    subject!(
        "subject_physics_geophysics", "Physics — Geophysics", "physics", "Physics", Subfield,
        "Physical study of Earth's interior, seismology, geomagnetism, geodynamics, or planetary interiors.",
        "the central contribution concerns climate, ecology, or descriptive geology rather than a physical Earth model.", PHYSICS,
        "Assess the physical Earth model, constitutive assumptions, inverse-problem resolution, source and propagation model, spatial scale, and compatibility with independent geophysical constraints. Distinguish what the data identify from what enters through regularization or prior structure."
    ),
    subject!(
        "subject_physics_quantum_information", "Physics — Quantum Information", "physics", "Physics", Subfield,
        "Quantum information, communication, sensing, error correction, or physically realized quantum computation.",
        "the contribution is a classical algorithm or an AMO experiment without an information-theoretic claim.", PHYSICS,
        "Examine resource assumptions, channel or noise model, entanglement and measurement structure, fault or error model, scaling, and the gap between ideal protocol and physical implementation. Check comparisons against the correct classical and quantum baselines."
    ),

    // Mathematics
    subject!(
        "subject_mathematics_general", "Mathematics — General", "mathematics", "Mathematics", Discipline,
        "Pure or applied mathematics spanning several areas or outside the listed mathematical subfields.",
        "a listed mathematical subfield clearly contains the main theorem or construction.", MATHEMATICS,
        "Evaluate the naturality of definitions and assumptions, strength and sharpness of the main results, illuminating examples and counterexamples, and the relation between the chosen formulation and the underlying mathematical structure. Use the general lens only when no narrower area dominates."
    ),
    subject!(
        "subject_mathematics_algebra_number", "Mathematics — Algebra & Number Theory", "mathematics", "Mathematics", Subfield,
        "Algebra, representation theory, algebraic geometry, arithmetic geometry, or number theory.",
        "the central result is geometric or analytic without material algebraic or arithmetic structure.", MATHEMATICS,
        "Focus on the category and invariants being studied, functoriality, hypotheses on rings, fields, schemes, groups, or representations, local-to-global steps, and the strength of classification or finiteness claims. Ask whether examples expose exceptional characteristics and boundary cases."
    ),
    subject!(
        "subject_mathematics_geometry_topology", "Mathematics — Geometry & Topology", "mathematics", "Mathematics", Subfield,
        "Differential, algebraic, symplectic, metric, or discrete geometry and algebraic or geometric topology.",
        "geometric language is merely a representation of an analytic or applied problem.", MATHEMATICS,
        "Review the geometric objects, regularity and compactness conditions, invariance, global versus local claims, singularities, moduli, and topological obstructions. Check whether constructions are intrinsic and whether examples cover the relevant geometric regimes."
    ),
    subject!(
        "subject_mathematics_analysis", "Mathematics — Analysis", "mathematics", "Mathematics", Subfield,
        "Real, complex, functional, harmonic, operator, or variational analysis not primarily organized around a PDE.",
        "the principal contribution is a differential-equation existence, regularity, or dynamics result.", MATHEMATICS,
        "Examine function spaces, modes of convergence, boundedness and compactness, measure-theoretic conditions, operator domains, sharp constants, and endpoint cases. Check that the topology and regularity used in conclusions match those established by the arguments."
    ),
    subject!(
        "subject_mathematics_pde", "Mathematics — PDE & Calculus of Variations", "mathematics", "Mathematics", Subfield,
        "Partial differential equations, calculus of variations, geometric flows, or continuum mathematical models.",
        "the equation is only a numerical test problem or routine applied model without a mathematical PDE contribution.", MATHEMATICS,
        "Focus on well-posedness, weak versus strong solutions, boundary and initial conditions, regularity, blow-up, coercivity, compactness, conservation, variational structure, and dependence on dimension or domain. Check whether the claimed solution concept is appropriate to the application and theorem."
    ),
    subject!(
        "subject_mathematics_probability", "Mathematics — Probability", "mathematics", "Mathematics", Subfield,
        "Probability theory, stochastic processes, random structures, concentration, or stochastic analysis.",
        "probability is used only for routine statistical inference rather than as the mathematical contribution.", MATHEMATICS,
        "Review probability spaces, filtrations, dependence, stopping or integrability conditions, asymptotic regime, tightness, modes of convergence, rare-event behavior, and stochastic regularity. Ask whether examples distinguish the claimed phenomenon from a standard limit theorem."
    ),
    subject!(
        "subject_mathematics_combinatorics", "Mathematics — Combinatorics & Discrete", "mathematics", "Mathematics", Subfield,
        "Combinatorics, graph theory, discrete geometry, extremal or probabilistic combinatorics.",
        "the graph or discrete representation serves only an algorithmic engineering objective.", MATHEMATICS,
        "Assess extremal constructions, counting regime, dependence on parameters, sharpness, probabilistic versus constructive arguments, forbidden configurations, and small or degenerate cases. Check whether the result materially advances the correct benchmark and whether examples establish necessity."
    ),
    subject!(
        "subject_mathematics_logic_foundations", "Mathematics — Logic & Foundations", "mathematics", "Mathematics", Subfield,
        "Mathematical logic, set theory, model theory, proof theory, computability, or foundations.",
        "formal verification is used only as a computer-science implementation tool.", MATHEMATICS,
        "Examine the formal system, language, metatheoretic assumptions, consistency strength, definability, interpretability, computability bounds, and model constructions. Verify that informal mathematical claims do not outrun what the stated foundational framework establishes."
    ),
    subject!(
        "subject_mathematics_applied_numerical", "Mathematics — Applied & Numerical", "mathematics", "Mathematics", Subfield,
        "Applied mathematics, numerical analysis, inverse problems, scientific computing, or approximation theory.",
        "the work is principally an engineering application or software benchmark without mathematical analysis.", MATHEMATICS,
        "Review the mathematical model, discretization, consistency, stability, convergence, conditioning, identifiability, approximation error, and relation between continuous and discrete problems. Distinguish empirical performance from a mathematical guarantee."
    ),
    subject!(
        "subject_mathematics_optimization_dynamics", "Mathematics — Optimization, Control & Dynamics", "mathematics", "Mathematics", Subfield,
        "Optimization theory, optimal control, operations research mathematics, dynamical systems, or ergodic dynamics.",
        "optimization is merely a routine fitting algorithm or the main contribution is an engineering controller.", MATHEMATICS,
        "Examine objective and constraint geometry, existence, duality, stationarity versus global optimality, controllability, stability, bifurcation, invariant sets, and long-run behavior. Check whether algorithmic or comparative claims use the appropriate solution concept and parameter regime."
    ),

    // Statistics
    subject!(
        "subject_statistics_general", "Statistics — General", "statistics", "Statistics", Discipline,
        "Statistical methodology spanning several areas or not covered by a narrower statistics specialist.",
        "statistics is only an application tool and another substantive discipline owns the contribution.", STATISTICS,
        "Evaluate the statistical target, sampling model, information structure, uncertainty, robustness, and inferential scope. Use this fallback when the paper's contribution is statistical but no listed subfield supplies a clearly better lens."
    ),
    subject!(
        "subject_statistics_theory", "Statistics — Theory & Asymptotics", "statistics", "Statistics", Subfield,
        "Decision theory, minimax analysis, asymptotic theory, nonparametrics, or foundational statistical methodology.",
        "the paper primarily applies established theory to one empirical domain.", STATISTICS,
        "Focus on the experiment or model class, loss and risk, asymptotic sequence, uniformity, lower and upper bounds, adaptivity, efficiency, and finite-sample relevance. Check whether assumptions and rates are comparable to the correct theoretical frontier."
    ),
    subject!(
        "subject_statistics_bayesian", "Statistics — Bayesian", "statistics", "Statistics", Subfield,
        "Bayesian modeling, posterior theory, probabilistic programming, prior construction, or Bayesian computation.",
        "Bayesian software is used routinely but no Bayesian modeling or inferential contribution is made.", STATISTICS,
        "Review the likelihood-prior relationship, prior support and sensitivity, identifiability, posterior concentration or calibration, computational approximation, diagnostics, and decision interpretation. Distinguish posterior precision from information supplied by the data."
    ),
    subject!(
        "subject_statistics_causal_semiparametric", "Statistics — Causal & Semiparametric", "statistics", "Statistics", Subfield,
        "Causal estimands, semiparametric efficiency, missing data, treatment effects, or robust causal methodology as the contribution.",
        "causal identification is only an application and the methodological contribution is not statistical.", STATISTICS,
        "Examine the target functional, observed-data model, tangent space or nuisance structure, identification, positivity, robustness, efficiency, and behavior under nuisance estimation. Check whether formal guarantees match the estimator and data-adaptive implementation actually used."
    ),
    subject!(
        "subject_statistics_highdim_learning", "Statistics — High-Dimensional & Learning", "statistics", "Statistics", Subfield,
        "High-dimensional inference, sparsity, statistical learning theory, prediction, or modern nonparametrics.",
        "the central contribution is a computer-science algorithm or application benchmark rather than statistical understanding.", STATISTICS,
        "Assess dimension and sample regimes, structural assumptions, complexity control, regularization, tuning, uncertainty after selection, distribution shift, and the gap between predictive risk and inferential claims. Compare guarantees and experiments to appropriate statistical baselines."
    ),
    subject!(
        "subject_statistics_time_series", "Statistics — Time Series", "statistics", "Statistics", Subfield,
        "Temporal dependence, forecasting, state-space models, longitudinal stochastic processes, or frequency-domain methods.",
        "time appears only as a fixed covariate or panel index with no temporal methodological issue.", STATISTICS,
        "Review stationarity or nonstationarity, dependence and memory, structural breaks, filtering, forecast horizon, temporal leakage, uncertainty propagation, and evaluation design. Check whether asymptotics and validation respect the observed temporal structure."
    ),
    subject!(
        "subject_statistics_spatial", "Statistics — Spatial", "statistics", "Statistics", Subfield,
        "Spatial statistics, point processes, geostatistics, spatial fields, or areal data methodology.",
        "space is merely a location label and the paper has no spatial model or spatial inferential contribution.", STATISTICS,
        "Examine support and resolution, spatial dependence, anisotropy, edge effects, preferential sampling, change of support, neighborhood structure, and prediction uncertainty. Distinguish spatial interpolation from causal or process claims."
    ),
    subject!(
        "subject_statistics_survival_biostat", "Statistics — Survival & Biostatistics", "statistics", "Statistics", Subfield,
        "Survival, event-history, competing-risk, longitudinal biomedical, diagnostic, or clinical statistical methodology.",
        "clinical substance dominates and the statistical methods are entirely standard.", STATISTICS,
        "Focus on censoring and truncation, competing events, time-varying risk, estimand definition, missingness, multiplicity, calibration, and clinical interpretability. Check whether endpoint construction and follow-up support the claimed patient-level inference."
    ),
    subject!(
        "subject_statistics_design_sampling", "Statistics — Design & Sampling", "statistics", "Statistics", Subfield,
        "Experimental design, adaptive design, survey sampling, randomization theory, or finite-population inference.",
        "the design is an application detail and no design or sampling methodology is contributed.", STATISTICS,
        "Review the assignment or sampling mechanism, inclusion probabilities, balance, interference, adaptivity, stopping, weighting, design-based estimand, and variance calculation. Determine whether the proposed design identifies the target under its operational constraints."
    ),
    subject!(
        "subject_statistics_psychometrics_computational", "Statistics — Latent Variables & Computation", "statistics", "Statistics", Subfield,
        "Psychometrics, latent-variable models, item response, mixture models, or statistical computation as a methodological contribution.",
        "latent constructs or computation are routine components of a primarily substantive application.", STATISTICS,
        "Assess measurement invariance, latent-scale identification, mixture separation, score interpretation, computational convergence, Monte Carlo error, and sensitivity to model structure. Check whether the latent object is empirically distinguished from alternative parameterizations."
    ),

    // Computer science
    subject!(
        "subject_computer_science_general", "Computer Science — General", "computer_science", "Computer Science", Discipline,
        "Computer-science research spanning several areas or outside the listed subfields.",
        "a listed computer-science subfield clearly owns the central artifact or theorem.", COMPUTER_SCIENCE,
        "Evaluate the computational problem, abstraction, novelty, correctness criterion, resource model, baseline, and evidence connecting implementation to the claimed general result. Use this fallback only for genuinely cross-cutting computer-science contributions."
    ),
    subject!(
        "subject_computer_science_algorithms", "Computer Science — Algorithms & Complexity", "computer_science", "Computer Science", Subfield,
        "Algorithms, data structures, complexity theory, approximation, online algorithms, or theoretical computer science.",
        "the algorithm is a routine implementation device for an applied system or statistical model.", COMPUTER_SCIENCE,
        "Review the computational model, input class, correctness, asymptotic and parameterized bounds, lower bounds, approximation notion, adversarial assumptions, and benchmark instances. Check whether the stated improvement is meaningful in the relevant regime."
    ),
    subject!(
        "subject_computer_science_ai_ml", "Computer Science — AI & Machine Learning", "computer_science", "Computer Science", Subfield,
        "Machine learning, artificial intelligence, reinforcement learning, generative models, or learning systems as the primary contribution.",
        "standard machine learning is used only to measure a substantive phenomenon in another field.", COMPUTER_SCIENCE,
        "Assess task formulation, training signal, architecture or learning contribution, generalization regime, distribution shift, ablations, compute and data comparisons, benchmark contamination, failure analysis, and reproducibility. Separate empirical scale effects from algorithmic insight."
    ),
    subject!(
        "subject_computer_science_nlp", "Computer Science — Natural-Language Processing", "computer_science", "Computer Science", Subfield,
        "Natural-language processing, computational linguistics systems, language models, or text generation and evaluation.",
        "texts are analyzed as social or historical evidence and no NLP method is contributed.", COMPUTER_SCIENCE,
        "Review linguistic task validity, annotation, language and domain coverage, tokenization and data provenance, evaluation metrics, human evaluation, contamination, multilingual claims, and error categories. Check whether benchmark gains correspond to the claimed language capability."
    ),
    subject!(
        "subject_computer_science_vision_graphics", "Computer Science — Vision & Graphics", "computer_science", "Computer Science", Subfield,
        "Computer vision, image/video understanding, computer graphics, rendering, or visual computing.",
        "imaging is chiefly a scientific measurement instrument or clinical diagnostic rather than a visual-computing contribution.", COMPUTER_SCIENCE,
        "Assess the image formation or rendering model, dataset and scene coverage, geometric assumptions, perceptual metric, temporal consistency, occlusion and lighting robustness, baselines, and qualitative failure modes. Check whether evaluation supports real-world or photorealism claims."
    ),
    subject!(
        "subject_computer_science_systems", "Computer Science — Systems & Networks", "computer_science", "Computer Science", Subfield,
        "Operating, distributed, cloud, storage, networking, mobile, or high-performance systems.",
        "the contribution is a hardware circuit or an application whose systems layer is conventional.", COMPUTER_SCIENCE,
        "Review the workload and threat model, architecture, consistency and failure assumptions, concurrency, scalability, tail behavior, resource accounting, deployability, and comparison to production-relevant baselines. Check whether microbenchmarks support end-to-end claims."
    ),
    subject!(
        "subject_computer_science_programming_languages", "Computer Science — Programming Languages & Formal Methods", "computer_science", "Computer Science", Subfield,
        "Programming languages, type systems, compilers, semantics, program verification, or formal methods.",
        "formal proof concerns a mathematical theorem rather than a programming-language or software property.", COMPUTER_SCIENCE,
        "Examine the language or system model, soundness and completeness claims, expressiveness, metatheory, trusted computing base, mechanization, compiler correctness, usability, and representative programs. Distinguish formal guarantees from properties of the deployed implementation."
    ),
    subject!(
        "subject_computer_science_databases", "Computer Science — Databases & Data Management", "computer_science", "Computer Science", Subfield,
        "Databases, query processing, transactions, data integration, knowledge bases, or data-management systems.",
        "a dataset is created for scientific analysis but no data-management contribution is made.", COMPUTER_SCIENCE,
        "Review the data and query model, correctness semantics, transaction or consistency guarantees, indexing and optimization, skew and scale, update behavior, workload representativeness, and system comparison. Check whether claimed generality survives heterogeneous data and operational constraints."
    ),
    subject!(
        "subject_computer_science_security", "Computer Science — Security & Privacy", "computer_science", "Computer Science", Subfield,
        "Computer security, cryptography applications, privacy, adversarial robustness, or usable security.",
        "security appears only as motivation and no threat, attack, defense, or privacy claim is evaluated.", COMPUTER_SCIENCE,
        "Assess the threat and trust model, attacker capabilities, security definition, attack surface, leakage, composition, deployment assumptions, adaptive behavior, and evaluation against realistic attacks. Check whether privacy or robustness guarantees cover the actual release and workflow."
    ),
    subject!(
        "subject_computer_science_hci", "Computer Science — Human-Computer Interaction", "computer_science", "Computer Science", Subfield,
        "Human-computer interaction, CSCW, information visualization, accessibility, or interactive-system research.",
        "humans appear only as annotators or users of a system whose contribution is otherwise algorithmic.", COMPUTER_SCIENCE,
        "Review the human task and context, participant population, interaction design, comparison condition, construct validity, learning and novelty effects, qualitative analysis, accessibility, and connection from study outcomes to design claims. Distinguish preference from performance and durable use."
    ),
    subject!(
        "subject_computer_science_robotics", "Computer Science — Robotics & Autonomous Systems", "computer_science", "Computer Science", Subfield,
        "Robotics, autonomous agents, planning, control software, embodied AI, or multi-agent systems.",
        "the primary contribution is mechanical hardware or control theory without a computational autonomy claim.", COMPUTER_SCIENCE,
        "Assess sensing, state estimation, planning, control integration, environment assumptions, sim-to-real transfer, safety, recovery, embodiment, and evaluation across tasks and disturbances. Check whether autonomy claims survive perception and actuation failures."
    ),
    subject!(
        "subject_computer_science_software", "Computer Science — Software Engineering", "computer_science", "Computer Science", Subfield,
        "Software engineering, testing, debugging, program analysis, development tools, repositories, or empirical software research.",
        "custom research code is merely an implementation artifact and no software-engineering claim is made.", COMPUTER_SCIENCE,
        "Review the software population, defect or maintenance construct, tool assumptions, benchmark leakage, oracle quality, developer workflow, ecological validity, and comparison to realistic baselines. Check whether repository evidence supports claims about general software practice."
    ),

    // Engineering
    subject!(
        "subject_engineering_general", "Engineering — General", "engineering", "Engineering", Discipline,
        "Engineering research spanning several systems or outside the listed engineering subfields.",
        "a listed engineering specialty clearly owns the design and performance claim.", ENGINEERING,
        "Evaluate requirements, design choices, governing constraints, verification, manufacturability, reliability, scale, and whether measured performance answers the stated engineering problem. Use this fallback only for genuinely cross-disciplinary engineered systems."
    ),
    subject!(
        "subject_engineering_electrical_computer", "Engineering — Electrical & Computer", "engineering", "Engineering", Subfield,
        "Circuits, electronics, communications, signal processing, embedded systems, or computer hardware.",
        "the contribution is primarily a computer-science system or a physical-material mechanism.", ENGINEERING,
        "Review signal and noise models, circuit or architecture constraints, power, bandwidth, timing, quantization, channel conditions, hardware-software interfaces, fabrication assumptions, and comparisons at matched operating points. Check whether component results support system-level claims."
    ),
    subject!(
        "subject_engineering_mechanical", "Engineering — Mechanical", "engineering", "Engineering", Subfield,
        "Mechanical design, mechanics, thermofluids, heat transfer, tribology, or mechanical systems.",
        "the main result is fundamental fluid physics or materials science without an engineered design claim.", ENGINEERING,
        "Assess loads, constitutive and thermal assumptions, geometry, boundary conditions, fatigue and wear, tolerances, efficiency, failure modes, and prototype similarity to the intended regime. Check whether performance trades off against weight, cost, durability, or manufacturability as claimed."
    ),
    subject!(
        "subject_engineering_aerospace", "Engineering — Aerospace", "engineering", "Engineering", Subfield,
        "Aeronautical, astronautical, propulsion, flight, spacecraft, or aerospace-systems research.",
        "the work concerns atmospheric or plasma physics with no vehicle or mission design objective.", ENGINEERING,
        "Review flight or mission envelope, aerodynamic and structural coupling, propulsion assumptions, guidance and control, mass and energy budgets, environmental extremes, qualification, and system margins. Check whether tests or simulations represent the relevant scale and operating conditions."
    ),
    subject!(
        "subject_engineering_civil", "Engineering — Civil, Structural & Transportation", "engineering", "Engineering", Subfield,
        "Civil, structural, geotechnical, construction, infrastructure, or transportation engineering.",
        "the paper is primarily urban social science, economics, or descriptive geology.", ENGINEERING,
        "Examine site and load assumptions, material behavior, boundary and soil conditions, codes, deterioration, network demand, resilience, life-cycle performance, and safety factors. Determine whether case or laboratory evidence supports deployment across the claimed infrastructure class."
    ),
    subject!(
        "subject_engineering_chemical_process", "Engineering — Chemical & Process", "engineering", "Engineering", Subfield,
        "Chemical engineering, reactors, separations, catalysis processes, process systems, or scale-up.",
        "the central contribution is molecular chemistry without a process or transport claim.", ENGINEERING,
        "Review mass and energy balances, kinetics, transport, phase behavior, residence-time and mixing assumptions, separation efficiency, process control, scale-up, feed variability, and hazards. Check whether laboratory conversion or selectivity implies viable process performance."
    ),
    subject!(
        "subject_engineering_materials", "Engineering — Materials", "engineering", "Engineering", Subfield,
        "Materials engineering, metallurgy, ceramics, polymers, composites, processing, or performance design.",
        "the work is fundamental condensed-matter physics or synthetic chemistry without an engineering property target.", ENGINEERING,
        "Assess processing-structure-property links, composition and microstructure control, defects, anisotropy, aging, environmental stability, mechanical or functional testing, comparators, and manufacturability. Check whether reported properties persist at relevant dimensions and cycling conditions."
    ),
    subject!(
        "subject_engineering_biomedical", "Engineering — Biomedical", "engineering", "Engineering", Subfield,
        "Medical devices, biomaterials, tissue engineering, biosensors, biomechanics, or biomedical systems.",
        "the primary claim is clinical efficacy or basic biology rather than an engineered biomedical artifact.", ENGINEERING,
        "Review physiological requirements, biocompatibility, interface and transport, sterilization, calibration, device failure, relevant biological model, translational path, and comparator technology. Separate proof of principle from evidence of safe and robust clinical function."
    ),
    subject!(
        "subject_engineering_environmental_energy", "Engineering — Environmental & Energy", "engineering", "Engineering", Subfield,
        "Environmental engineering, energy conversion and storage, water treatment, emissions control, or sustainable systems.",
        "the paper is climate or environmental science without a treatment, conversion, or systems-design contribution.", ENGINEERING,
        "Assess material and energy balances, contaminant or degradation pathways, efficiency at matched conditions, intermittency, resource inputs, lifetime, waste streams, scale-up, and system boundaries. Check whether environmental benefits survive realistic operation and life-cycle accounting."
    ),
    subject!(
        "subject_engineering_industrial_control", "Engineering — Industrial, Control & Manufacturing", "engineering", "Engineering", Subfield,
        "Industrial engineering, systems engineering, control, operations, manufacturing, automation, or reliability.",
        "the main contribution is abstract optimization or a robot-learning algorithm without an industrial system claim.", ENGINEERING,
        "Review system boundaries, observability and controllability, disturbances, scheduling and capacity, human operations, process variation, quality, reliability, maintainability, and deployment constraints. Check whether optimization or control improvements persist under realistic uncertainty and failure."
    ),

    // Biological sciences
    subject!(
        "subject_biology_general", "Biology — General", "biology", "Biological Science", Discipline,
        "Biological research spanning several levels of organization or outside the listed biological subfields.",
        "a listed biological specialty clearly contains the mechanism and evidence.", BIOLOGY,
        "Evaluate the biological question, organism or system, level of explanation, controls, variation, mechanism, and connection from assay or observation to biological conclusion. Use this fallback only when the contribution genuinely crosses several biological scales."
    ),
    subject!(
        "subject_biology_molecular_cell", "Biology — Molecular & Cell", "biology", "Biological Science", Subfield,
        "Molecular biology, cell biology, cell signaling, organelles, trafficking, or cellular mechanisms.",
        "the primary contribution is organismal physiology, ecology, or a clinical endpoint.", BIOLOGY,
        "Review molecular specificity, perturbation and rescue, localization, dosage and timing, cell-state heterogeneity, pathway alternatives, assay orthogonality, and whether cellular phenotypes establish the proposed mechanism rather than correlation."
    ),
    subject!(
        "subject_biology_genetics_genomics", "Biology — Genetics & Genomics", "biology", "Biological Science", Subfield,
        "Genetics, genomics, epigenomics, population genetics, genome regulation, or functional genomics.",
        "sequence data are only a measurement input and no genetic or genomic claim is central.", BIOLOGY,
        "Assess variant or feature definition, inheritance and population structure, genomic context, multiple testing, batch and mapping artifacts, regulatory interpretation, perturbational validation, and the jump from association to gene or pathway mechanism."
    ),
    subject!(
        "subject_biology_biochemistry_structural", "Biology — Biochemistry & Structural", "biology", "Biological Science", Subfield,
        "Biochemistry, enzymology, metabolism, structural biology, biophysics of macromolecules, or molecular interactions.",
        "the contribution is synthetic chemistry or materials characterization rather than biological molecular function.", BIOLOGY,
        "Review molecular identity and purity, binding and kinetic model, stoichiometry, conformational state, structural resolution, biochemical controls, in-cell relevance, and whether the structure or assay distinguishes the proposed mechanism from alternatives."
    ),
    subject!(
        "subject_biology_development_neuroscience", "Biology — Development & Neuroscience", "biology", "Biological Science", Subfield,
        "Developmental biology, stem cells, neurobiology, neural circuits, or developmental neuroscience.",
        "the main contribution is psychological behavior without a biological developmental or neural mechanism.", BIOLOGY,
        "Examine lineage or circuit identity, developmental timing, spatial organization, perturbation specificity, plasticity, behavioral linkage, model-organism correspondence, and whether cross-sectional or endpoint evidence supports the claimed developmental or neural process."
    ),
    subject!(
        "subject_biology_physiology", "Biology — Physiology", "biology", "Biological Science", Subfield,
        "Organismal, comparative, integrative, endocrine, cardiovascular, respiratory, or metabolic physiology.",
        "the paper is primarily clinical medicine or cell biology without an organism-level functional claim.", BIOLOGY,
        "Review homeostasis, organ-system interaction, dose and temporal response, compensatory mechanisms, sex and life-stage variation, model-organism limits, and whether measured proxies establish the claimed physiological function."
    ),
    subject!(
        "subject_biology_microbiology_immunology", "Biology — Microbiology, Virology & Immunology", "biology", "Biological Science", Subfield,
        "Microbiology, virology, host-pathogen biology, immunology, or microbial communities.",
        "the central claim is population epidemiology without a microbial or immune mechanism.", BIOLOGY,
        "Assess strain and host context, inoculum or exposure, contamination controls, replication competence, immune cell and antigen specificity, temporal response, microbiome compositional versus functional evidence, and whether in vitro or animal findings support the claimed host mechanism."
    ),
    subject!(
        "subject_biology_ecology_evolution", "Biology — Ecology & Evolution", "biology", "Biological Science", Subfield,
        "Ecology, evolutionary biology, behavior, population biology, community ecology, or macroevolution.",
        "the work is environmental monitoring without an ecological or evolutionary inference.", BIOLOGY,
        "Review spatial and temporal scale, sampling of populations and environments, phylogenetic dependence, demographic process, selection versus drift, species interactions, detectability, alternative histories, and whether short-run observations support evolutionary or ecosystem claims."
    ),
    subject!(
        "subject_biology_systems_computational", "Biology — Systems & Computational", "biology", "Biological Science", Subfield,
        "Systems biology, bioinformatics, computational biology, network biology, or multi-omics integration.",
        "the principal contribution is a general computer-science algorithm with biology only as a benchmark.", BIOLOGY,
        "Assess biological target definition, data integration, batch and cohort structure, network or mechanistic assumptions, annotation leakage, validation in independent systems, perturbational support, and whether computational patterns yield a biological explanation."
    ),
    subject!(
        "subject_biology_organismal_behavior", "Biology — Organismal, Integrative & Behavior", "biology", "Biological Science", Subfield,
        "Zoology, organismal biology, comparative anatomy, functional morphology, animal behavior, or integrative biology.",
        "the central contribution is cellular physiology, ecology, or human psychology rather than whole-organism biological function.", BIOLOGY,
        "Review organism and life-stage coverage, anatomy and functional performance, behavioral context, environmental and phylogenetic alternatives, sex and individual variation, captivity or handling effects, and whether observed traits support the claimed adaptive, mechanistic, or comparative conclusion."
    ),
    subject!(
        "subject_biology_plant_marine_conservation", "Biology — Plant, Marine & Conservation", "biology", "Biological Science", Subfield,
        "Plant science, marine biology, conservation biology, biodiversity, or applied organismal ecology.",
        "the central contribution is agricultural engineering or environmental policy rather than organismal biology.", BIOLOGY,
        "Review species and habitat coverage, environmental gradients, life history, dispersal, stress response, intervention feasibility, detection and abundance, temporal baseline, and whether local observations support conservation or ecosystem-wide conclusions."
    ),

    // Chemistry
    subject!(
        "subject_chemistry_general", "Chemistry — General", "chemistry", "Chemistry", Discipline,
        "Chemical research spanning several areas or outside the listed chemistry subfields.",
        "a listed chemistry specialty clearly contains the main transformation, measurement, or molecular claim.", CHEMISTRY,
        "Evaluate molecular identity, mechanism, thermodynamics and kinetics, purity, characterization, controls, reproducibility, and whether the observed property follows from the claimed chemical structure or process."
    ),
    subject!(
        "subject_chemistry_organic_biological", "Chemistry — Organic & Chemical Biology", "chemistry", "Chemistry", Subfield,
        "Organic synthesis, reaction methodology, catalysis, medicinal chemistry, or chemical biology.",
        "the main result is a biological mechanism with standard chemical probes or an industrial process scale-up.", CHEMISTRY,
        "Review substrate scope, selectivity, yields and mass balance, catalyst loading, mechanistic evidence, stereochemistry, compound identity and purity, comparator routes, biological probe specificity, and whether the reaction or molecule works under the claimed conditions."
    ),
    subject!(
        "subject_chemistry_inorganic_materials", "Chemistry — Inorganic & Materials", "chemistry", "Chemistry", Subfield,
        "Inorganic, organometallic, solid-state, coordination, or materials chemistry.",
        "the contribution is primarily device engineering or condensed-matter physics rather than chemical composition and bonding.", CHEMISTRY,
        "Assess composition, oxidation and coordination state, phase purity, defects, synthesis reproducibility, structure-property connection, stability, cycling, surface versus bulk behavior, and whether characterization rules out plausible alternative phases."
    ),
    subject!(
        "subject_chemistry_physical_theoretical", "Chemistry — Physical & Theoretical", "chemistry", "Chemistry", Subfield,
        "Physical chemistry, spectroscopy, chemical dynamics, quantum chemistry, statistical chemistry, or theoretical chemistry.",
        "the contribution is a general physics theory or numerical method without a chemical question.", CHEMISTRY,
        "Review the electronic or molecular model, potential-energy landscape, spectroscopic assignment, kinetic regime, solvent and temperature effects, approximation hierarchy, calibration, and connection between calculated quantities and experimental observables."
    ),
    subject!(
        "subject_chemistry_analytical_environmental", "Chemistry — Analytical & Environmental", "chemistry", "Chemistry", Subfield,
        "Analytical chemistry, separations, sensors, mass spectrometry, electrochemistry, or environmental chemistry.",
        "measurement is routine and the substantive contribution lies entirely in another scientific field.", CHEMISTRY,
        "Assess selectivity, sensitivity, calibration, matrix effects, blanks, recovery, detection limits, reference methods, speciation, transport and degradation, field representativeness, and whether the assay distinguishes the claimed analyte or process."
    ),

    // Earth and environmental sciences
    subject!(
        "subject_earth_environment_general", "Earth & Environment — General", "earth_environment", "Earth & Environmental Science", Discipline,
        "Earth, planetary, or environmental research spanning several systems or outside the listed specialties.",
        "a listed Earth or environmental subfield clearly owns the principal process and evidence.", EARTH_ENVIRONMENT,
        "Evaluate the system boundary, spatial and temporal scale, process model, proxy or instrument, uncertainty, representativeness, and connection from local evidence to regional, planetary, or long-run claims."
    ),
    subject!(
        "subject_earth_environment_geology", "Earth & Environment — Geology & Geochemistry", "earth_environment", "Earth & Environmental Science", Subfield,
        "Geology, geochemistry, geomorphology, paleoclimate proxies, sedimentology, or tectonics.",
        "the central contribution is a physical geophysics inverse problem or modern atmospheric process.", EARTH_ENVIRONMENT,
        "Review stratigraphic and spatial context, chronology, preservation and alteration, proxy calibration, sampling, geochemical mass balance, tectonic interpretation, and whether the record uniquely supports the proposed Earth history."
    ),
    subject!(
        "subject_earth_environment_climate_atmosphere", "Earth & Environment — Climate & Atmosphere", "earth_environment", "Earth & Environmental Science", Subfield,
        "Climate science, meteorology, atmospheric chemistry, weather, or Earth-system dynamics.",
        "the paper concerns policy impacts without a material climate or atmospheric scientific contribution.", EARTH_ENVIRONMENT,
        "Assess forcing and feedback, energy and moisture budgets, circulation, model resolution and tuning, internal variability, observational coverage, attribution, scenario interpretation, and whether projected or reconstructed changes exceed uncertainty and structural dependence."
    ),
    subject!(
        "subject_earth_environment_ocean_hydrology", "Earth & Environment — Ocean & Hydrology", "earth_environment", "Earth & Environmental Science", Subfield,
        "Oceanography, hydrology, cryosphere, limnology, groundwater, or watershed science.",
        "water is only an engineering input or the central mechanism is atmospheric rather than oceanic or hydrologic.", EARTH_ENVIRONMENT,
        "Review circulation or flow, storage and flux closure, mixing, boundary exchanges, sampling depth and season, catchment or basin scale, tracer assumptions, extremes, and whether sparse observations constrain the claimed water-system dynamics."
    ),
    subject!(
        "subject_earth_environment_sustainability", "Earth & Environment — Ecology & Sustainability", "earth_environment", "Earth & Environmental Science", Subfield,
        "Environmental science, biogeochemistry, pollution, ecosystem services, sustainability, or coupled human-natural systems.",
        "the contribution is primarily ecological biology, engineering treatment, or policy evaluation.", EARTH_ENVIRONMENT,
        "Assess system boundaries, stocks and flows, exposure pathways, ecological coupling, baselines, spatial displacement, life-cycle burdens, rebound, intervention durability, and whether environmental indicators support the claimed sustainability outcome."
    ),

    // Medicine and health
    subject!(
        "subject_medicine_health_general", "Medicine & Health — General", "medicine_health", "Medicine & Health", Discipline,
        "Medical, clinical, or health research spanning several specialties or outside the listed health subfields.",
        "a listed clinical, epidemiological, diagnostic, therapeutic, or health-systems lens clearly fits.", MEDICINE_HEALTH,
        "Evaluate the patient or population, clinical problem, comparator, endpoint, harms, follow-up, applicability, and whether the evidence changes diagnosis, prognosis, prevention, or care. Keep biological mechanism distinct from demonstrated patient benefit."
    ),
    subject!(
        "subject_medicine_health_clinical", "Medicine & Health — Clinical", "medicine_health", "Medicine & Health", Subfield,
        "Clinical observational research, prognosis, treatment outcomes, patient management, or specialty medicine.",
        "the contribution is a formal trial, diagnostic technology, or population-health study better covered below.", MEDICINE_HEALTH,
        "Review eligibility, disease definition and severity, treatment pathways, clinical comparators, confounding by indication, endpoint importance, follow-up, competing care, adverse events, and whether results apply to the patients named in the conclusion."
    ),
    subject!(
        "subject_medicine_health_epidemiology", "Medicine & Health — Epidemiology & Public Health", "medicine_health", "Medicine & Health", Subfield,
        "Epidemiology, population health, prevention, infectious-disease spread, environmental health, or health disparities.",
        "the paper is a patient-level clinical efficacy study or a biological transmission mechanism without population inference.", MEDICINE_HEALTH,
        "Assess population and case definition, surveillance and ascertainment, exposure timing, transmission or risk model, selection, competing risks, transport across populations, absolute burden, intervention reach, and whether associations support the public-health recommendation."
    ),
    subject!(
        "subject_medicine_health_trials", "Medicine & Health — Trials & Therapeutics", "medicine_health", "Medicine & Health", Subfield,
        "Clinical trials, therapeutic development, comparative treatment, dosing, or intervention efficacy and safety.",
        "the intervention is nonclinical or the study is purely observational with no trial design.", MEDICINE_HEALTH,
        "Review trial phase and estimand, randomization and masking, comparator, adherence and crossover, endpoint hierarchy, multiplicity, stopping, safety, clinical versus statistical significance, follow-up, and whether efficacy evidence supports the proposed use."
    ),
    subject!(
        "subject_medicine_health_diagnostics", "Medicine & Health — Diagnostics & Imaging", "medicine_health", "Medicine & Health", Subfield,
        "Diagnostic tests, biomarkers, pathology, medical imaging, screening, prognostic models, or clinical decision support.",
        "the main contribution is an imaging algorithm without a clinical diagnostic claim.", MEDICINE_HEALTH,
        "Assess reference standard, spectrum and verification bias, threshold selection, calibration, incremental value, prevalence dependence, reader or site variation, workflow, consequences of false decisions, and external validation in the intended clinical setting."
    ),
    subject!(
        "subject_medicine_health_services", "Medicine & Health — Services & Global Health", "medicine_health", "Medicine & Health", Subfield,
        "Health services, implementation, delivery systems, quality, cost, access, policy, or global health.",
        "the central contribution is a biomedical treatment effect under tightly controlled clinical conditions.", MEDICINE_HEALTH,
        "Review care setting, implementation pathway, provider and patient selection, access barriers, capacity, quality measures, costs and consequences, heterogeneity across institutions, sustainability, and whether system-level evidence supports scale or transfer."
    ),

    // Sociology
    subject!(
        "subject_sociology_general", "Sociology — General", "sociology", "Sociology", Discipline,
        "Sociological research spanning several areas or outside the listed sociological subfields.",
        "a listed sociological subfield clearly contains the principal social process.", SOCIOLOGY,
        "Evaluate the social units, institutions, mechanisms, historical and cultural setting, comparison, evidence, and scope of generalization. Use this fallback only when the argument genuinely spans several sociological traditions."
    ),
    subject!(
        "subject_sociology_inequality_demography", "Sociology — Inequality & Demography", "sociology", "Sociology", Subfield,
        "Social stratification, mobility, inequality, demography, family, life course, or population change.",
        "the central contribution is a labor or public-economics estimate without a sociological account of structure or group process.", SOCIOLOGY,
        "Review the stratification dimensions, population at risk, cohort and life-course timing, family or household structure, mobility concept, institutional sorting, compositional change, and whether observed gaps support the proposed mechanism rather than category construction alone."
    ),
    subject!(
        "subject_sociology_organizations_economic", "Sociology — Organizations & Economic Life", "sociology", "Sociology", Subfield,
        "Organizations, occupations, professions, markets, work, firms, economic sociology, or institutional fields.",
        "the paper models firms or markets without a sociological organizational or relational contribution.", SOCIOLOGY,
        "Assess organizational boundaries, authority and status, networks and fields, institutional pressures, workplace process, actor meaning, market construction, and whether evidence connects micro practices to the claimed organizational or economic outcome."
    ),
    subject!(
        "subject_sociology_political_movements", "Sociology — Political & Social Movements", "sociology", "Sociology", Subfield,
        "Political sociology, states, citizenship, collective action, protest, social movements, or power.",
        "the contribution is primarily electoral behavior, formal institutions, or international relations without a sociological mechanism.", SOCIOLOGY,
        "Review power and state-society relations, mobilizing structures, networks, identities, political opportunities, repression, organizational continuity, event selection, and the connection from collective action to institutional or cultural change."
    ),
    subject!(
        "subject_sociology_race_migration", "Sociology — Race, Ethnicity & Migration", "sociology", "Sociology", Subfield,
        "Race, ethnicity, indigeneity, immigration, citizenship, assimilation, boundaries, or transnational communities.",
        "group categories are incidental controls and no racial, ethnic, migration, or boundary process is central.", SOCIOLOGY,
        "Examine category construction, racialization or boundary making, legal status, selection into migration, generation and place, institutions, discrimination, identity, and whether comparisons distinguish group composition from the claimed social process."
    ),
    subject!(
        "subject_sociology_culture_media", "Sociology — Culture & Media", "sociology", "Sociology", Subfield,
        "Culture, meaning, classification, knowledge, religion, media, consumption, or cultural production.",
        "the paper is primarily textual interpretation without a sociological claim about actors, institutions, or social distribution.", SOCIOLOGY,
        "Review how meanings and classifications are produced, circulated, contested, and linked to actors and institutions; how cultural objects are selected; and whether evidence supports the claimed audience, field, repertoire, or cultural mechanism."
    ),
    subject!(
        "subject_sociology_networks", "Sociology — Social Networks", "sociology", "Sociology", Subfield,
        "Social networks, diffusion, relational inequality, social capital, peer structure, or network organizations.",
        "graphs are purely technological or biological and social relations are not the substantive object.", SOCIOLOGY,
        "Assess node and tie meaning, boundary specification, missing ties, homophily and influence, dependence, temporal ordering, network opportunity, diffusion mechanism, and whether structural measures identify the claimed social relation."
    ),
    subject!(
        "subject_sociology_urban_community", "Sociology — Urban & Community", "sociology", "Sociology", Subfield,
        "Urban sociology, neighborhoods, housing, place, communities, segregation, or local institutions.",
        "the main contribution is urban economics, planning engineering, or geography without a sociological community process.", SOCIOLOGY,
        "Review place and neighborhood definitions, residential selection, institutions, segregation, mobility, local networks, spatial scale, historical development, and whether evidence connects place-based conditions to the claimed social outcomes."
    ),
    subject!(
        "subject_sociology_medical_crime", "Sociology — Health, Medicine & Crime", "sociology", "Sociology", Subfield,
        "Medical sociology, health inequality, professions and care, criminology, punishment, law, or deviance.",
        "clinical efficacy, epidemiology, or legal doctrine is central without a sociological institution or inequality claim.", SOCIOLOGY,
        "Assess institutional definitions of illness or deviance, professional authority, surveillance and punishment, access, stigma, selection, neighborhood and organizational context, and whether evidence supports the claimed social production of health, crime, or control."
    ),

    // Political science
    subject!(
        "subject_political_science_general", "Political Science — General", "political_science", "Political Science", Discipline,
        "Political research spanning several areas or outside the listed political-science subfields.",
        "a listed political-science subfield clearly contains the actors, institution, or outcome.", POLITICAL_SCIENCE,
        "Evaluate the political actors, institution, strategic environment, authority, mechanism, comparison, and scope conditions. Use this fallback for genuinely cross-cutting political questions rather than as a substitute for a clear subfield."
    ),
    subject!(
        "subject_political_science_comparative", "Political Science — Comparative Politics", "political_science", "Political Science", Subfield,
        "Comparative institutions, regimes, democratization, parties, state capacity, governance, or political development.",
        "the paper concerns international interactions or one policy without comparative institutional inference.", POLITICAL_SCIENCE,
        "Review case and regime comparability, institutional variation, state and party organization, historical sequence, actor incentives, selection of cases, alternative political explanations, and whether the comparison supports the claimed general political mechanism."
    ),
    subject!(
        "subject_political_science_ir_security", "Political Science — International Relations & Security", "political_science", "Political Science", Subfield,
        "International relations, security, war, alliances, diplomacy, international organizations, or foreign policy.",
        "cross-border economic exchange is central but strategic international politics is not.", POLITICAL_SCIENCE,
        "Assess actors and levels of analysis, strategic interaction, information and commitment, threat and capability measures, alliance or institutional constraints, conflict selection, temporal sequence, and whether evidence distinguishes the proposed international mechanism."
    ),
    subject!(
        "subject_political_science_ipe", "Political Science — International Political Economy", "political_science", "Political Science", Subfield,
        "Trade politics, international finance, sanctions, development institutions, globalization, or cross-border political economy.",
        "the contribution is a purely economic trade or finance result without a political institution or distributional mechanism.", POLITICAL_SCIENCE,
        "Review domestic and international actors, distributional coalitions, institutions, bargaining, capital and trade exposure, policy endogeneity, cross-border spillovers, and whether economic measures identify the political mechanism claimed."
    ),
    subject!(
        "subject_political_science_institutions", "Political Science — Institutions & Governance", "political_science", "Political Science", Subfield,
        "Legislatures, executives, courts, bureaucracy, federalism, constitutions, corruption, or governance.",
        "the central contribution is legal doctrine or organizational sociology without a political institutional claim.", POLITICAL_SCIENCE,
        "Examine formal and informal rules, actor authority and incentives, agenda control, implementation, enforcement, institutional equilibrium, strategic adaptation, and whether observed outcomes reveal institutional effects rather than selection into rules."
    ),
    subject!(
        "subject_political_science_behavior_elections", "Political Science — Behavior, Opinion & Elections", "political_science", "Political Science", Subfield,
        "Political behavior, public opinion, voting, campaigns, parties, representation, or political communication.",
        "the paper studies general social attitudes without a material political behavior or representation claim.", POLITICAL_SCIENCE,
        "Review the electorate or audience, attitude and preference measurement, information environment, turnout and choice set, campaign exposure, partisan sorting, representation link, and whether survey or electoral evidence supports the claimed behavioral mechanism."
    ),
    subject!(
        "subject_political_science_theory", "Political Science — Political Theory", "political_science", "Political Science", Subfield,
        "Normative, analytic, historical, or critical political theory concerning justice, authority, democracy, liberty, or power.",
        "the central contribution is empirical political behavior or general moral philosophy without a political institutional object.", POLITICAL_SCIENCE,
        "Assess the normative problem, conceptual distinctions, institutional setting, argumentative premises, treatment of objections, feasibility and idealization, relation to canonical positions, and whether prescriptions follow at the level of political authority claimed."
    ),
    subject!(
        "subject_political_science_policy_admin", "Political Science — Public Policy & Administration", "political_science", "Political Science", Subfield,
        "Policy design, implementation, bureaucracy, regulation, public administration, or program governance.",
        "the paper estimates a program effect without a material claim about political design, administration, or implementation.", POLITICAL_SCIENCE,
        "Review policy authority, target population, administrative capacity, implementation chain, discretion, compliance, feedback, distribution, political feasibility, and whether observed program outcomes support the broader policy or governance conclusion."
    ),
    subject!(
        "subject_political_science_conflict_environment", "Political Science — Conflict & Environmental Politics", "political_science", "Political Science", Subfield,
        "Civil conflict, peacebuilding, repression, political violence, resource politics, or environmental governance.",
        "the paper is chiefly climate science, criminology, or international war without the relevant domestic conflict or governance mechanism.", POLITICAL_SCIENCE,
        "Assess actors, territorial and resource stakes, violence measurement, conflict onset and duration, selection into exposure, intervention and enforcement, post-conflict institutions, environmental distribution, and whether evidence supports the proposed political pathway."
    ),

    // History
    subject!(
        "subject_history_general", "History — General", "history", "History", Discipline,
        "Historical scholarship spanning several periods or themes or outside the listed historical subfields.",
        "a listed period or thematic history specialist clearly fits the main intervention.", HISTORY,
        "Evaluate chronology, context, source base, historiographic intervention, contingency, continuity and change, and the scale at which the claim is made. Use this fallback only when the contribution genuinely crosses periods and thematic literatures."
    ),
    subject!(
        "subject_history_ancient", "History — Ancient", "history", "History", Subfield,
        "Ancient Mediterranean, Near Eastern, African, Asian, American, or other pre-medieval history.",
        "the central evidence and historiography belong to a later period.", HISTORY,
        "Review chronology and geography, linguistic and archaeological evidence, textual transmission, anachronism, institutional categories, source survival, and whether fragmentary evidence supports the breadth and precision of the ancient-historical claim."
    ),
    subject!(
        "subject_history_medieval", "History — Medieval", "history", "History", Subfield,
        "Medieval history across regions, including institutions, religion, economy, society, and material culture.",
        "the paper is primarily ancient or early-modern and does not turn on medieval periodization.", HISTORY,
        "Assess periodization, manuscript and material sources, ecclesiastical and political institutions, local variation, translation and terminology, chronology, and whether later categories are projected onto medieval actors or structures."
    ),
    subject!(
        "subject_history_early_modern", "History — Early Modern", "history", "History", Subfield,
        "Early-modern state formation, empire, religion, science, commerce, culture, or social change.",
        "the central intervention belongs clearly to medieval or modern historiography.", HISTORY,
        "Review the paper's period boundary, imperial and confessional setting, print and knowledge networks, commercial and state institutions, source geography, and whether the evidence supports claimed transitions rather than assuming modernization."
    ),
    subject!(
        "subject_history_modern", "History — Modern & Contemporary", "history", "History", Subfield,
        "Modern or contemporary history, including industrialization, nation-states, colonialism, war, and mass politics.",
        "the paper's intervention is primarily a social-science analysis of current outcomes rather than historical explanation.", HISTORY,
        "Assess chronology, archival access and silences, state and mass institutions, colonial and global connections, memory and retrospective sources, causal sequence, and whether the narrative distinguishes contemporary categories from actors' own terms."
    ),
    subject!(
        "subject_history_economic", "History — Economic & Business", "history", "History", Subfield,
        "Economic, business, labor, financial, technological, or quantitative history.",
        "the contribution is principally an economics estimate with little historiographic or source-based historical argument.", HISTORY,
        "Review historical measurement and comparability, institutional and technological context, firm and labor records, price or monetary units, selection in surviving sources, periodization, and whether quantitative patterns sustain the claimed historical mechanism."
    ),
    subject!(
        "subject_history_political_diplomatic", "History — Political, Diplomatic & Military", "history", "History", Subfield,
        "Political, diplomatic, legal-institutional, military, state, or international history.",
        "the paper is an international-relations model using historical cases without a primary historical intervention.", HISTORY,
        "Assess state and nonstate actors, decision sequence, institutional authority, diplomatic and military sources, strategic retrospective bias, contingency, multiple theaters or levels, and whether elite archives are sufficient for the broader political claim."
    ),
    subject!(
        "subject_history_social_cultural", "History — Social, Cultural & Gender", "history", "History", Subfield,
        "Social, cultural, gender, race, family, everyday-life, or subaltern history.",
        "culture or inequality is analyzed without a historical source base or historiographic contribution.", HISTORY,
        "Review whose experience the sources preserve, category and identity formation, material and institutional context, representativeness, reading against archival silences, scale from individual cases to groups, and the chronology of cultural or social change."
    ),
    subject!(
        "subject_history_intellectual_global", "History — Intellectual, Global & Environmental", "history", "History", Subfield,
        "Intellectual, religious, science, medicine, global, transnational, colonial, or environmental history.",
        "the paper is purely philosophical, literary, or environmental-scientific without a historical intervention.", HISTORY,
        "Assess concepts in their historical language, circulation and translation, connected archives, asymmetries of empire, scale across regions, knowledge and environmental context, reception, and whether transnational linkage is demonstrated rather than inferred from parallel developments."
    ),

    // Economics
    subject!(
        "subject_economics_general", "Economics — General", "economics", "Economics", Discipline,
        "Economic research spanning several fields or outside the listed economics subfields.",
        "a listed economics field clearly contains the model, data, and contribution.", ECONOMICS,
        "Evaluate the economic object, agents and constraints, equilibrium or empirical margin, incidence, welfare, and connection from evidence to the claimed mechanism. Use this fallback only for genuinely cross-field economic research."
    ),
    subject!(
        "subject_economics_macro", "Economics — Macroeconomics", "economics", "Economics", Subfield,
        "Macroeconomics, monetary or fiscal policy, growth, business cycles, labor macro, or aggregate dynamics.",
        "the paper is household or firm microeconomics without an aggregate equilibrium or macro policy claim.", ECONOMICS,
        "Examine the aggregate mechanism, equilibrium closure, expectations, aggregation, transition versus steady state, policy experiment, calibration targets, incidence, and whether the claimed general-equilibrium channel is separated from accounting effects."
    ),
    subject!(
        "subject_economics_micro_theory", "Economics — Microeconomic Theory", "economics", "Economics", Subfield,
        "Microeconomic theory, games, information, contracts, mechanism design, matching, networks, or market design.",
        "the formal result is mathematical but has no material economic incentives, allocation, or welfare content.", ECONOMICS,
        "Assess primitives, information and timing, equilibrium concept, incentives, off-path behavior, implementability, comparative statics, robustness to strategic alternatives, and the economic content and scope of welfare conclusions."
    ),
    subject!(
        "subject_economics_econometrics", "Economics — Econometrics", "economics", "Economics", Subfield,
        "Econometric theory or methodology is itself the main contribution.",
        "established econometric tools are applied without a methodological contribution.", ECONOMICS,
        "Review the estimand and statistical experiment, identifying and regularity assumptions, asymptotic sequence, robustness, efficiency, finite-sample behavior, implementation, and whether empirical illustrations exercise the method's difficult cases."
    ),
    subject!(
        "subject_economics_behavioral_experimental", "Economics — Behavioral & Experimental", "economics", "Economics", Subfield,
        "Behavioral economics, experimental economics, decision theory with behavioral content, or field and laboratory evidence on economic choice.",
        "the contribution is general psychology without an economic choice, incentive, market, or welfare object.", ECONOMICS,
        "Assess the economic choice environment, incentives, information, elicitation, equilibrium or strategic context, behavioral construct, treatment contrast, demand and experimenter effects, heterogeneity, external validity, and whether the evidence distinguishes the proposed departure from standard economic benchmarks."
    ),
    subject!(
        "subject_economics_public_labor", "Economics — Public, Labor & Education", "economics", "Economics", Subfield,
        "Public finance, taxation, social insurance, labor, inequality, education, family, or personnel economics.",
        "the principal contribution is sociological stratification or education practice without economic behavior or policy incidence.", ECONOMICS,
        "Assess the behavioral margin, institutional and policy rules, labor or household selection, incidence, sufficient statistics or model channel, distributional consequences, equilibrium responses, and whether the counterfactual maps to a feasible policy."
    ),
    subject!(
        "subject_economics_development_trade", "Economics — Development & International", "economics", "Economics", Subfield,
        "Development, international trade, migration, political economy of development, or cross-country economic change.",
        "international politics or historical narrative is central without an economic allocation or development mechanism.", ECONOMICS,
        "Review institutions and market context, selection across locations, prices and quantities, household and firm margins, spillovers and equilibrium, external validity, trade or migration incidence, and whether policy conclusions respect implementation capacity."
    ),
    subject!(
        "subject_economics_io", "Economics — Industrial Organization", "economics", "Economics", Subfield,
        "Industrial organization, demand, firm conduct, market power, entry, platforms, auctions, or competition policy.",
        "the main contribution is a management strategy or computer platform system without market equilibrium analysis.", ECONOMICS,
        "Assess market definition, demand and substitution, firm information and conduct, entry and dynamics, equilibrium, identification of markups or primitives, counterfactual policy, pass-through, and welfare allocation across consumers and firms."
    ),
    subject!(
        "subject_economics_finance", "Economics — Finance", "economics", "Economics", Subfield,
        "Asset pricing, corporate finance, banking, household finance, intermediaries, or market microstructure.",
        "the contribution is accounting description or business valuation without a finance mechanism or asset-market claim.", ECONOMICS,
        "Review risk and information, pricing kernel or corporate objective, financing and intermediary constraints, selection, equilibrium and no-arbitrage discipline, horizon, return measurement, institutional detail, and whether evidence distinguishes the proposed financial channel."
    ),
    subject!(
        "subject_economics_health_urban_environment", "Economics — Health, Urban & Environmental", "economics", "Economics", Subfield,
        "Health economics, urban and regional economics, transportation, housing, environmental or energy economics.",
        "the contribution is clinical, engineering, geographic, or environmental science without an economic behavior or welfare object.", ECONOMICS,
        "Assess prices, access and sorting, spatial or health externalities, household and firm location or care choices, policy incidence, capitalization, congestion, equilibrium adjustment, distribution, and whether welfare conclusions match the estimated margin."
    ),
    subject!(
        "subject_economics_history", "Economics — Economic History", "economics", "Economics", Subfield,
        "Economic history using economic theory or empirical methods to explain historical development.",
        "the contribution is primarily historiographic and source-interpretive without an economic mechanism or estimand.", ECONOMICS,
        "Review historical institutions and measurement, unit comparability, selection in records, timing, mechanism and counterfactual, persistence, spatial and cohort composition, and whether modern economic concepts are warranted by the historical setting."
    ),

    // Psychology and cognitive science
    subject!(
        "subject_psychology_general", "Psychology — General", "psychology", "Psychology & Cognitive Science", Discipline,
        "Psychological or cognitive research spanning several areas or outside the listed specialties.",
        "a listed psychological subfield clearly contains the construct and evidence.", PSYCHOLOGY,
        "Evaluate the psychological construct, task and operationalization, population, context, comparison, alternative cognitive or social process, and whether measured behavior supports the level of mental explanation claimed."
    ),
    subject!(
        "subject_psychology_cognitive", "Psychology — Cognitive", "psychology", "Psychology & Cognitive Science", Subfield,
        "Cognition, perception, memory, attention, language, learning, or computational cognition.",
        "the primary contribution is a neural mechanism, social process, or NLP system rather than cognition.", PSYCHOLOGY,
        "Review the cognitive process and task decomposition, stimulus control, speed-accuracy tradeoffs, learning and strategy, model identifiability, individual variation, and whether behavioral signatures distinguish the proposed representation or process."
    ),
    subject!(
        "subject_psychology_social_personality", "Psychology — Social & Personality", "psychology", "Psychology & Cognitive Science", Subfield,
        "Social psychology, personality, attitudes, identity, interpersonal behavior, judgment, or decision-making.",
        "the main contribution is sociological institutions or political behavior at a collective level.", PSYCHOLOGY,
        "Assess construct validity, situational and dispositional alternatives, demand and expectancy, sampling, cultural context, behavior versus self-report, multiple operationalizations, and whether effects support the claimed general social or personality process."
    ),
    subject!(
        "subject_psychology_development_clinical", "Psychology — Developmental & Clinical", "psychology", "Psychology & Cognitive Science", Subfield,
        "Developmental, educational, clinical, health, or psychopathology research focused on psychological processes.",
        "the central object is clinical treatment efficacy or biological development rather than psychological theory.", PSYCHOLOGY,
        "Review age and developmental timing, cohort and caregiver context, diagnostic and symptom definition, comorbidity, impairment, measurement invariance, attrition, normative comparison, and whether trajectories or interventions establish the claimed developmental or clinical mechanism."
    ),
    subject!(
        "subject_psychology_behavioral_neuroscience", "Psychology — Behavioral Neuroscience", "psychology", "Psychology & Cognitive Science", Subfield,
        "Behavioral neuroscience, cognitive neuroscience, neuropsychology, psychophysiology, or brain-behavior research.",
        "the central contribution is cellular neuroscience or medical imaging diagnosis rather than psychological function.", PSYCHOLOGY,
        "Assess localization and network claims, temporal resolution, task and contrast logic, reverse inference, preprocessing and multiplicity, lesion or perturbation evidence, behavior-brain linkage, and whether neural measures add explanatory content beyond correlated activation."
    ),
    subject!(
        "subject_psychology_industrial_human_factors", "Psychology — Industrial, Organizational & Human Factors", "psychology", "Psychology & Cognitive Science", Subfield,
        "Work psychology, personnel selection, teams, leadership, occupational behavior, ergonomics, or human factors.",
        "the main contribution is management strategy, organizational sociology, or interface design without a psychological construct or human-performance claim.", PSYCHOLOGY,
        "Review the worker or operator population, task and organizational context, construct and criterion validity, selection and range restriction, common-method bias, team and leadership levels, fatigue and workload, safety-relevant performance, and whether laboratory or survey evidence supports behavior in the intended workplace or system."
    ),

    // Anthropology and archaeology
    subject!(
        "subject_anthropology_general", "Anthropology — General", "anthropology", "Anthropology & Archaeology", Discipline,
        "Anthropological or archaeological scholarship spanning several traditions or outside the listed specialties.",
        "a sociocultural, biological-linguistic, or archaeological lens clearly fits.", ANTHROPOLOGY,
        "Evaluate the cultural or material setting, researcher position, categories in local terms, comparison, historical depth, evidence, and movement between situated observation and broader anthropological claim."
    ),
    subject!(
        "subject_anthropology_sociocultural", "Anthropology — Sociocultural", "anthropology", "Anthropology & Archaeology", Subfield,
        "Ethnographic, sociocultural, medical, political, economic, or linguistic-cultural anthropology.",
        "the paper is primarily survey sociology or textual humanities without ethnographic or anthropological comparison.", ANTHROPOLOGY,
        "Review field relation and access, emic and analytic categories, translation, reflexivity, variation within the field site, temporal and political context, evidentiary path from scenes and interviews to interpretation, and comparative scope."
    ),
    subject!(
        "subject_anthropology_biological_linguistic", "Anthropology — Biological & Linguistic", "anthropology", "Anthropology & Archaeology", Subfield,
        "Biological anthropology, human evolution, primatology, linguistic anthropology, or language and culture.",
        "the main result is general evolutionary biology or formal linguistics without an anthropological human context.", ANTHROPOLOGY,
        "Assess population and comparative sample, ancestry and environment, evolutionary interpretation, morphology or behavior, language practice and ideology, community variation, ethical provenance, and whether biological or linguistic measures support the human historical claim."
    ),
    subject!(
        "subject_anthropology_archaeology", "Anthropology — Archaeology & Material Culture", "anthropology", "Anthropology & Archaeology", Subfield,
        "Archaeology, material culture, heritage, bioarchaeology, or archaeological science.",
        "material objects are chiefly art-historical texts or geological specimens without archaeological context.", ANTHROPOLOGY,
        "Review provenience, chronology and dating, formation and preservation, sampling, typology, material analysis, site and regional context, analogy, heritage ethics, and whether fragmentary remains support claims about past practice or social organization."
    ),

    // Geography
    subject!(
        "subject_geography_general", "Geography — General", "geography", "Geography", Discipline,
        "Geographic research spanning human, physical, and geospatial approaches or outside the listed specialties.",
        "a listed geographic subfield clearly contains the spatial process.", GEOGRAPHY,
        "Evaluate spatial scale, place and region, boundaries, mobility and connection, representation, unevenness, and whether the spatial evidence supports movement from a particular location to the claimed geographic process."
    ),
    subject!(
        "subject_geography_human_urban", "Geography — Human, Economic & Urban", "geography", "Geography", Subfield,
        "Human, economic, political, urban, cultural, or development geography.",
        "the contribution is primarily sociology, economics, or political science without a geographic account of space and place.", GEOGRAPHY,
        "Review production of place and scale, spatial division and mobility, networks and territory, uneven development, mapping and representation, local history, actor power, and whether comparisons preserve the geographic context of the mechanism."
    ),
    subject!(
        "subject_geography_physical", "Geography — Physical & Environmental", "geography", "Geography", Subfield,
        "Physical geography, biogeography, geomorphology, landscape, human-environment, or environmental change.",
        "the main contribution is a narrow Earth-system mechanism without a geographic landscape or spatial synthesis.", GEOGRAPHY,
        "Assess landscape and process scale, spatial heterogeneity, field sampling, land cover and history, coupled human influence, temporal baseline, regionalization, and whether local measurements identify the claimed landscape dynamics."
    ),
    subject!(
        "subject_geography_gis", "Geography — GIS & Remote Sensing", "geography", "Geography", Subfield,
        "Geographic information science, cartography, spatial data infrastructure, remote sensing, or geocomputation.",
        "GIS is a routine tool and no geographic measurement, representation, or spatial-method contribution is made.", GEOGRAPHY,
        "Review spatial ontology, scale and resolution, projection, positional error, classification and validation, change detection, map communication, interoperability, and whether the geographic representation is fit for the substantive inference."
    ),

    // Philosophy
    subject!(
        "subject_philosophy_general", "Philosophy — General", "philosophy", "Philosophy", Discipline,
        "Philosophical work spanning several areas or outside the listed philosophical specialties.",
        "a listed philosophical area clearly contains the principal argument.", PHILOSOPHY,
        "Evaluate the central question, concepts, premises, argumentative validity, dialectical targets, counterexamples, objections, and the exact strength and modality of the conclusion. Use this fallback only for genuinely cross-area philosophy."
    ),
    subject!(
        "subject_philosophy_logic_epistemology", "Philosophy — Logic & Epistemology", "philosophy", "Philosophy", Subfield,
        "Philosophical logic, epistemology, rational belief, formal epistemology, or philosophy of language.",
        "the central result is mathematical logic or empirical cognition rather than a philosophical account of knowledge or meaning.", PHILOSOPHY,
        "Review the account of knowledge, evidence, justification, rationality, consequence, or meaning; formal-to-informal interpretation; skeptical and higher-order cases; counterexamples; and whether the conclusion follows under the epistemic norms stated."
    ),
    subject!(
        "subject_philosophy_metaphysics_mind", "Philosophy — Metaphysics & Mind", "philosophy", "Philosophy", Subfield,
        "Metaphysics, ontology, modality, causation, time, personal identity, consciousness, or philosophy of mind.",
        "the contribution is empirical neuroscience or psychology without a substantive metaphysical or philosophy-of-mind argument.", PHILOSOPHY,
        "Assess the ontology and modal commitments, grounding or dependence relation, identity conditions, conceivability and explanatory arguments, relation to science, thought experiments, and whether the view resolves rather than relocates the target problem."
    ),
    subject!(
        "subject_philosophy_ethics_political", "Philosophy — Ethics & Political", "philosophy", "Philosophy", Subfield,
        "Normative ethics, metaethics, applied ethics, political philosophy, justice, rights, or responsibility.",
        "the central contribution is empirical policy analysis or political theory grounded primarily in historical interpretation.", PHILOSOPHY,
        "Review the normative principle, moral standing and agency, distribution and institutions, demandingness, feasibility, aggregation, conflicts of value, treatment of cases and objections, and whether recommendations follow at the claimed practical level."
    ),
    subject!(
        "subject_philosophy_science_history", "Philosophy — Science & History of Philosophy", "philosophy", "Philosophy", Subfield,
        "Philosophy of science, biology, physics, social science, medicine, or historically grounded philosophy.",
        "the paper is history of science without a philosophical claim or science without conceptual analysis.", PHILOSOPHY,
        "Assess the scientific practice or historical text represented, explanation and evidence, idealization, realism, causation and models, interpretive context, relation to primary sources, and whether the philosophical conclusion depends on an accurate account of the science or figure."
    ),

    // Linguistics
    subject!(
        "subject_linguistics_general", "Linguistics — General", "linguistics", "Linguistics", Discipline,
        "Linguistic research spanning several levels or outside the listed linguistic specialties.",
        "a formal, sound-structure, or social-historical linguistic lens clearly fits.", LINGUISTICS,
        "Evaluate the linguistic object, language sample, descriptive adequacy, cross-linguistic scope, theoretical representation, judgments or corpus evidence, and whether generalizations distinguish language-specific facts from proposed universals."
    ),
    subject!(
        "subject_linguistics_syntax_semantics", "Linguistics — Syntax, Semantics & Pragmatics", "linguistics", "Linguistics", Subfield,
        "Syntax, formal semantics, pragmatics, discourse, morphology-syntax, or grammatical theory.",
        "the primary contribution is an NLP model or literary interpretation without a linguistic grammar claim.", LINGUISTICS,
        "Review constructions and contrasts, judgments and contexts, compositional analysis, interfaces, alternative derivations, typological coverage, dialect and speaker variation, and whether the formal account predicts both licensed and excluded readings."
    ),
    subject!(
        "subject_linguistics_sound_structure", "Linguistics — Phonetics, Phonology & Morphology", "linguistics", "Linguistics", Subfield,
        "Phonetics, phonology, morphology, speech production or perception, and sound or word structure.",
        "speech is merely an engineering signal and no linguistic sound-structure claim is made.", LINGUISTICS,
        "Assess acoustic or articulatory measurement, segmentation, lexical and prosodic context, speaker and dialect variation, phonological representation, alternations, typology, and whether evidence separates categorical structure from gradient production or perception."
    ),
    subject!(
        "subject_linguistics_social_historical", "Linguistics — Social, Historical & Computational", "linguistics", "Linguistics", Subfield,
        "Sociolinguistics, language variation and change, historical linguistics, documentation, corpus or computational linguistics with a linguistic contribution.",
        "the contribution is chiefly sociology, history, or computer science without a linguistic account of language structure or use.", LINGUISTICS,
        "Review community and corpus sampling, register and interaction, annotation, reconstruction and contact, chronology, language ideology, change mechanism, computational representation, and whether patterns support the claimed linguistic rather than demographic or technological explanation."
    ),

    // Education
    subject!(
        "subject_education_general", "Education — General", "education", "Education", Discipline,
        "Education research spanning policy, learning, instruction, institutions, or assessment.",
        "a listed education specialty clearly contains the intervention or institution.", EDUCATION,
        "Evaluate learners and educators, setting, learning objective, instructional or institutional mechanism, implementation, outcome meaning, equity, and whether evidence supports transfer beyond the observed educational context."
    ),
    subject!(
        "subject_education_policy", "Education — Policy & Economics", "education", "Education", Subfield,
        "Education policy, school choice, finance, accountability, teacher labor markets, access, or inequality.",
        "the paper is economics of education without material educational institutions or practice to assess.", EDUCATION,
        "Review institutional rules, student and school selection, implementation, capacity and incentives, outcome and distribution, equilibrium responses, subgroup effects, policy feasibility, and whether measured changes represent meaningful educational progress."
    ),
    subject!(
        "subject_education_learning", "Education — Learning, Curriculum & Instruction", "education", "Education", Subfield,
        "Learning sciences, pedagogy, curriculum, classroom instruction, teacher practice, or educational technology.",
        "the contribution is a general cognitive theory or technology with no educational learning claim.", EDUCATION,
        "Assess the learning construct and progression, prior knowledge, instructional design, teacher enactment, classroom context, engagement, fidelity, near and far transfer, assessment alignment, and whether observed performance reflects durable understanding."
    ),
    subject!(
        "subject_education_higher_measurement", "Education — Higher, Special & Assessment", "education", "Education", Subfield,
        "Higher education, special education, educational measurement, assessment, admissions, or institutional student support.",
        "the contribution is psychometric theory or organizational policy without a substantive education question.", EDUCATION,
        "Review student population and support needs, institutional pathway, accessibility, construct and measurement invariance, stakes and consequences, persistence, accommodations, selection, and whether scores or completion outcomes support the educational interpretation."
    ),

    // Law
    subject!(
        "subject_law_general", "Law — General", "law", "Law", Discipline,
        "Legal scholarship spanning several domains or outside the listed legal specialties.",
        "a listed doctrinal, private-criminal, international, or empirical-legal lens clearly fits.", LAW,
        "Evaluate the governing authority, jurisdiction, legal question, interpretive method, institutional competence, administrability, precedent, remedy, and the difference between descriptive doctrine and normative reform."
    ),
    subject!(
        "subject_law_public", "Law — Constitutional, Administrative & Regulatory", "law", "Law", Subfield,
        "Constitutional, administrative, regulatory, legislation, courts, public law, or governance doctrine.",
        "the primary question is international, private, or criminal law without a material public-law issue.", LAW,
        "Review text and precedent, standard of review, institutional authority, separation and delegation, procedural posture, regulatory implementation, federal or jurisdictional allocation, remedy, and whether the proposed rule is doctrinally and administratively coherent."
    ),
    subject!(
        "subject_law_private_criminal", "Law — Private & Criminal", "law", "Law", Subfield,
        "Contracts, torts, property, corporations, commercial, family, criminal law, procedure, or punishment.",
        "the central contribution is public regulation or empirical crime analysis without doctrinal private or criminal law.", LAW,
        "Assess elements and defenses, rights and duties, causation and fault, procedural posture, remedies and sanctions, party incentives, interactions across doctrines, administrability, and whether hard cases expose instability in the proposed rule."
    ),
    subject!(
        "subject_law_international", "Law — International & Comparative", "law", "Law", Subfield,
        "Public or private international law, human rights, trade law, comparative law, transnational regulation, or conflict of laws.",
        "international relations or comparative politics is central but no legal-authority or doctrinal claim is made.", LAW,
        "Review sources and hierarchy of law, jurisdiction and choice of law, treaty interpretation, state practice, institutional authority, compliance and enforcement, comparative functional equivalence, and whether legal conclusions travel across the systems named."
    ),
    subject!(
        "subject_law_empirical_economic", "Law — Empirical & Law and Economics", "law", "Law", Subfield,
        "Empirical legal studies, law and economics, legal institutions, judicial behavior, or quantitative doctrinal consequences.",
        "the contribution is an economics or political-science result with law only as background.", LAW,
        "Review the legal treatment and institutional setting, case and decision selection, coding of doctrine and outcomes, strategic behavior, identification, external validity across jurisdictions, welfare and distribution, and whether empirical findings warrant the legal reform proposed."
    ),

    // Business and management
    subject!(
        "subject_business_general", "Business — General", "business", "Business & Management", Discipline,
        "Business or management research spanning several functions or outside the listed specialties.",
        "a listed strategy, operations, accounting-finance, or marketing lens clearly contains the contribution.", BUSINESS,
        "Evaluate the organization or market, decision maker, managerial mechanism, performance objective, institutional setting, implementation, and whether evidence supports useful inference for firms without treating managerial relevance as self-evident."
    ),
    subject!(
        "subject_business_strategy_organization", "Business — Strategy, Organization & Entrepreneurship", "business", "Business & Management", Subfield,
        "Strategy, organization theory, organizational behavior, entrepreneurship, innovation, or human resources.",
        "the contribution is industrial organization economics or sociology without a managerial or firm-strategy object.", BUSINESS,
        "Review firm boundaries and capabilities, competitive and institutional setting, managerial choice, organization and incentives, selection and performance, innovation pathway, founder or workforce heterogeneity, and whether evidence distinguishes strategy from ex post success narratives."
    ),
    subject!(
        "subject_business_operations_information", "Business — Operations & Information Systems", "business", "Business & Management", Subfield,
        "Operations management, supply chains, service systems, analytics, information systems, or digital operations.",
        "the central contribution is an engineering or computer-science system without an organizational operating decision.", BUSINESS,
        "Assess process and demand assumptions, capacity and inventory, network and supplier constraints, service levels, digital adoption, human workflow, disruption and resilience, implementation costs, and whether optimization gains survive operational uncertainty."
    ),
    subject!(
        "subject_business_accounting_finance", "Business — Accounting & Corporate Finance", "business", "Business & Management", Subfield,
        "Accounting, auditing, disclosure, governance, corporate finance, capital markets, or taxation in firms.",
        "the primary contribution is asset-pricing or public-finance economics without a firm reporting or governance question.", BUSINESS,
        "Review reporting and institutional rules, measurement of disclosure or governance, contracting and information channels, selection, market response, managerial incentives, real effects, audit or enforcement context, and whether findings imply the claimed corporate decision."
    ),
    subject!(
        "subject_business_marketing", "Business — Marketing & Consumer", "business", "Business & Management", Subfield,
        "Marketing, consumer behavior, branding, pricing, channels, sales, advertising, or customer analytics.",
        "the main contribution is general psychology or demand estimation without a marketing decision or market context.", BUSINESS,
        "Assess consumer construct and choice environment, targeting and exposure, channel and competitive context, pricing, measurement of response, short- versus long-run effects, heterogeneity, firm objective, and whether experimental or observational evidence supports the marketing action proposed."
    ),

    // Humanities
    subject!(
        "subject_humanities_general", "Humanities — General", "humanities", "Humanities", Discipline,
        "Humanities scholarship spanning several traditions or outside the listed literary, religious, visual, media, or digital fields.",
        "a listed humanities specialty clearly contains the primary objects and scholarly conversation.", HUMANITIES,
        "Evaluate the archive or corpus, historical and linguistic context, interpretive problem, conceptual vocabulary, evidence from form and detail, relation to scholarship, counterreadings, and the scale of the cultural claim."
    ),
    subject!(
        "subject_humanities_literature_classics", "Humanities — Literature & Classics", "humanities", "Humanities", Subfield,
        "Literary studies, comparative literature, classics, philology, rhetoric, or book history.",
        "the central contribution is historical fact, formal linguistics, or automated text analysis without literary interpretation.", HUMANITIES,
        "Review textual version and language, genre and form, close-reading evidence, historical and intertextual context, translation, corpus selection, relation between local passages and broad interpretation, and whether plausible counterreadings are answered."
    ),
    subject!(
        "subject_humanities_religion", "Humanities — Religion & Theology", "humanities", "Humanities", Subfield,
        "Religious studies, theology, scriptural interpretation, ritual, doctrine, or religion and society.",
        "religion is only a demographic category and no religious text, practice, institution, or theological argument is central.", HUMANITIES,
        "Assess tradition-specific categories, sources and languages, doctrine and practice, historical location, insider and analytic perspectives, comparison, normative commitments, and whether evidence supports claims across communities or periods."
    ),
    subject!(
        "subject_humanities_art_music", "Humanities — Art, Architecture & Music", "humanities", "Humanities", Subfield,
        "Art history, architectural history, visual culture, musicology, performance, or material aesthetics.",
        "images or music are data inputs without an art-historical, architectural, or musicological contribution.", HUMANITIES,
        "Review object provenance and condition, formal and material analysis, attribution and chronology, site or performance context, circulation and reception, comparison, reproduction limits, and whether visual or sonic details sustain the historical interpretation."
    ),
    subject!(
        "subject_humanities_media_cultural", "Humanities — Media & Cultural Studies", "humanities", "Humanities", Subfield,
        "Film, television, media, communication, cultural studies, popular culture, games, or performance studies.",
        "the contribution is a technical media system or quantitative communication effect without interpretive cultural analysis.", HUMANITIES,
        "Assess corpus and platform selection, medium and form, production and circulation, audience and reception, industry and power, historical context, representational analysis, and whether selected objects support claims about a wider culture."
    ),
    subject!(
        "subject_humanities_digital_public", "Humanities — Digital & Public", "humanities", "Humanities", Subfield,
        "Digital humanities, public humanities, archives and editions, cultural heritage, museums, or computational cultural analysis.",
        "the central contribution is a general computer-science method or public-history narrative without a humanities research object.", HUMANITIES,
        "Review corpus and metadata provenance, digitization and OCR bias, modeling choices, interpretability, interface and audience, preservation, community authority, public representation, and whether computational patterns return to historically and textually grounded claims."
    ),

    // Agricultural, food, and veterinary sciences
    subject!(
        "subject_agriculture_veterinary_general", "Agriculture & Veterinary Science — General", "agriculture_veterinary", "Agriculture & Veterinary Science", Discipline,
        "Agricultural, food, forestry, fisheries, animal, or veterinary research spanning several systems or outside the listed specialties.",
        "a listed agricultural, food, animal, veterinary, forestry, or rural-systems lens clearly contains the main contribution.", AGRICULTURE_VETERINARY,
        "Evaluate the managed biological system, production objective, environment, inputs, biological and economic constraints, intervention, yield or welfare outcome, season and scale, and whether evidence supports transfer from the studied conditions to the claimed agricultural system. Use this fallback only for genuinely cross-system work."
    ),
    subject!(
        "subject_agriculture_veterinary_crop_soil", "Agriculture & Veterinary Science — Crop, Soil & Horticulture", "agriculture_veterinary", "Agriculture & Veterinary Science", Subfield,
        "Agronomy, crop science, horticulture, soil science, agroecology, plant breeding, or crop protection.",
        "the contribution is basic plant biology without a managed production or agroecosystem claim.", AGRICULTURE_VETERINARY,
        "Review genotype, cultivar, soil and nutrient conditions, pest and disease pressure, weather and season, plot and farm design, management interactions, yield and quality measures, carryover effects, and whether controlled or site-specific evidence supports performance across the target production environment."
    ),
    subject!(
        "subject_agriculture_veterinary_animal", "Agriculture & Veterinary Science — Animal & Veterinary", "agriculture_veterinary", "Agriculture & Veterinary Science", Subfield,
        "Animal science, veterinary medicine, livestock systems, animal health, breeding, welfare, or comparative clinical research.",
        "the central contribution is human clinical medicine, wildlife ecology, or cellular biology without an animal-health or production-system claim.", AGRICULTURE_VETERINARY,
        "Assess species, breed and production setting, husbandry, nutrition, exposure or intervention, diagnostic definition, welfare and adverse outcomes, clustering by herd or facility, pathogen and environmental context, follow-up, and whether findings support the claimed animal population and management practice."
    ),
    subject!(
        "subject_agriculture_veterinary_food", "Agriculture & Veterinary Science — Food Science & Safety", "agriculture_veterinary", "Agriculture & Veterinary Science", Subfield,
        "Food chemistry, processing, preservation, sensory science, food microbiology, quality, or food safety.",
        "the contribution is human nutrition, molecular chemistry, or process engineering without a food-system quality or safety claim.", AGRICULTURE_VETERINARY,
        "Review raw-material variation, formulation and processing conditions, microbial or chemical hazards, sampling, shelf life, sensory design, analytical validation, storage and distribution, scale-up, comparator products, and whether laboratory quality or safety measures support the claimed consumer and production setting."
    ),
    subject!(
        "subject_agriculture_veterinary_forestry_fisheries", "Agriculture & Veterinary Science — Forestry, Fisheries & Aquaculture", "agriculture_veterinary", "Agriculture & Veterinary Science", Subfield,
        "Forestry, silviculture, fisheries science, aquaculture, rangelands, or renewable biological-resource management.",
        "the central contribution is conservation biology or environmental policy without a managed harvest, production, or resource-system claim.", AGRICULTURE_VETERINARY,
        "Assess stock or stand definition, age and species structure, recruitment and mortality, harvest or production regime, habitat and climate variation, monitoring and detectability, spatial spillovers, ecological feedback, and whether short-run or local evidence supports sustainable management at the claimed scale."
    ),
    subject!(
        "subject_agriculture_veterinary_systems_rural", "Agriculture & Veterinary Science — Agricultural Systems & Rural Development", "agriculture_veterinary", "Agriculture & Veterinary Science", Subfield,
        "Farming systems, agricultural extension, rural development, food systems, farm management, or technology adoption.",
        "the contribution is development economics or rural sociology without material agricultural production, extension, or food-system expertise.", AGRICULTURE_VETERINARY,
        "Review farm and household objectives, agroecological constraints, labor and input access, extension and adoption pathway, risk and seasonality, value-chain and market context, heterogeneity across producers, scalability, and whether productivity, resilience, income, and distributional claims are jointly supported."
    ),

    // Communication and information
    subject!(
        "subject_communication_information_general", "Communication & Information — General", "communication_information", "Communication & Information", Discipline,
        "Communication, journalism, information, or knowledge-institution research spanning several areas or outside the listed specialties.",
        "a listed communication, information, risk-communication, or interpersonal lens clearly contains the central process.", COMMUNICATION_INFORMATION,
        "Evaluate the communicative actors, message or information object, medium, audience, institution, production and circulation process, reception, and claimed effect. Use this fallback only when the contribution genuinely bridges communication and information fields."
    ),
    subject!(
        "subject_communication_information_media_journalism", "Communication & Information — Media, Journalism & Public Communication", "communication_information", "Communication & Information", Subfield,
        "Journalism studies, mass communication, news, political communication, public relations, or media institutions.",
        "the contribution is interpretive media studies without a communication-process claim or political behavior without a material media institution.", COMMUNICATION_INFORMATION,
        "Review outlet and platform selection, production routines, gatekeeping, source and message construction, audience exposure, media-system context, ownership and incentives, trust, reception, and whether content or audience evidence supports the claimed public-communication effect."
    ),
    subject!(
        "subject_communication_information_library", "Communication & Information — Library & Information Science", "communication_information", "Communication & Information", Subfield,
        "Library and information science, archives, knowledge organization, information behavior, scholarly communication, or information institutions.",
        "the central contribution is database engineering or digital humanities without an information-practice or institution claim.", COMMUNICATION_INFORMATION,
        "Assess collection and user boundaries, metadata and classification, provenance, retrieval and discovery, preservation, access and exclusion, professional practice, platform governance, information behavior, and whether system metrics represent meaningful access, use, or knowledge organization."
    ),
    subject!(
        "subject_communication_information_science_health_risk", "Communication & Information — Science, Health & Risk Communication", "communication_information", "Communication & Information", Subfield,
        "Communication of science, medicine, environment, uncertainty, crisis, hazards, or public risk.",
        "the contribution is clinical, environmental, or engineering risk assessment without a communicative process or audience claim.", COMMUNICATION_INFORMATION,
        "Review the scientific or risk object, source credibility, uncertainty framing, numeracy and visual display, audience knowledge and trust, channel, misinformation context, behavioral outcome, equity and accessibility, and whether measured responses support the claimed communication mechanism."
    ),
    subject!(
        "subject_communication_information_interpersonal_organizational", "Communication & Information — Interpersonal & Organizational", "communication_information", "Communication & Information", Subfield,
        "Interpersonal, organizational, health, family, group, or computer-mediated communication.",
        "the main contribution is social psychology or management without a material message, interaction, discourse, or communication-system claim.", COMMUNICATION_INFORMATION,
        "Assess interaction and relationship context, message and channel, conversational sequence, power and role structure, self-report versus observed communication, cultural and organizational setting, privacy and mediation, and whether communicative evidence supports the claimed relational or institutional outcome."
    ),

    // Architecture, planning, and design
    subject!(
        "subject_architecture_design_general", "Architecture, Planning & Design — General", "architecture_design", "Architecture, Planning & Design", Discipline,
        "Architecture, planning, landscape, or design scholarship spanning several areas or outside the listed specialties.",
        "a listed built-environment, planning, landscape, or design lens clearly contains the central artifact or intervention.", ARCHITECTURE_DESIGN,
        "Evaluate the people and place served, design problem, site and institutional context, program and constraints, artifact or plan, performance and experience, implementation, and evidence connecting design choices to the claimed outcome. Use this fallback only for genuinely cross-disciplinary design work."
    ),
    subject!(
        "subject_architecture_design_built_environment", "Architecture, Planning & Design — Architecture & Built Environment", "architecture_design", "Architecture, Planning & Design", Subfield,
        "Architectural design and theory, building science, housing design, interiors, heritage conservation, or built-environment research.",
        "the contribution is structural engineering or art history without a material architectural performance, use, or design argument.", ARCHITECTURE_DESIGN,
        "Review site and program, spatial organization, users and accessibility, material and environmental performance, codes and constructability, representation, precedent, occupation and post-occupancy evidence, and whether the proposed building or typology answers its stated social and physical constraints."
    ),
    subject!(
        "subject_architecture_design_planning", "Architecture, Planning & Design — Urban & Regional Planning", "architecture_design", "Architecture, Planning & Design", Subfield,
        "Urban and regional planning, land use, transportation planning, housing, infrastructure governance, or community development.",
        "the central contribution is urban economics, geography, or civil engineering without a planning institution, process, or intervention.", ARCHITECTURE_DESIGN,
        "Assess planning authority and process, spatial scale, land and infrastructure constraints, affected populations, participation, implementation and enforcement, displacement and distribution, scenario assumptions, temporal horizon, and whether the proposal remains feasible under political and market response."
    ),
    subject!(
        "subject_architecture_design_landscape", "Architecture, Planning & Design — Landscape Architecture", "architecture_design", "Architecture, Planning & Design", Subfield,
        "Landscape architecture, ecological design, public space, site planning, or landscape performance.",
        "the work is ecosystem science or geography without a designed landscape, site, or public-space claim.", ARCHITECTURE_DESIGN,
        "Review site history and ecology, hydrology and climate, soils and vegetation, circulation and access, maintenance and succession, human use, seasonal performance, representation and construction, and whether site evidence supports ecological and social outcomes over the claimed time horizon."
    ),
    subject!(
        "subject_architecture_design_human_centered", "Architecture, Planning & Design — Human-Centered & Product Design", "architecture_design", "Architecture, Planning & Design", Subfield,
        "Industrial, product, service, interaction, participatory, or human-centered design where design knowledge is the contribution.",
        "the central contribution is HCI evaluation, mechanical engineering, or marketing without a material design inquiry or artifact claim.", ARCHITECTURE_DESIGN,
        "Assess problem framing, stakeholders and excluded users, design criteria, alternatives and iteration, form and function, accessibility, lifecycle and repair, context of use, evaluation with representative users, and whether one artifact supports the stated general design principle."
    ),

    // Social work and social policy
    subject!(
        "subject_social_work_policy_general", "Social Work & Social Policy — General", "social_work_policy", "Social Work & Social Policy", Discipline,
        "Social work, human services, welfare, or community-intervention research spanning several areas or outside the listed specialties.",
        "a listed practice, welfare-policy, or community-organization lens clearly contains the intervention and outcome.", SOCIAL_WORK_POLICY,
        "Evaluate the population and need, service or policy setting, theory of change, practitioner and institution, access and uptake, client-defined and administrative outcomes, implementation, equity, and whether evidence supports responsible transfer to the named population."
    ),
    subject!(
        "subject_social_work_policy_practice", "Social Work & Social Policy — Practice & Human Services", "social_work_policy", "Social Work & Social Policy", Subfield,
        "Clinical and direct social work, child and family services, mental-health services, case management, safeguarding, or human-service delivery.",
        "the contribution is clinical treatment efficacy or organizational management without a social-work practice, service, or person-in-environment claim.", SOCIAL_WORK_POLICY,
        "Review client and family context, practitioner role and discretion, service pathway, engagement and dropout, safeguarding, fidelity and adaptation, multidisciplinary coordination, meaningful outcomes, burden and access, and whether supported-practice evidence travels to ordinary service conditions."
    ),
    subject!(
        "subject_social_work_policy_welfare", "Social Work & Social Policy — Welfare & Social Policy", "social_work_policy", "Social Work & Social Policy", Subfield,
        "Welfare states, poverty policy, social protection, disability policy, family policy, housing support, or comparative social policy.",
        "the paper is public economics or political administration without a material welfare institution, service-user, or social-policy contribution.", SOCIAL_WORK_POLICY,
        "Assess eligibility and entitlement, target population, take-up and exclusion, benefit and service interaction, administrative burden, family and labor responses, distribution, implementation capacity, comparative institutional fit, and whether measured outcomes support the broader welfare conclusion."
    ),
    subject!(
        "subject_social_work_policy_community_nonprofit", "Social Work & Social Policy — Community & Nonprofit", "social_work_policy", "Social Work & Social Policy", Subfield,
        "Community practice, nonprofit and voluntary organizations, mutual aid, social development, advocacy, or collective service provision.",
        "the main contribution is organizational sociology or public administration without a community-practice, voluntary-sector, or service mission.", SOCIAL_WORK_POLICY,
        "Review community definition and representation, governance and accountability, participation and power, organizational capacity, funding and mission drift, volunteer and paid labor, partnership, reach, sustainability, and whether local outcomes support the claimed community or sector-wide mechanism."
    ),

    // Health professions
    subject!(
        "subject_health_professions_general", "Health Professions — General", "health_professions", "Health Professions", Discipline,
        "Nursing, rehabilitation, pharmacy, oral health, nutrition, or allied-health research spanning several professions or outside the listed specialties.",
        "a listed health-profession lens or a medicine-and-health specialist clearly contains the care process and outcome.", HEALTH_PROFESSIONS,
        "Evaluate the person and care setting, professional practice, intervention or assessment, scope of practice, team and workflow, patient-important outcome, implementation and equity, and whether evidence supports adoption in the named service context. Keep professional-process claims distinct from biological efficacy alone."
    ),
    subject!(
        "subject_health_professions_nursing", "Health Professions — Nursing & Care Science", "health_professions", "Health Professions", Subfield,
        "Nursing science, midwifery, care delivery, symptom management, patient safety, or nursing workforce research.",
        "the central contribution is physician-led clinical efficacy or health-services economics without a nursing or care-process claim.", HEALTH_PROFESSIONS,
        "Review patient acuity and setting, nursing intervention and dose, staffing and skill mix, continuity, protocol and professional judgment, patient and caregiver experience, safety and missed care, implementation burden, clustering by unit, and whether outcomes follow from the claimed nursing process."
    ),
    subject!(
        "subject_health_professions_rehabilitation", "Health Professions — Rehabilitation, Disability & Occupational Therapy", "health_professions", "Health Professions", Subfield,
        "Physical, occupational, speech, vocational, or multidisciplinary rehabilitation; disability and functioning research.",
        "the contribution is basic motor science or acute medical treatment without a rehabilitation, participation, or functioning claim.", HEALTH_PROFESSIONS,
        "Assess impairment, activity and participation targets, baseline function, intervention dose and progression, therapist and setting effects, assistive technology, adherence, meaningful change, durability, accessibility, and whether standardized measures reflect goals important to the population studied."
    ),
    subject!(
        "subject_health_professions_pharmacy", "Health Professions — Pharmacy & Pharmacoepidemiology", "health_professions", "Health Professions", Subfield,
        "Pharmacy practice, medication use and safety, pharmacotherapy, pharmacoepidemiology, pharmacovigilance, or medicines policy.",
        "the central contribution is molecular pharmacology, a clinical trial, or health economics without a medication-use or pharmacy-practice claim.", HEALTH_PROFESSIONS,
        "Review medication exposure and indication, dispensing and adherence, dose and switching, interactions, confounding by disease severity, safety ascertainment, surveillance and reporting, pharmacist intervention, care setting, and whether observed use supports benefit, harm, or implementation claims."
    ),
    subject!(
        "subject_health_professions_dentistry", "Health Professions — Dentistry & Oral Health", "health_professions", "Health Professions", Subfield,
        "Dentistry, oral medicine, dental materials in clinical use, oral epidemiology, prevention, or oral-health services.",
        "the main contribution is materials engineering or general epidemiology without an oral-health mechanism, procedure, or care claim.", HEALTH_PROFESSIONS,
        "Assess oral condition and diagnostic criteria, tooth and patient levels, operator and site effects, procedure and comparator, material and biological outcomes, follow-up and restoration survival, prevention and adherence, access, and whether evidence supports the intended dental population and practice."
    ),
    subject!(
        "subject_health_professions_nutrition_exercise", "Health Professions — Nutrition, Dietetics & Exercise", "health_professions", "Health Professions", Subfield,
        "Human nutrition, dietetics, exercise science, sports medicine, physical activity, or lifestyle intervention research.",
        "the contribution is food chemistry, elite performance engineering, or population epidemiology without a material nutrition, exercise, or professional-practice claim.", HEALTH_PROFESSIONS,
        "Review dietary or activity assessment, energy balance and dose, adherence and substitution, baseline status, training and recovery, body-composition and performance measures, confounding lifestyle changes, clinically meaningful outcomes, safety, and whether short controlled exposure supports sustained real-world benefit."
    ),

    // Interdisciplinary studies
    subject!(
        "subject_interdisciplinary_studies_general", "Interdisciplinary Studies — General", "interdisciplinary_studies", "Interdisciplinary Studies", Discipline,
        "Interdisciplinary scholarship whose central contribution cannot be evaluated adequately from one listed discipline or more specific interdisciplinary field.",
        "one or two listed disciplinary or interdisciplinary subfield reviewers can cover the paper's central contribution without a general fallback.", INTERDISCIPLINARY_STUDIES,
        "Evaluate why the paper combines its fields, whether their concepts and standards are translated rather than merely juxtaposed, where evidence from one domain bears on claims in another, and whether the integrated conclusion is stronger than parallel disciplinary observations. Use this fallback only when no narrower subject role fits."
    ),
    subject!(
        "subject_interdisciplinary_studies_sts", "Interdisciplinary Studies — Science, Technology & Society", "interdisciplinary_studies", "Interdisciplinary Studies", Subfield,
        "Science and technology studies, sociology or anthropology of knowledge, infrastructure studies, innovation studies, or critical data and algorithm studies.",
        "the contribution is philosophy or history of science, technical system design, or innovation economics without a material knowledge-practice or sociotechnical claim.", INTERDISCIPLINARY_STUDIES,
        "Review the scientific or technical practice, actors and institutions, material infrastructure, standards and classifications, production of expertise, co-production of social and technical order, historical setting, and whether local cases support the broader sociotechnical mechanism claimed."
    ),
    subject!(
        "subject_interdisciplinary_studies_gender_sexuality", "Interdisciplinary Studies — Gender & Sexuality", "interdisciplinary_studies", "Interdisciplinary Studies", Subfield,
        "Gender, sexuality, feminist, queer, masculinity, or intersectional studies across social-scientific and humanistic traditions.",
        "gender or sexuality appears only as a demographic covariate and no category, institution, identity, representation, or power relation is central.", INTERDISCIPLINARY_STUDIES,
        "Assess how categories are defined historically and socially, whose experience and evidence are represented, interactions with race, class, disability, nation and other structures, institutional and cultural mechanisms, positionality, counterexamples, and whether claims travel across populations or periods without erasing relevant variation."
    ),
    subject!(
        "subject_interdisciplinary_studies_race_indigenous", "Interdisciplinary Studies — Race, Ethnicity & Indigenous", "interdisciplinary_studies", "Interdisciplinary Studies", Subfield,
        "Ethnic studies, race and diaspora studies, Indigenous studies, settler-colonial studies, or community-grounded scholarship across fields.",
        "race, ethnicity, or indigeneity is only a control variable and no historical, institutional, cultural, territorial, or knowledge-governance claim is central.", INTERDISCIPLINARY_STUDIES,
        "Review category and community specificity, historical and territorial context, colonial and state institutions, migration and diaspora, language and source authority, community knowledge and data governance, researcher position, internal heterogeneity, and whether comparison preserves rather than flattens distinct histories and political relationships."
    ),
    subject!(
        "subject_interdisciplinary_studies_area_global", "Interdisciplinary Studies — Area & Global", "interdisciplinary_studies", "Interdisciplinary Studies", Subfield,
        "Regional, area, transnational, postcolonial, development, or global studies integrating language, history, institutions, and contemporary evidence.",
        "the contribution fits a specific history, politics, economics, geography, literature, or anthropology subfield without material interdisciplinary regional synthesis.", INTERDISCIPLINARY_STUDIES,
        "Assess regional and language competence, period and boundary choices, comparison and connected histories, colonial and geopolitical categories, source and case coverage, movement across local, national and global scales, and whether a broad global claim is grounded in the regions and institutions actually studied."
    ),
];
