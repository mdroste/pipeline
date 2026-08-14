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
const NEUROSCIENCE: &str = include_str!("../../../../prompts/auto_review/subjects/neuroscience.md");
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

// Each role's subfield review focus lives beside the discipline lenses in
// `prompts/auto_review/subjects/focus/{id}.md`, so all long-form reviewer
// prose is Markdown and this file holds only compact routing metadata.
macro_rules! subject {
    ($id:literal, $label:literal, $discipline:literal, $discipline_label:literal,
     $level:ident, $description:literal, $exclusions:literal, $base:ident) => {
        SubjectSpec {
            id: $id,
            label: $label,
            discipline_id: $discipline,
            discipline_label: $discipline_label,
            level: SubjectLevel::$level,
            routing_description: $description,
            routing_exclusions: $exclusions,
            discipline_prompt: $base,
            review_focus: include_str!(concat!(
                "../../../../prompts/auto_review/subjects/focus/",
                $id,
                ".md"
            )),
        }
    };
}

pub const SUBJECTS: &[SubjectSpec] = &[
    // Physics
    subject!(
        "subject_physics_general", "Physics — General", "physics", "Physics", Discipline,
        "Broad physics research that does not fit a more specific physics subfield in this catalog.",
        "a listed physics subfield clearly carries the main contribution.", PHYSICS
    ),
    subject!(
        "subject_physics_theoretical_mathematical", "Physics — Theoretical & Mathematical", "physics", "Physics", Subfield,
        "Theoretical or mathematical physics centered on formal physical models, symmetries, fields, or exact structure.",
        "the result is primarily a pure mathematical theorem without a material physical interpretation.", PHYSICS
    ),
    subject!(
        "subject_physics_particle_nuclear", "Physics — Particle & Nuclear", "physics", "Physics", Subfield,
        "Particle, high-energy, nuclear, hadronic, accelerator, or fundamental-interaction research.",
        "astrophysical use of particle models is central but laboratory or nuclear physics is not.", PHYSICS
    ),
    subject!(
        "subject_physics_condensed_materials", "Physics — Condensed Matter & Materials", "physics", "Physics", Subfield,
        "Condensed-matter, soft-matter, mesoscopic, many-body, or materials-physics research.",
        "the primary contribution is materials synthesis or engineering performance rather than physical mechanism.", PHYSICS
    ),
    subject!(
        "subject_physics_amo_quantum", "Physics — AMO & Quantum Optics", "physics", "Physics", Subfield,
        "Atomic, molecular, optical, photonic, ultracold-matter, or quantum-optics research.",
        "the paper is chiefly about quantum algorithms or information protocols rather than a physical AMO platform.", PHYSICS
    ),
    subject!(
        "subject_physics_statistical_complex", "Physics — Statistical & Complex Systems", "physics", "Physics", Subfield,
        "Statistical mechanics, nonlinear dynamics, complex systems, networks in physics, or nonequilibrium phenomena.",
        "network analysis is primarily social or computational and lacks a physical statistical-mechanics claim.", PHYSICS
    ),
    subject!(
        "subject_physics_astrophysics_cosmology", "Physics — Astrophysics & Cosmology (General)", "physics", "Physics", Subfield,
        "Astrophysics or astronomy spanning several areas or outside the listed astronomy subfields.",
        "the main object is terrestrial geophysics, or a listed astronomy subfield — stellar and exoplanetary, galactic and extragalactic, or cosmology and gravitation — clearly carries the contribution.", PHYSICS
    ),
    subject!(
        "subject_physics_stellar_exoplanetary", "Physics — Stellar & Exoplanetary Astrophysics", "physics", "Physics", Subfield,
        "Stars, stellar evolution, compact objects as stellar endpoints, exoplanets, planet formation, or stellar-system dynamics.",
        "galaxy-scale structure, cosmological inference, or planetary geoscience of solar-system bodies carries the contribution.", PHYSICS
    ),
    subject!(
        "subject_physics_galactic_extragalactic", "Physics — Galactic & Extragalactic Astronomy", "physics", "Physics", Subfield,
        "Galaxy formation and evolution, interstellar and circumgalactic media, active galactic nuclei, or large galaxy surveys.",
        "cosmological parameter inference or individual stellar and exoplanetary systems carry the contribution.", PHYSICS
    ),
    subject!(
        "subject_physics_cosmology_gravitation", "Physics — Cosmology & Gravitation", "physics", "Physics", Subfield,
        "Cosmological parameter inference, the early universe, dark matter and dark energy, gravitational waves, or tests of gravity.",
        "galaxy astrophysics where cosmology is only a backdrop, or laboratory gravity experiments reviewed as instrumentation.", PHYSICS
    ),
    subject!(
        "subject_physics_plasma_fluid", "Physics — Plasma & Fluid", "physics", "Physics", Subfield,
        "Plasma physics, fluid dynamics, magnetohydrodynamics, turbulence, or continuum-flow research.",
        "the main contribution is an engineering device with fluid behavior only as an input.", PHYSICS
    ),
    subject!(
        "subject_physics_geophysics", "Physics — Geophysics", "physics", "Physics", Subfield,
        "Physical study of Earth's interior, seismology, geomagnetism, geodynamics, or planetary interiors.",
        "the central contribution concerns climate, ecology, or descriptive geology rather than a physical Earth model.", PHYSICS
    ),
    subject!(
        "subject_physics_quantum_information", "Physics — Quantum Information", "physics", "Physics", Subfield,
        "Quantum information, communication, sensing, error correction, or physically realized quantum computation.",
        "the contribution is a classical algorithm or an AMO experiment without an information-theoretic claim.", PHYSICS
    ),
    subject!(
        "subject_physics_biological", "Physics — Biological & Soft-Matter Physics", "physics", "Physics", Subfield,
        "Biological physics, active matter, biomechanics, membrane and polymer physics, cellular mechanics, or quantitative physical models of living systems.",
        "biological mechanism is central but no physical law, scale, material property, or nonequilibrium model carries the contribution.", PHYSICS
    ),
    subject!(
        "subject_physics_optics_photonics", "Physics — Optics & Photonics", "physics", "Physics", Subfield,
        "Classical optics, photonics, lasers, imaging physics, metamaterials, nonlinear optics, or light propagation and detection.",
        "atomic or molecular quantum-state control is central, or the contribution is primarily an engineered device without a material optical-physics claim.", PHYSICS
    ),
    subject!(
        "subject_physics_applied_devices", "Physics — Applied Physics & Devices", "physics", "Physics", Subfield,
        "Applied physics of electronic, magnetic, thermal, nano-, sensor, or energy-conversion devices where a physical transport or transduction mechanism is central.",
        "engineering integration, reliability, or product performance is the main contribution and the underlying physics is standard.", PHYSICS
    ),

    // Mathematics
    subject!(
        "subject_mathematics_general", "Mathematics — General", "mathematics", "Mathematics", Discipline,
        "Pure or applied mathematics spanning several areas or outside the listed mathematical subfields.",
        "a listed mathematical subfield clearly contains the main theorem or construction.", MATHEMATICS
    ),
    subject!(
        "subject_mathematics_algebra_number", "Mathematics — Algebra & Number Theory", "mathematics", "Mathematics", Subfield,
        "Algebra, representation theory, algebraic geometry, arithmetic geometry, or number theory.",
        "the central result is geometric or analytic without material algebraic or arithmetic structure.", MATHEMATICS
    ),
    subject!(
        "subject_mathematics_geometry_topology", "Mathematics — Geometry & Topology", "mathematics", "Mathematics", Subfield,
        "Differential, algebraic, symplectic, metric, or discrete geometry and algebraic or geometric topology.",
        "geometric language is merely a representation of an analytic or applied problem.", MATHEMATICS
    ),
    subject!(
        "subject_mathematics_analysis", "Mathematics — Analysis", "mathematics", "Mathematics", Subfield,
        "Real, complex, functional, harmonic, operator, or variational analysis not primarily organized around a PDE.",
        "the principal contribution is a differential-equation existence, regularity, or dynamics result.", MATHEMATICS
    ),
    subject!(
        "subject_mathematics_pde", "Mathematics — PDE & Calculus of Variations", "mathematics", "Mathematics", Subfield,
        "Partial differential equations, calculus of variations, geometric flows, or continuum mathematical models.",
        "the equation is only a numerical test problem or routine applied model without a mathematical PDE contribution.", MATHEMATICS
    ),
    subject!(
        "subject_mathematics_probability", "Mathematics — Probability", "mathematics", "Mathematics", Subfield,
        "Probability theory, stochastic processes, random structures, concentration, or stochastic analysis.",
        "probability is used only for routine statistical inference rather than as the mathematical contribution.", MATHEMATICS
    ),
    subject!(
        "subject_mathematics_combinatorics", "Mathematics — Combinatorics & Discrete", "mathematics", "Mathematics", Subfield,
        "Combinatorics, graph theory, discrete geometry, extremal or probabilistic combinatorics.",
        "the graph or discrete representation serves only an algorithmic engineering objective.", MATHEMATICS
    ),
    subject!(
        "subject_mathematics_logic_foundations", "Mathematics — Logic & Foundations", "mathematics", "Mathematics", Subfield,
        "Mathematical logic, set theory, model theory, proof theory, computability, or foundations.",
        "formal verification is used only as a computer-science implementation tool.", MATHEMATICS
    ),
    subject!(
        "subject_mathematics_applied_numerical", "Mathematics — Applied & Numerical", "mathematics", "Mathematics", Subfield,
        "Applied mathematics, numerical analysis, inverse problems, scientific computing, or approximation theory.",
        "the work is principally an engineering application or software benchmark without mathematical analysis.", MATHEMATICS
    ),
    subject!(
        "subject_mathematics_optimization_dynamics", "Mathematics — Optimization, Control & Dynamics", "mathematics", "Mathematics", Subfield,
        "Optimization theory, optimal control, operations research mathematics, dynamical systems, or ergodic dynamics.",
        "optimization is merely a routine fitting algorithm or the main contribution is an engineering controller.", MATHEMATICS
    ),
    subject!(
        "subject_mathematics_algebraic_geometry_representation", "Mathematics — Algebraic Geometry & Representation Theory", "mathematics", "Mathematics", Subfield,
        "Algebraic geometry, schemes and stacks, representation theory, Lie theory, homological algebra, or interactions among these structures.",
        "the principal objects are elementary algebraic or number-theoretic and do not require geometric, categorical, or representation-theoretic structure.", MATHEMATICS
    ),
    subject!(
        "subject_mathematics_stochastic_processes_finance", "Mathematics — Stochastic Processes & Mathematical Finance", "mathematics", "Mathematics", Subfield,
        "Stochastic processes, stochastic calculus, interacting particle systems, stochastic control, mathematical finance, or rigorous random dynamics.",
        "the contribution is statistical inference from stochastic data or an applied financial model without a substantive mathematical result.", MATHEMATICS
    ),
    subject!(
        "subject_mathematics_mathematical_physics", "Mathematics — Mathematical Physics", "mathematics", "Mathematics", Subfield,
        "Rigorous mathematical physics, spectral and scattering theory, quantum many-body systems, integrable systems, statistical mechanics, or mathematically controlled physical limits.",
        "the work is primarily a physics model with formal calculations but no mathematical theorem or rigorous structural contribution.", MATHEMATICS
    ),

    // Statistics
    subject!(
        "subject_statistics_general", "Statistics — General", "statistics", "Statistics", Discipline,
        "Statistical methodology spanning several areas or not covered by a narrower statistics specialist.",
        "statistics is only an application tool and another substantive discipline owns the contribution.", STATISTICS
    ),
    subject!(
        "subject_statistics_theory", "Statistics — Theory & Asymptotics", "statistics", "Statistics", Subfield,
        "Decision theory, minimax analysis, asymptotic theory, nonparametrics, or foundational statistical methodology.",
        "the paper primarily applies established theory to one empirical domain.", STATISTICS
    ),
    subject!(
        "subject_statistics_bayesian", "Statistics — Bayesian", "statistics", "Statistics", Subfield,
        "Bayesian modeling, posterior theory, probabilistic programming, prior construction, or Bayesian computation.",
        "Bayesian software is used routinely but no Bayesian modeling or inferential contribution is made.", STATISTICS
    ),
    subject!(
        "subject_statistics_causal_semiparametric", "Statistics — Causal & Semiparametric", "statistics", "Statistics", Subfield,
        "Causal estimands, semiparametric efficiency, missing data, treatment effects, or robust causal methodology as the contribution.",
        "causal identification is only an application and the methodological contribution is not statistical.", STATISTICS
    ),
    subject!(
        "subject_statistics_highdim_learning", "Statistics — High-Dimensional & Learning", "statistics", "Statistics", Subfield,
        "High-dimensional inference, sparsity, statistical learning theory, prediction, or modern nonparametrics.",
        "the central contribution is a computer-science algorithm or application benchmark rather than statistical understanding.", STATISTICS
    ),
    subject!(
        "subject_statistics_time_series", "Statistics — Time Series", "statistics", "Statistics", Subfield,
        "Temporal dependence, forecasting, state-space models, longitudinal stochastic processes, or frequency-domain methods.",
        "time appears only as a fixed covariate or panel index with no temporal methodological issue.", STATISTICS
    ),
    subject!(
        "subject_statistics_spatial", "Statistics — Spatial", "statistics", "Statistics", Subfield,
        "Spatial statistics, point processes, geostatistics, spatial fields, or areal data methodology.",
        "space is merely a location label and the paper has no spatial model or spatial inferential contribution.", STATISTICS
    ),
    subject!(
        "subject_statistics_survival_biostat", "Statistics — Survival & Biostatistics", "statistics", "Statistics", Subfield,
        "Survival, event-history, competing-risk, longitudinal biomedical, diagnostic, or clinical statistical methodology.",
        "clinical substance dominates and the statistical methods are entirely standard.", STATISTICS
    ),
    subject!(
        "subject_statistics_design_sampling", "Statistics — Design & Sampling", "statistics", "Statistics", Subfield,
        "Experimental design, adaptive design, survey sampling, randomization theory, or finite-population inference.",
        "the design is an application detail and no design or sampling methodology is contributed.", STATISTICS
    ),
    subject!(
        "subject_statistics_psychometrics_computational", "Statistics — Latent Variables & Computation", "statistics", "Statistics", Subfield,
        "Psychometrics, latent-variable models, item response, mixture models, or statistical computation as a methodological contribution.",
        "latent constructs or computation are routine components of a primarily substantive application.", STATISTICS
    ),
    subject!(
        "subject_statistics_nonparametric_robust", "Statistics — Nonparametric & Robust Methods", "statistics", "Statistics", Subfield,
        "Nonparametric and semiparametric estimation outside causal work, robust statistics, rank methods, smoothing, density estimation, or inference under contamination and weak distributional assumptions.",
        "a conventional parametric model is central and robustness or nonparametric adaptation is only a minor sensitivity check.", STATISTICS
    ),
    subject!(
        "subject_statistics_multivariate_functional", "Statistics — Multivariate & Functional Data", "statistics", "Statistics", Subfield,
        "Multivariate analysis, covariance and precision estimation, dimension reduction, functional data, tensor data, compositional data, or repeated high-structure measurements.",
        "the main contribution is generic machine-learning prediction or a low-dimensional regression with several outcomes.", STATISTICS
    ),
    subject!(
        "subject_statistics_experimental_design", "Statistics — Experimental Design & Adaptive Trials", "statistics", "Statistics", Subfield,
        "Optimal and sequential design, adaptive randomization, response-adaptive trials, factorial and fractional designs, bandit experiments, or design-based efficiency theory.",
        "a standard randomized experiment whose assignment mechanism is fixed and not itself a statistical contribution.", STATISTICS
    ),

    // Computer science
    subject!(
        "subject_computer_science_general", "Computer Science — General", "computer_science", "Computer Science", Discipline,
        "Computer-science research spanning several areas or outside the listed subfields.",
        "a listed computer-science subfield clearly owns the central artifact or theorem.", COMPUTER_SCIENCE
    ),
    subject!(
        "subject_computer_science_algorithms", "Computer Science — Algorithms & Complexity", "computer_science", "Computer Science", Subfield,
        "Algorithms, data structures, complexity theory, approximation, online algorithms, or theoretical computer science.",
        "the algorithm is a routine implementation device for an applied system or statistical model.", COMPUTER_SCIENCE
    ),
    subject!(
        "subject_computer_science_ai_ml", "Computer Science — Machine Learning & AI (General)", "computer_science", "Computer Science", Subfield,
        "Machine learning and AI methodology spanning several areas or outside the listed AI subfields: learning theory, optimization for learning, probabilistic modeling, generative methods, or general model classes.",
        "standard machine learning is used only to measure a substantive phenomenon in another field, or a listed AI subfield — LLMs and foundation models, reinforcement learning, AI safety and evaluation, or ML systems — clearly carries the contribution.", COMPUTER_SCIENCE
    ),
    subject!(
        "subject_computer_science_llm_foundation", "Computer Science — LLMs & Foundation Models", "computer_science", "Computer Science", Subfield,
        "Large language models, foundation models, multimodal generative systems, their training, adaptation, alignment techniques, or capabilities.",
        "general machine-learning methodology where foundation models are only an example, or social-science text analysis using an LLM as a measurement tool.", COMPUTER_SCIENCE
    ),
    subject!(
        "subject_computer_science_reinforcement_learning", "Computer Science — Reinforcement Learning & Decision-Making", "computer_science", "Computer Science", Subfield,
        "Reinforcement learning, sequential decision-making, bandits, learned planning, or agentic control as the central contribution.",
        "supervised or generative learning without a sequential decision problem, or classical control theory without learning.", COMPUTER_SCIENCE
    ),
    subject!(
        "subject_computer_science_ai_safety_evaluation", "Computer Science — AI Safety, Alignment & Evaluation", "computer_science", "Computer Science", Subfield,
        "AI safety, alignment, interpretability, model-evaluation science, red-teaming, or measurement of AI-system risk.",
        "general AI capability work, or AI policy discussion without a technical safety or evaluation contribution.", COMPUTER_SCIENCE
    ),
    subject!(
        "subject_computer_science_ml_systems", "Computer Science — ML Systems & Efficiency", "computer_science", "Computer Science", Subfield,
        "Systems for machine learning: training and inference infrastructure, efficiency, compression, hardware acceleration, or distributed and federated learning.",
        "an ML algorithm whose systems footprint is incidental, or general distributed systems without a learning workload.", COMPUTER_SCIENCE
    ),
    subject!(
        "subject_computer_science_nlp", "Computer Science — Natural-Language Processing", "computer_science", "Computer Science", Subfield,
        "Natural-language processing, computational linguistics systems, language models, or text generation and evaluation.",
        "texts are analyzed as social or historical evidence and no NLP method is contributed.", COMPUTER_SCIENCE
    ),
    subject!(
        "subject_computer_science_vision_graphics", "Computer Science — Vision & Graphics", "computer_science", "Computer Science", Subfield,
        "Computer vision, image/video understanding, computer graphics, rendering, or visual computing.",
        "imaging is chiefly a scientific measurement instrument or clinical diagnostic rather than a visual-computing contribution.", COMPUTER_SCIENCE
    ),
    subject!(
        "subject_computer_science_systems", "Computer Science — Systems & Networks", "computer_science", "Computer Science", Subfield,
        "Operating, distributed, cloud, storage, networking, mobile, or high-performance systems.",
        "the contribution is a hardware circuit or an application whose systems layer is conventional.", COMPUTER_SCIENCE
    ),
    subject!(
        "subject_computer_science_programming_languages", "Computer Science — Programming Languages & Formal Methods", "computer_science", "Computer Science", Subfield,
        "Programming languages, type systems, compilers, semantics, program verification, or formal methods.",
        "formal proof concerns a mathematical theorem rather than a programming-language or software property.", COMPUTER_SCIENCE
    ),
    subject!(
        "subject_computer_science_databases", "Computer Science — Databases & Data Management", "computer_science", "Computer Science", Subfield,
        "Databases, query processing, transactions, data integration, knowledge bases, or data-management systems.",
        "a dataset is created for scientific analysis but no data-management contribution is made.", COMPUTER_SCIENCE
    ),
    subject!(
        "subject_computer_science_security", "Computer Science — Security & Privacy", "computer_science", "Computer Science", Subfield,
        "Computer security, cryptography applications, privacy, adversarial robustness, or usable security.",
        "security appears only as motivation and no threat, attack, defense, or privacy claim is evaluated.", COMPUTER_SCIENCE
    ),
    subject!(
        "subject_computer_science_hci", "Computer Science — Human-Computer Interaction", "computer_science", "Computer Science", Subfield,
        "Human-computer interaction, CSCW, information visualization, accessibility, or interactive-system research.",
        "humans appear only as annotators or users of a system whose contribution is otherwise algorithmic.", COMPUTER_SCIENCE
    ),
    subject!(
        "subject_computer_science_robotics", "Computer Science — Robotics & Autonomous Systems", "computer_science", "Computer Science", Subfield,
        "Robotics, autonomous agents, planning, control software, embodied AI, or multi-agent systems.",
        "the primary contribution is mechanical hardware or control theory without a computational autonomy claim.", COMPUTER_SCIENCE
    ),
    subject!(
        "subject_computer_science_software", "Computer Science — Software Engineering", "computer_science", "Computer Science", Subfield,
        "Software engineering, testing, debugging, program analysis, development tools, repositories, or empirical software research.",
        "custom research code is merely an implementation artifact and no software-engineering claim is made.", COMPUTER_SCIENCE
    ),
    subject!(
        "subject_computer_science_distributed_systems", "Computer Science — Distributed & Cloud Systems", "computer_science", "Computer Science", Subfield,
        "Distributed systems, cloud platforms, consensus, replication, distributed storage, fault tolerance, edge systems, or large-scale service infrastructure.",
        "the contribution is a local operating-system or networking mechanism without distributed state, failure, or coordination semantics.", COMPUTER_SCIENCE
    ),
    subject!(
        "subject_computer_science_information_retrieval", "Computer Science — Information Retrieval & Data Mining", "computer_science", "Computer Science", Subfield,
        "Search, ranking, recommendation, information retrieval, data mining, graph mining, or user-item discovery systems.",
        "the contribution is database query processing, NLP generation, or a substantive text-as-data analysis without a retrieval or discovery objective.", COMPUTER_SCIENCE
    ),
    subject!(
        "subject_computer_science_scientific_computing", "Computer Science — Scientific Computing & Computational Science", "computer_science", "Computer Science", Subfield,
        "Computational science systems, scientific workflows, high-performance scientific software, domain-specific simulation infrastructure, or computational methods whose systems contribution serves science.",
        "the contribution is a mathematical numerical method, a domain-science result using standard code, or generic high-performance systems without a scientific-computing claim.", COMPUTER_SCIENCE
    ),

    // Engineering
    subject!(
        "subject_engineering_general", "Engineering — General", "engineering", "Engineering", Discipline,
        "Engineering research spanning several systems or outside the listed engineering subfields.",
        "a listed engineering specialty clearly owns the design and performance claim.", ENGINEERING
    ),
    subject!(
        "subject_engineering_electrical_computer", "Engineering — Electrical & Computer", "engineering", "Engineering", Subfield,
        "Circuits, electronics, communications, signal processing, embedded systems, or computer hardware.",
        "the contribution is primarily a computer-science system or a physical-material mechanism.", ENGINEERING
    ),
    subject!(
        "subject_engineering_mechanical", "Engineering — Mechanical", "engineering", "Engineering", Subfield,
        "Mechanical design, mechanics, thermofluids, heat transfer, tribology, or mechanical systems.",
        "the main result is fundamental fluid physics or materials science without an engineered design claim.", ENGINEERING
    ),
    subject!(
        "subject_engineering_aerospace", "Engineering — Aerospace", "engineering", "Engineering", Subfield,
        "Aeronautical, astronautical, propulsion, flight, spacecraft, or aerospace-systems research.",
        "the work concerns atmospheric or plasma physics with no vehicle or mission design objective.", ENGINEERING
    ),
    subject!(
        "subject_engineering_civil", "Engineering — Civil, Structural & Transportation", "engineering", "Engineering", Subfield,
        "Civil, structural, geotechnical, construction, infrastructure, or transportation engineering.",
        "the paper is primarily urban social science, economics, or descriptive geology.", ENGINEERING
    ),
    subject!(
        "subject_engineering_chemical_process", "Engineering — Chemical & Process", "engineering", "Engineering", Subfield,
        "Chemical engineering, reactors, separations, catalysis processes, process systems, or scale-up.",
        "the central contribution is molecular chemistry without a process or transport claim.", ENGINEERING
    ),
    subject!(
        "subject_engineering_materials", "Engineering — Materials", "engineering", "Engineering", Subfield,
        "Materials engineering, metallurgy, ceramics, polymers, composites, processing, or performance design.",
        "the work is fundamental condensed-matter physics or synthetic chemistry without an engineering property target.", ENGINEERING
    ),
    subject!(
        "subject_engineering_biomedical", "Engineering — Biomedical", "engineering", "Engineering", Subfield,
        "Medical devices, biomaterials, tissue engineering, biosensors, biomechanics, or biomedical systems.",
        "the primary claim is clinical efficacy or basic biology rather than an engineered biomedical artifact.", ENGINEERING
    ),
    subject!(
        "subject_engineering_environmental_energy", "Engineering — Environmental & Energy", "engineering", "Engineering", Subfield,
        "Environmental engineering, energy conversion and storage, water treatment, emissions control, or sustainable systems.",
        "the paper is climate or environmental science without a treatment, conversion, or systems-design contribution.", ENGINEERING
    ),
    subject!(
        "subject_engineering_industrial_control", "Engineering — Industrial, Control & Manufacturing", "engineering", "Engineering", Subfield,
        "Industrial engineering, systems engineering, control, operations, manufacturing, automation, or reliability.",
        "the main contribution is abstract optimization or a robot-learning algorithm without an industrial system claim.", ENGINEERING
    ),
    subject!(
        "subject_engineering_robotics_mechatronics", "Engineering — Robotics & Mechatronics", "engineering", "Engineering", Subfield,
        "Robotic hardware, mechatronic systems, actuators, manipulation, locomotion, human-robot systems, or integrated sensing-control design.",
        "the principal contribution is autonomous planning or robot learning with standard hardware, or a mechanical component with no integrated robotic function.", ENGINEERING
    ),
    subject!(
        "subject_engineering_nuclear", "Engineering — Nuclear Engineering", "engineering", "Engineering", Subfield,
        "Nuclear fission or fusion systems, reactor physics and thermal hydraulics, radiation engineering, fuel cycles, shielding, or nuclear materials and safety.",
        "the contribution is fundamental particle or plasma physics without a reactor, radiation, fuel, or engineered nuclear-system objective.", ENGINEERING
    ),
    subject!(
        "subject_engineering_power_energy_systems", "Engineering — Power & Energy Systems", "engineering", "Engineering", Subfield,
        "Electric power systems, grids, power electronics, renewable integration, energy storage systems, microgrids, or energy-system operation and control.",
        "the contribution is an electrochemical material, environmental life-cycle study, or electricity-market model without a central power-system engineering claim.", ENGINEERING
    ),

    // Biological sciences
    subject!(
        "subject_biology_general", "Biology — General", "biology", "Biological Science", Discipline,
        "Biological research spanning several levels of organization or outside the listed biological subfields.",
        "a listed biological specialty clearly contains the mechanism and evidence.", BIOLOGY
    ),
    subject!(
        "subject_biology_molecular_cell", "Biology — Molecular & Cell", "biology", "Biological Science", Subfield,
        "Molecular biology, cell biology, cell signaling, organelles, trafficking, or cellular mechanisms.",
        "the primary contribution is organismal physiology, ecology, or a clinical endpoint.", BIOLOGY
    ),
    subject!(
        "subject_biology_genetics_genomics", "Biology — Genetics & Genomics", "biology", "Biological Science", Subfield,
        "Genetics, genomics, epigenomics, population genetics, genome regulation, or functional genomics.",
        "sequence data are only a measurement input and no genetic or genomic claim is central.", BIOLOGY
    ),
    subject!(
        "subject_biology_biochemistry_structural", "Biology — Biochemistry & Structural", "biology", "Biological Science", Subfield,
        "Biochemistry, enzymology, metabolism, structural biology, biophysics of macromolecules, or molecular interactions.",
        "the contribution is synthetic chemistry or materials characterization rather than biological molecular function.", BIOLOGY
    ),
    subject!(
        "subject_biology_development_neuroscience", "Biology — Development & Neuroscience", "biology", "Biological Science", Subfield,
        "Developmental biology, stem cells, or neural development within a broad biological program.",
        "the main contribution is psychological behavior without a biological mechanism, or mechanistic neuroscience is the central program, which the neuroscience discipline reviews.", BIOLOGY
    ),
    subject!(
        "subject_biology_physiology", "Biology — Physiology", "biology", "Biological Science", Subfield,
        "Organismal, comparative, integrative, endocrine, cardiovascular, respiratory, or metabolic physiology.",
        "the paper is primarily clinical medicine or cell biology without an organism-level functional claim.", BIOLOGY
    ),
    subject!(
        "subject_biology_microbiology_immunology", "Biology — Microbiology, Virology & Immunology", "biology", "Biological Science", Subfield,
        "Microbiology, virology, host-pathogen biology, immunology, or microbial communities.",
        "the central claim is population epidemiology without a microbial or immune mechanism.", BIOLOGY
    ),
    subject!(
        "subject_biology_ecology_evolution", "Biology — Ecology & Evolution", "biology", "Biological Science", Subfield,
        "Ecology, evolutionary biology, behavior, population biology, community ecology, or macroevolution.",
        "the work is environmental monitoring without an ecological or evolutionary inference.", BIOLOGY
    ),
    subject!(
        "subject_biology_systems_computational", "Biology — Systems & Computational", "biology", "Biological Science", Subfield,
        "Systems biology, bioinformatics, computational biology, network biology, or multi-omics integration.",
        "the principal contribution is a general computer-science algorithm with biology only as a benchmark.", BIOLOGY
    ),
    subject!(
        "subject_biology_organismal_behavior", "Biology — Organismal, Integrative & Behavior", "biology", "Biological Science", Subfield,
        "Zoology, organismal biology, comparative anatomy, functional morphology, animal behavior, or integrative biology.",
        "the central contribution is cellular physiology, ecology, or human psychology rather than whole-organism biological function.", BIOLOGY
    ),
    subject!(
        "subject_biology_plant_marine_conservation", "Biology — Plant, Marine & Conservation", "biology", "Biological Science", Subfield,
        "Plant science, marine biology, conservation biology, biodiversity, or applied organismal ecology.",
        "the central contribution is agricultural engineering or environmental policy rather than organismal biology.", BIOLOGY
    ),

    // Chemistry
    subject!(
        "subject_chemistry_general", "Chemistry — General", "chemistry", "Chemistry", Discipline,
        "Chemical research spanning several areas or outside the listed chemistry subfields.",
        "a listed chemistry specialty clearly contains the main transformation, measurement, or molecular claim.", CHEMISTRY
    ),
    subject!(
        "subject_chemistry_organic_biological", "Chemistry — Organic & Chemical Biology", "chemistry", "Chemistry", Subfield,
        "Organic synthesis, reaction methodology, catalysis, medicinal chemistry, or chemical biology.",
        "the main result is a biological mechanism with standard chemical probes or an industrial process scale-up.", CHEMISTRY
    ),
    subject!(
        "subject_chemistry_inorganic_materials", "Chemistry — Inorganic & Materials", "chemistry", "Chemistry", Subfield,
        "Inorganic, organometallic, solid-state, coordination, or materials chemistry.",
        "the contribution is primarily device engineering or condensed-matter physics rather than chemical composition and bonding.", CHEMISTRY
    ),
    subject!(
        "subject_chemistry_physical_theoretical", "Chemistry — Physical & Theoretical", "chemistry", "Chemistry", Subfield,
        "Physical chemistry, spectroscopy, chemical dynamics, quantum chemistry, statistical chemistry, or theoretical chemistry.",
        "the contribution is a general physics theory or numerical method without a chemical question.", CHEMISTRY
    ),
    subject!(
        "subject_chemistry_analytical_environmental", "Chemistry — Analytical & Environmental", "chemistry", "Chemistry", Subfield,
        "Analytical chemistry, separations, sensors, mass spectrometry, electrochemistry, or environmental chemistry.",
        "measurement is routine and the substantive contribution lies entirely in another scientific field.", CHEMISTRY
    ),

    // Earth and environmental sciences
    subject!(
        "subject_earth_environment_general", "Earth & Environment — General", "earth_environment", "Earth & Environmental Science", Discipline,
        "Earth, planetary, or environmental research spanning several systems or outside the listed specialties.",
        "a listed Earth or environmental subfield clearly owns the principal process and evidence.", EARTH_ENVIRONMENT
    ),
    subject!(
        "subject_earth_environment_geology", "Earth & Environment — Geology & Geochemistry", "earth_environment", "Earth & Environmental Science", Subfield,
        "Geology, geochemistry, geomorphology, paleoclimate proxies, sedimentology, or tectonics.",
        "the central contribution is a physical geophysics inverse problem or modern atmospheric process.", EARTH_ENVIRONMENT
    ),
    subject!(
        "subject_earth_environment_climate_atmosphere", "Earth & Environment — Climate & Atmosphere", "earth_environment", "Earth & Environmental Science", Subfield,
        "Climate science, meteorology, atmospheric chemistry, weather, or Earth-system dynamics.",
        "the paper concerns policy impacts without a material climate or atmospheric scientific contribution, or paleoclimate reconstruction, biogeochemical cycling, or climate-impact assessment carries the contribution.", EARTH_ENVIRONMENT
    ),
    subject!(
        "subject_earth_environment_paleoclimate", "Earth & Environment — Paleoclimate & Reconstruction", "earth_environment", "Earth & Environmental Science", Subfield,
        "Paleoclimate, paleoceanography, or past environmental reconstruction from proxies, cores, and archives.",
        "modern instrumental climate analysis, or archaeological environmental history reviewed by its own field.", EARTH_ENVIRONMENT
    ),
    subject!(
        "subject_earth_environment_biogeochemistry", "Earth & Environment — Biogeochemistry & Carbon Cycle", "earth_environment", "Earth & Environmental Science", Subfield,
        "Biogeochemical cycles, carbon and nutrient fluxes, greenhouse-gas budgets, or land and ocean carbon-sink dynamics.",
        "ecosystem ecology without an elemental-cycle claim, or a purely atmospheric-physics contribution.", EARTH_ENVIRONMENT
    ),
    subject!(
        "subject_earth_environment_climate_impacts", "Earth & Environment — Climate Impacts & Adaptation", "earth_environment", "Earth & Environmental Science", Subfield,
        "Climate-change impacts, vulnerability, adaptation, or climate-risk assessment for ecosystems, resources, or society.",
        "physical climate dynamics without an impact pathway, or economic policy analysis where the climate component is only an input.", EARTH_ENVIRONMENT
    ),
    subject!(
        "subject_earth_environment_ocean_hydrology", "Earth & Environment — Ocean & Hydrology", "earth_environment", "Earth & Environmental Science", Subfield,
        "Oceanography, hydrology, cryosphere, limnology, groundwater, or watershed science.",
        "water is only an engineering input or the central mechanism is atmospheric rather than oceanic or hydrologic.", EARTH_ENVIRONMENT
    ),
    subject!(
        "subject_earth_environment_sustainability", "Earth & Environment — Ecology & Sustainability", "earth_environment", "Earth & Environmental Science", Subfield,
        "Environmental science, biogeochemistry, pollution, ecosystem services, sustainability, or coupled human-natural systems.",
        "the contribution is primarily ecological biology, engineering treatment, or policy evaluation.", EARTH_ENVIRONMENT
    ),

    // Medicine and health
    subject!(
        "subject_medicine_health_general", "Medicine & Health — General", "medicine_health", "Medicine & Health", Discipline,
        "Medical, clinical, or health research spanning several specialties or outside the listed health subfields.",
        "a listed clinical, epidemiological, diagnostic, therapeutic, or health-systems lens clearly fits.", MEDICINE_HEALTH
    ),
    subject!(
        "subject_medicine_health_clinical", "Medicine & Health — Clinical", "medicine_health", "Medicine & Health", Subfield,
        "Clinical observational research, prognosis, treatment outcomes, patient management, or specialty medicine.",
        "the contribution is a formal trial, diagnostic technology, or population-health study better covered below.", MEDICINE_HEALTH
    ),
    subject!(
        "subject_medicine_health_epidemiology", "Medicine & Health — Epidemiology & Public Health", "medicine_health", "Medicine & Health", Subfield,
        "Epidemiology, population health, prevention, infectious-disease spread, environmental health, or health disparities.",
        "the paper is a patient-level clinical efficacy study or a biological transmission mechanism without population inference.", MEDICINE_HEALTH
    ),
    subject!(
        "subject_medicine_health_trials", "Medicine & Health — Trials & Therapeutics", "medicine_health", "Medicine & Health", Subfield,
        "Clinical trials, therapeutic development, comparative treatment, dosing, or intervention efficacy and safety.",
        "the intervention is nonclinical or the study is purely observational with no trial design.", MEDICINE_HEALTH
    ),
    subject!(
        "subject_medicine_health_diagnostics", "Medicine & Health — Diagnostics & Imaging", "medicine_health", "Medicine & Health", Subfield,
        "Diagnostic tests, biomarkers, pathology, medical imaging, screening, prognostic models, or clinical decision support.",
        "the main contribution is an imaging algorithm without a clinical diagnostic claim.", MEDICINE_HEALTH
    ),
    subject!(
        "subject_medicine_health_services", "Medicine & Health — Services & Global Health", "medicine_health", "Medicine & Health", Subfield,
        "Health services, implementation, delivery systems, quality, cost, access, policy, or global health.",
        "the central contribution is a biomedical treatment effect under tightly controlled clinical conditions.", MEDICINE_HEALTH
    ),
    subject!(
        "subject_medicine_health_cardiometabolic", "Medicine & Health — Cardiovascular, Metabolic & Renal", "medicine_health", "Medicine & Health", Subfield,
        "Cardiovascular medicine, endocrinology and diabetes, obesity and metabolism, nephrology, hypertension, or integrated cardiometabolic and renal disease.",
        "the central contribution is basic physiology or a population risk association without a patient-facing cardiometabolic diagnosis, prognosis, prevention, or treatment claim.", MEDICINE_HEALTH
    ),
    subject!(
        "subject_medicine_health_oncology", "Medicine & Health — Oncology & Hematology", "medicine_health", "Medicine & Health", Subfield,
        "Cancer and hematologic disease, screening, molecular classification, systemic therapy, radiation, surgery, prognosis, or survivorship.",
        "the main contribution is basic tumor biology or a general diagnostic algorithm without an intended oncologic decision or patient outcome.", MEDICINE_HEALTH
    ),
    subject!(
        "subject_medicine_health_neurology_mental", "Medicine & Health — Neurology, Psychiatry & Mental Health", "medicine_health", "Medicine & Health", Subfield,
        "Neurologic and psychiatric disease, neurodevelopment, mental health, addiction, cognition, behavioral treatment, or clinical neuroscience.",
        "the contribution is basic neural mechanism or general psychology without a diagnosis, impairment, prognosis, prevention, or clinical-care claim.", MEDICINE_HEALTH
    ),

    // Sociology
    subject!(
        "subject_sociology_general", "Sociology — General", "sociology", "Sociology", Discipline,
        "Sociological research spanning several areas or outside the listed sociological subfields.",
        "a listed sociological subfield clearly contains the principal social process.", SOCIOLOGY
    ),
    subject!(
        "subject_sociology_inequality_demography", "Sociology — Inequality & Demography", "sociology", "Sociology", Subfield,
        "Social stratification, mobility, inequality, family, life course, or population change as social structure.",
        "the central contribution is a labor or public-economics estimate without a sociological account of structure or group process, or formal demographic rates, methods, or projections carry the contribution.", SOCIOLOGY
    ),
    subject!(
        "subject_sociology_demography_population", "Sociology — Demography & Population", "sociology", "Sociology", Subfield,
        "Formal demography and population studies: fertility, mortality, migration flows, population projection, or demographic methods.",
        "inequality and stratification analysis where population composition is context rather than the object of study.", SOCIOLOGY
    ),
    subject!(
        "subject_sociology_organizations_economic", "Sociology — Organizations & Economic Life", "sociology", "Sociology", Subfield,
        "Organizations, occupations, professions, markets, work, firms, economic sociology, or institutional fields.",
        "the paper models firms or markets without a sociological organizational or relational contribution.", SOCIOLOGY
    ),
    subject!(
        "subject_sociology_political_movements", "Sociology — Political & Social Movements", "sociology", "Sociology", Subfield,
        "Political sociology, states, citizenship, collective action, protest, social movements, or power.",
        "the contribution is primarily electoral behavior, formal institutions, or international relations without a sociological mechanism.", SOCIOLOGY
    ),
    subject!(
        "subject_sociology_race_migration", "Sociology — Race, Ethnicity & Migration", "sociology", "Sociology", Subfield,
        "Race, ethnicity, indigeneity, immigration, citizenship, assimilation, boundaries, or transnational communities.",
        "group categories are incidental controls and no racial, ethnic, migration, or boundary process is central.", SOCIOLOGY
    ),
    subject!(
        "subject_sociology_culture_media", "Sociology — Culture & Media", "sociology", "Sociology", Subfield,
        "Culture, meaning, classification, knowledge, religion, media, consumption, or cultural production.",
        "the paper is primarily textual interpretation without a sociological claim about actors, institutions, or social distribution.", SOCIOLOGY
    ),
    subject!(
        "subject_sociology_networks", "Sociology — Social Networks", "sociology", "Sociology", Subfield,
        "Social networks, diffusion, relational inequality, social capital, peer structure, or network organizations.",
        "graphs are purely technological or biological and social relations are not the substantive object.", SOCIOLOGY
    ),
    subject!(
        "subject_sociology_urban_community", "Sociology — Urban & Community", "sociology", "Sociology", Subfield,
        "Urban sociology, neighborhoods, housing, place, communities, segregation, or local institutions.",
        "the main contribution is urban economics, planning engineering, or geography without a sociological community process.", SOCIOLOGY
    ),
    subject!(
        "subject_sociology_medical_crime", "Sociology — Health, Medicine & Crime", "sociology", "Sociology", Subfield,
        "Medical sociology, health inequality, professions and care, criminology, punishment, law, or deviance.",
        "clinical efficacy, epidemiology, or legal doctrine is central without a sociological institution or inequality claim.", SOCIOLOGY
    ),
    subject!(
        "subject_sociology_family_life_course", "Sociology — Family & Life Course", "sociology", "Sociology", Subfield,
        "Family formation and change, households, marriage, fertility, parenting, kinship, aging, intergenerational relations, or life-course transitions.",
        "family variables are only demographic controls, or the contribution is a household economic model without a sociological institution, relationship, or meaning structure.", SOCIOLOGY
    ),
    subject!(
        "subject_sociology_crime_punishment", "Sociology — Crime, Law & Punishment", "sociology", "Sociology", Subfield,
        "Criminology, policing, courts and punishment, incarceration, surveillance, deviance, legal consciousness, or social control.",
        "the central claim is legal doctrine, individual clinical pathology, or a policy effect without a sociological account of institutions, inequality, or control.", SOCIOLOGY
    ),
    subject!(
        "subject_sociology_science_knowledge_professions", "Sociology — Science, Knowledge & Professions", "sociology", "Sociology", Subfield,
        "Sociology of science and knowledge, expertise, professions, classification, evaluation, intellectual fields, or knowledge-producing organizations.",
        "the contribution is philosophy or history of science without a sociological claim about actors, institutions, authority, or social organization.", SOCIOLOGY
    ),

    // Political science
    subject!(
        "subject_political_science_general", "Political Science — General", "political_science", "Political Science", Discipline,
        "Political research spanning several areas or outside the listed political-science subfields.",
        "a listed political-science subfield clearly contains the actors, institution, or outcome.", POLITICAL_SCIENCE
    ),
    subject!(
        "subject_political_science_comparative", "Political Science — Comparative Politics", "political_science", "Political Science", Subfield,
        "Comparative institutions, regimes, democratization, parties, state capacity, governance, or political development.",
        "the paper concerns international interactions or one policy without comparative institutional inference.", POLITICAL_SCIENCE
    ),
    subject!(
        "subject_political_science_ir_security", "Political Science — International Relations & Security", "political_science", "Political Science", Subfield,
        "International relations, security, war, alliances, diplomacy, international organizations, or foreign policy.",
        "cross-border economic exchange is central but strategic international politics is not.", POLITICAL_SCIENCE
    ),
    subject!(
        "subject_political_science_ipe", "Political Science — International Political Economy", "political_science", "Political Science", Subfield,
        "Trade politics, international finance, sanctions, development institutions, globalization, or cross-border political economy.",
        "the contribution is a purely economic trade or finance result without a political institution or distributional mechanism.", POLITICAL_SCIENCE
    ),
    subject!(
        "subject_political_science_institutions", "Political Science — Institutions & Governance", "political_science", "Political Science", Subfield,
        "Legislatures, executives, courts, bureaucracy, federalism, constitutions, corruption, or governance.",
        "the central contribution is legal doctrine or organizational sociology without a political institutional claim.", POLITICAL_SCIENCE
    ),
    subject!(
        "subject_political_science_behavior_elections", "Political Science — Behavior, Opinion & Elections", "political_science", "Political Science", Subfield,
        "Political behavior, public opinion, voting, campaigns, parties, representation, or political communication.",
        "the paper studies general social attitudes without a material political behavior or representation claim.", POLITICAL_SCIENCE
    ),
    subject!(
        "subject_political_science_theory", "Political Science — Political Theory", "political_science", "Political Science", Subfield,
        "Normative, analytic, historical, or critical political theory concerning justice, authority, democracy, liberty, or power.",
        "the central contribution is empirical political behavior or general moral philosophy without a political institutional object.", POLITICAL_SCIENCE
    ),
    subject!(
        "subject_political_science_policy_admin", "Political Science — Public Policy & Administration", "political_science", "Political Science", Subfield,
        "Policy design, implementation, bureaucracy, regulation, public administration, or program governance.",
        "the paper estimates a program effect without a material claim about political design, administration, or implementation.", POLITICAL_SCIENCE
    ),
    subject!(
        "subject_political_science_conflict_environment", "Political Science — Conflict & Environmental Politics", "political_science", "Political Science", Subfield,
        "Civil conflict, peacebuilding, repression, political violence, resource politics, or environmental governance.",
        "the paper is chiefly climate science, criminology, or international war without the relevant domestic conflict or governance mechanism.", POLITICAL_SCIENCE
    ),
    subject!(
        "subject_political_science_methodology", "Political Science — Political Methodology", "political_science", "Political Science", Subfield,
        "Political methodology, measurement and scaling, research design, formal-empirical integration, text or survey methodology, or new statistical tools for political data.",
        "established methods are merely applied to a substantive political question and no methodological contribution is claimed.", POLITICAL_SCIENCE
    ),
    subject!(
        "subject_political_science_domestic_political_economy", "Political Science — Domestic Political Economy", "political_science", "Political Science", Subfield,
        "Domestic distributive politics, taxation and redistribution, business-state relations, regulation, labor and welfare politics, or political determinants of economic policy.",
        "the main contribution is international political economy or a purely economic incidence result without political actors, institutions, or coalition formation.", POLITICAL_SCIENCE
    ),
    subject!(
        "subject_political_science_identity_representation", "Political Science — Identity, Representation & Citizenship", "political_science", "Political Science", Subfield,
        "Political representation and participation organized around race, ethnicity, gender, religion, indigeneity, migration, citizenship, or group identity.",
        "identity is only a demographic subgroup or the contribution is sociological boundary formation without a material political institution, behavior, or representation claim.", POLITICAL_SCIENCE
    ),

    // History
    subject!(
        "subject_history_general", "History — General", "history", "History", Discipline,
        "Historical scholarship spanning several periods or themes or outside the listed historical subfields.",
        "a listed period or thematic history specialist clearly fits the main intervention.", HISTORY
    ),
    subject!(
        "subject_history_ancient", "History — Ancient", "history", "History", Subfield,
        "Ancient Mediterranean, Near Eastern, African, Asian, American, or other pre-medieval history.",
        "the central evidence and historiography belong to a later period.", HISTORY
    ),
    subject!(
        "subject_history_medieval", "History — Medieval", "history", "History", Subfield,
        "Medieval history across regions, including institutions, religion, economy, society, and material culture.",
        "the paper is primarily ancient or early-modern and does not turn on medieval periodization.", HISTORY
    ),
    subject!(
        "subject_history_early_modern", "History — Early Modern", "history", "History", Subfield,
        "Early-modern state formation, empire, religion, science, commerce, culture, or social change.",
        "the central intervention belongs clearly to medieval or modern historiography.", HISTORY
    ),
    subject!(
        "subject_history_modern", "History — Modern & Contemporary", "history", "History", Subfield,
        "Modern or contemporary history, including industrialization, nation-states, colonialism, war, and mass politics.",
        "the paper's intervention is primarily a social-science analysis of current outcomes rather than historical explanation.", HISTORY
    ),
    subject!(
        "subject_history_economic", "History — Economic & Business", "history", "History", Subfield,
        "Economic, business, labor, financial, technological, or quantitative history.",
        "the contribution is principally an economics estimate with little historiographic or source-based historical argument.", HISTORY
    ),
    subject!(
        "subject_history_political_diplomatic", "History — Political, Diplomatic & Military", "history", "History", Subfield,
        "Political, diplomatic, legal-institutional, military, state, or international history.",
        "the paper is an international-relations model using historical cases without a primary historical intervention.", HISTORY
    ),
    subject!(
        "subject_history_social_cultural", "History — Social, Cultural & Gender", "history", "History", Subfield,
        "Social, cultural, gender, race, family, everyday-life, or subaltern history.",
        "culture or inequality is analyzed without a historical source base or historiographic contribution.", HISTORY
    ),
    subject!(
        "subject_history_intellectual_global", "History — Intellectual, Global & Environmental", "history", "History", Subfield,
        "Intellectual, religious, science, medicine, global, transnational, colonial, or environmental history.",
        "the paper is purely philosophical, literary, or environmental-scientific without a historical intervention.", HISTORY
    ),
    subject!(
        "subject_history_science_medicine_technology", "History — Science, Medicine & Technology", "history", "History", Subfield,
        "History of science, medicine, technology, expertise, laboratories, infrastructure, or knowledge institutions.",
        "the paper evaluates current scientific validity or technology performance without a historiographic intervention or historical source base.", HISTORY
    ),
    subject!(
        "subject_history_empire_colonial", "History — Empire, Colonialism & Decolonization", "history", "History", Subfield,
        "Imperial, colonial, postcolonial, Indigenous, slavery, borderlands, anticolonial, or decolonization history.",
        "empire is only background to a domestic narrative and colonial relations, archives, or historiography do not shape the main claim.", HISTORY
    ),
    subject!(
        "subject_history_environmental", "History — Environmental & Climate History", "history", "History", Subfield,
        "Environmental, climate, energy, resource, agricultural, disaster, or human-animal history grounded in historical sources.",
        "the central contribution is environmental science, current policy analysis, or ecological reconstruction without a historiographic argument about human-environment relations.", HISTORY
    ),

    // Economics
    subject!(
        "subject_economics_general", "Economics — General", "economics", "Economics", Discipline,
        "Economic research spanning several fields or outside the listed economics subfields.",
        "a listed economics field clearly contains the model, data, and contribution.", ECONOMICS
    ),
    subject!(
        "subject_economics_macro", "Economics — Macroeconomics (General)", "economics", "Economics", Subfield,
        "Macroeconomic research spanning several aggregate mechanisms or lying outside the listed monetary, growth, international, and labor-macro fields.",
        "a listed macroeconomic field clearly contains the main contribution, or the paper is household or firm microeconomics without an aggregate equilibrium claim.", ECONOMICS
    ),
    subject!(
        "subject_economics_monetary", "Economics — Monetary Economics & Banking", "economics", "Economics", Subfield,
        "Monetary economics, central banking, inflation, nominal rigidities, money and payments, or bank and intermediary transmission of monetary policy.",
        "asset-pricing or corporate-finance work without a central monetary, banking-system, or aggregate nominal mechanism.", ECONOMICS
    ),
    subject!(
        "subject_economics_growth", "Economics — Growth & Long-Run Development", "economics", "Economics", Subfield,
        "Economic growth, productivity, innovation, structural transformation, development accounting, or long-run aggregate change.",
        "short-run business-cycle dynamics or local development interventions without a long-run growth or structural-transformation claim.", ECONOMICS
    ),
    subject!(
        "subject_economics_international_macro", "Economics — International Macroeconomics", "economics", "Economics", Subfield,
        "Open-economy macroeconomics, exchange rates, international risk sharing, sovereign debt, global imbalances, capital flows, or cross-border monetary transmission.",
        "trade in goods or firm export behavior without an aggregate external-balance, currency, sovereign, or international financial mechanism.", ECONOMICS
    ),
    subject!(
        "subject_economics_macro_labor", "Economics — Labor Macroeconomics", "economics", "Economics", Subfield,
        "Aggregate labor markets, unemployment dynamics, search and matching, wage setting, worker and job flows, or labor-market propagation of macro shocks.",
        "individual labor-supply or personnel questions without an aggregate labor-market equilibrium or business-cycle mechanism.", ECONOMICS
    ),
    subject!(
        "subject_economics_micro_theory", "Economics — Microeconomic Theory", "economics", "Economics", Subfield,
        "Microeconomic theory, games, information, contracts, mechanism design, matching, networks, or market design.",
        "the formal result is mathematical but has no material economic incentives, allocation, or welfare content.", ECONOMICS
    ),
    subject!(
        "subject_economics_econometrics", "Economics — Econometrics", "economics", "Economics", Subfield,
        "Econometric theory or methodology is itself the main contribution.",
        "established econometric tools are applied without a methodological contribution.", ECONOMICS
    ),
    subject!(
        "subject_economics_behavioral_experimental", "Economics — Behavioral & Experimental", "economics", "Economics", Subfield,
        "Behavioral economics, experimental economics, decision theory with behavioral content, or field and laboratory evidence on economic choice.",
        "the contribution is general psychology without an economic choice, incentive, market, or welfare object.", ECONOMICS
    ),
    subject!(
        "subject_economics_public_labor", "Economics — Public, Labor & Education (General)", "economics", "Economics", Subfield,
        "Economic research that genuinely spans public finance, labor, education, family, or personnel economics and is not well represented by one listed field.",
        "a listed public-finance, labor, or education field clearly contains the main contribution, or the paper has no economic behavior or policy-incidence object.", ECONOMICS
    ),
    subject!(
        "subject_economics_public_finance", "Economics — Public Finance", "economics", "Economics", Subfield,
        "Taxation, social insurance, redistribution, government spending, fiscal federalism, public goods, or normative and positive public economics.",
        "aggregate fiscal stabilization without a public-finance incidence or welfare object, or labor-market research in which tax and transfer institutions are incidental.", ECONOMICS
    ),
    subject!(
        "subject_economics_labor", "Economics — Labor Economics", "economics", "Economics", Subfield,
        "Labor supply and demand, wages, employment, inequality, discrimination, migration, family economics, personnel, or human-capital decisions outside education institutions.",
        "aggregate unemployment and vacancy dynamics with a macro propagation claim, or schooling institutions and learning outcomes as the central object.", ECONOMICS
    ),
    subject!(
        "subject_economics_education", "Economics — Economics of Education", "economics", "Economics", Subfield,
        "Schooling, skill formation, teachers, school choice, education finance, admissions, higher education, or returns to education studied as economic decisions and institutions.",
        "curriculum or pedagogy research without an economic allocation, incentive, or policy-incidence claim, or generic labor human-capital work with no education institution.", ECONOMICS
    ),
    subject!(
        "subject_economics_development_trade", "Economics — Development & International (General)", "economics", "Economics", Subfield,
        "Economic research that materially crosses development and international economics or lies outside the listed development, trade, and international-macro fields.",
        "a listed development, international-trade, or international-macro field clearly contains the main contribution, or no economic allocation mechanism is central.", ECONOMICS
    ),
    subject!(
        "subject_economics_development", "Economics — Development Economics", "economics", "Economics", Subfield,
        "Households, firms, states, markets, poverty, institutions, or policy in low- and middle-income settings, including structural transformation at the micro and regional level.",
        "cross-country growth accounting, international trade, or political-science research without a central development-economics allocation or welfare question.", ECONOMICS
    ),
    subject!(
        "subject_economics_international_trade", "Economics — International Trade", "economics", "Economics", Subfield,
        "International trade, trade policy, multinational production, global value chains, economic geography of trade, or firm participation in foreign markets.",
        "exchange rates, sovereign debt, or capital flows without a central goods-trade, production-location, or trade-policy mechanism.", ECONOMICS
    ),
    subject!(
        "subject_economics_io", "Economics — Industrial Organization", "economics", "Economics", Subfield,
        "Industrial organization, demand, firm conduct, market power, entry, platforms, auctions, or competition policy.",
        "the main contribution is a management strategy or computer platform system without market equilibrium analysis.", ECONOMICS
    ),
    subject!(
        "subject_economics_finance", "Economics — Finance", "economics", "Economics", Subfield,
        "Asset pricing, corporate finance, banking, household finance, intermediaries, or market microstructure.",
        "the contribution is accounting description or business valuation without a finance mechanism or asset-market claim.", ECONOMICS
    ),
    subject!(
        "subject_economics_health_urban_environment", "Economics — Health, Urban & Environmental (General)", "economics", "Economics", Subfield,
        "Economic research that materially crosses health, urban, transportation, housing, environmental, or energy economics and is not well represented by one listed field.",
        "a listed health, urban, or environmental-energy field clearly contains the main contribution, or the paper lacks an economic behavior or welfare object.", ECONOMICS
    ),
    subject!(
        "subject_economics_health", "Economics — Health Economics", "economics", "Economics", Subfield,
        "Health care demand and supply, insurance, providers, health behavior, medical innovation, health inequality, or economic evaluation of health policy.",
        "clinical efficacy or population epidemiology without an economic choice, market, insurance, provider, or welfare claim.", ECONOMICS
    ),
    subject!(
        "subject_economics_urban", "Economics — Urban, Regional & Transportation", "economics", "Economics", Subfield,
        "Cities, housing, land use, transportation, local public goods, spatial sorting, regional development, or place-based policy.",
        "geographic description, planning design, or transport engineering without an economic location, market, equilibrium, or welfare mechanism.", ECONOMICS
    ),
    subject!(
        "subject_economics_environmental_energy", "Economics — Environmental & Energy", "economics", "Economics", Subfield,
        "Pollution, climate, natural resources, electricity and energy markets, environmental valuation, regulation, or adaptation studied through economic behavior and welfare.",
        "environmental science or energy engineering without an economic behavioral, market, policy-incidence, or welfare claim.", ECONOMICS
    ),
    subject!(
        "subject_economics_history", "Economics — Economic History", "economics", "Economics", Subfield,
        "Economic history using economic theory or empirical methods to explain historical development.",
        "the contribution is primarily historiographic and source-interpretive without an economic mechanism or estimand.", ECONOMICS
    ),

    // Psychology and cognitive science
    subject!(
        "subject_psychology_general", "Psychology — General", "psychology", "Psychology & Cognitive Science", Discipline,
        "Psychological or cognitive research spanning several areas or outside the listed specialties.",
        "a listed psychological subfield clearly contains the construct and evidence.", PSYCHOLOGY
    ),
    subject!(
        "subject_psychology_cognitive", "Psychology — Cognitive", "psychology", "Psychology & Cognitive Science", Subfield,
        "Cognition, perception, memory, attention, language, learning, or computational cognition.",
        "the primary contribution is a neural mechanism, social process, or NLP system rather than cognition.", PSYCHOLOGY
    ),
    subject!(
        "subject_psychology_social_personality", "Psychology — Social & Personality", "psychology", "Psychology & Cognitive Science", Subfield,
        "Social psychology, personality, attitudes, identity, interpersonal behavior, judgment, or decision-making.",
        "the main contribution is sociological institutions or political behavior at a collective level.", PSYCHOLOGY
    ),
    subject!(
        "subject_psychology_development_clinical", "Psychology — Developmental & Clinical", "psychology", "Psychology & Cognitive Science", Subfield,
        "Developmental, educational, clinical, health, or psychopathology research focused on psychological processes.",
        "the central object is clinical treatment efficacy or biological development rather than psychological theory.", PSYCHOLOGY
    ),
    subject!(
        "subject_psychology_behavioral_neuroscience", "Psychology — Behavioral Neuroscience", "psychology", "Psychology & Cognitive Science", Subfield,
        "Brain-behavior research framed by psychological function: neuropsychology, psychophysiology, or behavioral neuroscience of psychological constructs.",
        "the central contribution is cellular, circuit, or computational neuroscience, which the neuroscience discipline reviews, or medical imaging diagnosis rather than psychological function.", PSYCHOLOGY
    ),
    subject!(
        "subject_psychology_industrial_human_factors", "Psychology — Industrial, Organizational & Human Factors", "psychology", "Psychology & Cognitive Science", Subfield,
        "Work psychology, personnel selection, teams, leadership, occupational behavior, ergonomics, or human factors.",
        "the main contribution is management strategy, organizational sociology, or interface design without a psychological construct or human-performance claim.", PSYCHOLOGY
    ),
    // Neuroscience
    subject!(
        "subject_neuroscience_general", "Neuroscience — General", "neuroscience", "Neuroscience", Discipline,
        "Neuroscience research spanning several areas or outside the listed neuroscience subfields.",
        "a listed neuroscience subfield clearly carries the main contribution.", NEUROSCIENCE
    ),
    subject!(
        "subject_neuroscience_cellular_molecular", "Neuroscience — Cellular & Molecular", "neuroscience", "Neuroscience", Subfield,
        "Molecular, cellular, synaptic, or developmental neuroscience centered on mechanisms within and between neurons and glia.",
        "the contribution is systems-level recording, human cognition, or general cell biology without a neural mechanism claim.", NEUROSCIENCE
    ),
    subject!(
        "subject_neuroscience_systems_circuits", "Neuroscience — Systems & Circuits", "neuroscience", "Neuroscience", Subfield,
        "Systems and circuit neuroscience: in vivo recording, circuit manipulation, population coding, or sensory, motor, and state control in animals.",
        "human task-based cognition, or a purely computational model without new physiological evidence.", NEUROSCIENCE
    ),
    subject!(
        "subject_neuroscience_cognitive", "Neuroscience — Cognitive & Human", "neuroscience", "Neuroscience", Subfield,
        "Human cognitive, perceptual, affective, or social neuroscience linking brain measurement to mental constructs.",
        "behavior-only psychology without neural data, or clinical treatment studies of neurological disease.", NEUROSCIENCE
    ),
    subject!(
        "subject_neuroscience_computational", "Neuroscience — Computational & Theoretical", "neuroscience", "Neuroscience", Subfield,
        "Computational and theoretical neuroscience: mechanistic or normative models of neural dynamics, coding, learning, or computation.",
        "generic machine learning without a neural claim, or standard model-based data analysis without a theoretical contribution.", NEUROSCIENCE
    ),
    subject!(
        "subject_neuroscience_clinical_translational", "Neuroscience — Clinical & Translational", "neuroscience", "Neuroscience", Subfield,
        "Translational and clinical neuroscience: disease mechanisms, models, biomarkers, neuromodulation, or therapeutic-target work.",
        "a powered clinical trial or patient-care study, which clinical medicine and clinical-study roles review.", NEUROSCIENCE
    ),

    // Anthropology and archaeology
    subject!(
        "subject_anthropology_general", "Anthropology — General", "anthropology", "Anthropology & Archaeology", Discipline,
        "Anthropological or archaeological scholarship spanning several traditions or outside the listed specialties.",
        "a sociocultural, biological-linguistic, or archaeological lens clearly fits.", ANTHROPOLOGY
    ),
    subject!(
        "subject_anthropology_sociocultural", "Anthropology — Sociocultural", "anthropology", "Anthropology & Archaeology", Subfield,
        "Ethnographic, sociocultural, medical, political, economic, or linguistic-cultural anthropology.",
        "the paper is primarily survey sociology or textual humanities without ethnographic or anthropological comparison.", ANTHROPOLOGY
    ),
    subject!(
        "subject_anthropology_biological_linguistic", "Anthropology — Biological & Linguistic", "anthropology", "Anthropology & Archaeology", Subfield,
        "Biological anthropology, human evolution, primatology, linguistic anthropology, or language and culture.",
        "the main result is general evolutionary biology or formal linguistics without an anthropological human context.", ANTHROPOLOGY
    ),
    subject!(
        "subject_anthropology_archaeology", "Anthropology — Archaeology & Material Culture", "anthropology", "Anthropology & Archaeology", Subfield,
        "Archaeology, material culture, heritage, bioarchaeology, or archaeological science.",
        "material objects are chiefly art-historical texts or geological specimens without archaeological context.", ANTHROPOLOGY
    ),
    subject!(
        "subject_anthropology_medical", "Anthropology — Medical Anthropology", "anthropology", "Anthropology & Archaeology", Subfield,
        "Medical anthropology, illness and healing, embodiment, care, disability, pharmaceuticals, global health, or the cultural and political organization of medicine.",
        "the central contribution is clinical efficacy, epidemiology, or health sociology without ethnographic or comparative anthropological analysis.", ANTHROPOLOGY
    ),
    subject!(
        "subject_anthropology_political_economic", "Anthropology — Political & Economic Anthropology", "anthropology", "Anthropology & Archaeology", Subfield,
        "Anthropology of states, law, markets, labor, exchange, value, development, infrastructure, violence, borders, or political economy.",
        "the contribution is an economics or political-science account without ethnographic categories, situated practice, or anthropological comparison.", ANTHROPOLOGY
    ),
    subject!(
        "subject_anthropology_linguistic", "Anthropology — Linguistic Anthropology", "anthropology", "Anthropology & Archaeology", Subfield,
        "Linguistic anthropology, language ideology, discourse and interaction, multilingualism, semiotics, linguistic inequality, or language and social identity.",
        "the central result is formal linguistics, NLP, or discourse content without an anthropological claim about language in social and cultural practice.", ANTHROPOLOGY
    ),

    // Geography
    subject!(
        "subject_geography_general", "Geography — General", "geography", "Geography", Discipline,
        "Geographic research spanning human, physical, and geospatial approaches or outside the listed specialties.",
        "a listed geographic subfield clearly contains the spatial process.", GEOGRAPHY
    ),
    subject!(
        "subject_geography_human_urban", "Geography — Human, Economic & Urban", "geography", "Geography", Subfield,
        "Human, economic, political, urban, cultural, or development geography.",
        "the contribution is primarily sociology, economics, or political science without a geographic account of space and place.", GEOGRAPHY
    ),
    subject!(
        "subject_geography_physical", "Geography — Physical & Environmental", "geography", "Geography", Subfield,
        "Physical geography, biogeography, geomorphology, landscape, human-environment, or environmental change.",
        "the main contribution is a narrow Earth-system mechanism without a geographic landscape or spatial synthesis.", GEOGRAPHY
    ),
    subject!(
        "subject_geography_gis", "Geography — GIS & Remote Sensing", "geography", "Geography", Subfield,
        "Geographic information science, cartography, spatial data infrastructure, remote sensing, or geocomputation.",
        "GIS is a routine tool and no geographic measurement, representation, or spatial-method contribution is made.", GEOGRAPHY
    ),

    // Philosophy
    subject!(
        "subject_philosophy_general", "Philosophy — General", "philosophy", "Philosophy", Discipline,
        "Philosophical work spanning several areas or outside the listed philosophical specialties.",
        "a listed philosophical area clearly contains the principal argument.", PHILOSOPHY
    ),
    subject!(
        "subject_philosophy_logic_epistemology", "Philosophy — Logic & Epistemology", "philosophy", "Philosophy", Subfield,
        "Philosophical logic, epistemology, rational belief, formal epistemology, or philosophy of language.",
        "the central result is mathematical logic or empirical cognition rather than a philosophical account of knowledge or meaning.", PHILOSOPHY
    ),
    subject!(
        "subject_philosophy_metaphysics_mind", "Philosophy — Metaphysics & Mind", "philosophy", "Philosophy", Subfield,
        "Metaphysics, ontology, modality, causation, time, personal identity, consciousness, or philosophy of mind.",
        "the contribution is empirical neuroscience or psychology without a substantive metaphysical or philosophy-of-mind argument.", PHILOSOPHY
    ),
    subject!(
        "subject_philosophy_ethics_political", "Philosophy — Ethics & Political", "philosophy", "Philosophy", Subfield,
        "Normative ethics, metaethics, applied ethics, political philosophy, justice, rights, or responsibility.",
        "the central contribution is empirical policy analysis or political theory grounded primarily in historical interpretation.", PHILOSOPHY
    ),
    subject!(
        "subject_philosophy_science_history", "Philosophy — Science & History of Philosophy", "philosophy", "Philosophy", Subfield,
        "Philosophy of science, biology, physics, social science, medicine, or historically grounded philosophy.",
        "the paper is history of science without a philosophical claim or science without conceptual analysis.", PHILOSOPHY
    ),

    // Linguistics
    subject!(
        "subject_linguistics_general", "Linguistics — General", "linguistics", "Linguistics", Discipline,
        "Linguistic research spanning several levels or outside the listed linguistic specialties.",
        "a formal, sound-structure, or social-historical linguistic lens clearly fits.", LINGUISTICS
    ),
    subject!(
        "subject_linguistics_syntax_semantics", "Linguistics — Syntax, Semantics & Pragmatics", "linguistics", "Linguistics", Subfield,
        "Syntax, formal semantics, pragmatics, discourse, morphology-syntax, or grammatical theory.",
        "the primary contribution is an NLP model or literary interpretation without a linguistic grammar claim.", LINGUISTICS
    ),
    subject!(
        "subject_linguistics_sound_structure", "Linguistics — Phonetics, Phonology & Morphology", "linguistics", "Linguistics", Subfield,
        "Phonetics, phonology, morphology, speech production or perception, and sound or word structure.",
        "speech is merely an engineering signal and no linguistic sound-structure claim is made.", LINGUISTICS
    ),
    subject!(
        "subject_linguistics_social_historical", "Linguistics — Social, Historical & Computational", "linguistics", "Linguistics", Subfield,
        "Sociolinguistics, language variation and change, historical linguistics, documentation, corpus or computational linguistics with a linguistic contribution.",
        "the contribution is chiefly sociology, history, or computer science without a linguistic account of language structure or use.", LINGUISTICS
    ),

    // Education
    subject!(
        "subject_education_general", "Education — General", "education", "Education", Discipline,
        "Education research spanning policy, learning, instruction, institutions, or assessment.",
        "a listed education specialty clearly contains the intervention or institution.", EDUCATION
    ),
    subject!(
        "subject_education_policy", "Education — Policy & Economics", "education", "Education", Subfield,
        "Education policy, school choice, finance, accountability, teacher labor markets, access, or inequality.",
        "the paper is economics of education without material educational institutions or practice to assess.", EDUCATION
    ),
    subject!(
        "subject_education_learning", "Education — Learning, Curriculum & Instruction", "education", "Education", Subfield,
        "Learning sciences, pedagogy, curriculum, classroom instruction, teacher practice, or educational technology.",
        "the contribution is a general cognitive theory or technology with no educational learning claim.", EDUCATION
    ),
    subject!(
        "subject_education_higher_measurement", "Education — Higher, Special & Assessment", "education", "Education", Subfield,
        "Higher education, special education, educational measurement, assessment, admissions, or institutional student support.",
        "the contribution is psychometric theory or organizational policy without a substantive education question.", EDUCATION
    ),

    // Law
    subject!(
        "subject_law_general", "Law — General", "law", "Law", Discipline,
        "Legal scholarship spanning several domains or outside the listed legal specialties.",
        "a listed doctrinal, private-criminal, international, or empirical-legal lens clearly fits.", LAW
    ),
    subject!(
        "subject_law_public", "Law — Constitutional, Administrative & Regulatory", "law", "Law", Subfield,
        "Constitutional, administrative, regulatory, legislation, courts, public law, or governance doctrine.",
        "the primary question is international, private, or criminal law without a material public-law issue.", LAW
    ),
    subject!(
        "subject_law_private_criminal", "Law — Private & Criminal", "law", "Law", Subfield,
        "Contracts, torts, property, corporations, commercial, family, criminal law, procedure, or punishment.",
        "the central contribution is public regulation or empirical crime analysis without doctrinal private or criminal law.", LAW
    ),
    subject!(
        "subject_law_international", "Law — International & Comparative", "law", "Law", Subfield,
        "Public or private international law, human rights, trade law, comparative law, transnational regulation, or conflict of laws.",
        "international relations or comparative politics is central but no legal-authority or doctrinal claim is made.", LAW
    ),
    subject!(
        "subject_law_empirical_economic", "Law — Empirical & Law and Economics", "law", "Law", Subfield,
        "Empirical legal studies, law and economics, legal institutions, judicial behavior, or quantitative doctrinal consequences.",
        "the contribution is an economics or political-science result with law only as background.", LAW
    ),

    // Business and management
    subject!(
        "subject_business_general", "Business — General", "business", "Business & Management", Discipline,
        "Business or management research spanning several functions or outside the listed specialties.",
        "a listed strategy, operations, accounting-finance, or marketing lens clearly contains the contribution.", BUSINESS
    ),
    subject!(
        "subject_business_strategy_organization", "Business — Strategy, Organization & Entrepreneurship", "business", "Business & Management", Subfield,
        "Strategy, organization theory, organizational behavior, entrepreneurship, innovation, or human resources.",
        "the contribution is industrial organization economics or sociology without a managerial or firm-strategy object.", BUSINESS
    ),
    subject!(
        "subject_business_operations_information", "Business — Operations & Information Systems", "business", "Business & Management", Subfield,
        "Operations management, supply chains, service systems, analytics, information systems, or digital operations.",
        "the central contribution is an engineering or computer-science system without an organizational operating decision.", BUSINESS
    ),
    subject!(
        "subject_business_accounting_finance", "Business — Accounting & Corporate Finance", "business", "Business & Management", Subfield,
        "Accounting, auditing, disclosure, governance, corporate finance, capital markets, or taxation in firms.",
        "the primary contribution is asset-pricing or public-finance economics without a firm reporting or governance question.", BUSINESS
    ),
    subject!(
        "subject_business_marketing", "Business — Marketing & Consumer", "business", "Business & Management", Subfield,
        "Marketing, consumer behavior, branding, pricing, channels, sales, advertising, or customer analytics.",
        "the main contribution is general psychology or demand estimation without a marketing decision or market context.", BUSINESS
    ),
    subject!(
        "subject_business_organizational_behavior_hr", "Business — Organizational Behavior & Human Resources", "business", "Business & Management", Subfield,
        "Organizational behavior, human-resource management, leadership, teams, workplace culture, personnel systems, or employee well-being and performance.",
        "the central contribution is general social psychology or labor economics without a managerial organization, workforce practice, or firm-performance object.", BUSINESS
    ),
    subject!(
        "subject_business_entrepreneurship_innovation", "Business — Entrepreneurship & Innovation", "business", "Business & Management", Subfield,
        "Entrepreneurship, new ventures, innovation strategy, technology commercialization, venture finance, ecosystems, or founder and startup dynamics.",
        "innovation is an aggregate growth outcome or technology measure without a venture, firm-strategy, commercialization, or entrepreneurial process.", BUSINESS
    ),
    subject!(
        "subject_business_supply_chain_operations", "Business — Supply Chain & Operations Management", "business", "Business & Management", Subfield,
        "Supply chains, sourcing, inventory, logistics, service operations, capacity, procurement, resilience, or operational process design.",
        "the contribution is abstract optimization, transportation engineering, or an information system without a material managerial operating decision.", BUSINESS
    ),

    // Humanities
    subject!(
        "subject_humanities_general", "Humanities — General", "humanities", "Humanities", Discipline,
        "Humanities scholarship spanning several traditions or outside the listed literary, religious, visual, media, or digital fields.",
        "a listed humanities specialty clearly contains the primary objects and scholarly conversation.", HUMANITIES
    ),
    subject!(
        "subject_humanities_literature_classics", "Humanities — Literature & Classics", "humanities", "Humanities", Subfield,
        "Literary studies, comparative literature, classics, philology, rhetoric, or book history.",
        "the central contribution is historical fact, formal linguistics, or automated text analysis without literary interpretation.", HUMANITIES
    ),
    subject!(
        "subject_humanities_religion", "Humanities — Religion & Theology", "humanities", "Humanities", Subfield,
        "Religious studies, theology, scriptural interpretation, ritual, doctrine, or religion and society.",
        "religion is only a demographic category and no religious text, practice, institution, or theological argument is central.", HUMANITIES
    ),
    subject!(
        "subject_humanities_art_music", "Humanities — Art, Architecture & Music", "humanities", "Humanities", Subfield,
        "Art history, architectural history, visual culture, musicology, performance, or material aesthetics.",
        "images or music are data inputs without an art-historical, architectural, or musicological contribution.", HUMANITIES
    ),
    subject!(
        "subject_humanities_media_cultural", "Humanities — Media & Cultural Studies", "humanities", "Humanities", Subfield,
        "Film, television, media, communication, cultural studies, popular culture, games, or performance studies.",
        "the contribution is a technical media system or quantitative communication effect without interpretive cultural analysis.", HUMANITIES
    ),
    subject!(
        "subject_humanities_digital_public", "Humanities — Digital & Public", "humanities", "Humanities", Subfield,
        "Digital humanities, public humanities, archives and editions, cultural heritage, museums, or computational cultural analysis.",
        "the central contribution is a general computer-science method or public-history narrative without a humanities research object.", HUMANITIES
    ),
    subject!(
        "subject_humanities_film_theater_performance", "Humanities — Film, Theater & Performance", "humanities", "Humanities", Subfield,
        "Film studies, theater and dance studies, performance studies, screen cultures, dramaturgy, or embodied and mediated performance.",
        "performance is only an experimental intervention or media exposure and no interpretive claim about form, staging, embodiment, or spectatorship is central.", HUMANITIES
    ),
    subject!(
        "subject_humanities_translation_book_history", "Humanities — Translation, Philology & Book History", "humanities", "Humanities", Subfield,
        "Translation studies, philology, textual criticism, bibliography, manuscript and print cultures, reception, or the material history and transmission of texts.",
        "translation is merely a transparent aid, or the paper interprets a stable modern text without a material linguistic, editorial, or transmission problem.", HUMANITIES
    ),
    subject!(
        "subject_humanities_museum_heritage", "Humanities — Museums, Heritage & Material Culture", "humanities", "Humanities", Subfield,
        "Museum and heritage studies, curation, conservation, monuments, collecting, provenance, restitution, or humanistic interpretation of material culture.",
        "the object is an archaeological specimen studied chiefly for past social inference, or museum display is incidental to an art-historical argument.", HUMANITIES
    ),

    // Agricultural, food, and veterinary sciences
    subject!(
        "subject_agriculture_veterinary_general", "Agriculture & Veterinary Science — General", "agriculture_veterinary", "Agriculture & Veterinary Science", Discipline,
        "Agricultural, food, forestry, fisheries, animal, or veterinary research spanning several systems or outside the listed specialties.",
        "a listed agricultural, food, animal, veterinary, forestry, or rural-systems lens clearly contains the main contribution.", AGRICULTURE_VETERINARY
    ),
    subject!(
        "subject_agriculture_veterinary_crop_soil", "Agriculture & Veterinary Science — Crop, Soil & Horticulture", "agriculture_veterinary", "Agriculture & Veterinary Science", Subfield,
        "Agronomy, crop science, horticulture, soil science, agroecology, plant breeding, or crop protection.",
        "the contribution is basic plant biology without a managed production or agroecosystem claim.", AGRICULTURE_VETERINARY
    ),
    subject!(
        "subject_agriculture_veterinary_animal", "Agriculture & Veterinary Science — Animal & Veterinary", "agriculture_veterinary", "Agriculture & Veterinary Science", Subfield,
        "Animal science, veterinary medicine, livestock systems, animal health, breeding, welfare, or comparative clinical research.",
        "the central contribution is human clinical medicine, wildlife ecology, or cellular biology without an animal-health or production-system claim.", AGRICULTURE_VETERINARY
    ),
    subject!(
        "subject_agriculture_veterinary_food", "Agriculture & Veterinary Science — Food Science & Safety", "agriculture_veterinary", "Agriculture & Veterinary Science", Subfield,
        "Food chemistry, processing, preservation, sensory science, food microbiology, quality, or food safety.",
        "the contribution is human nutrition, molecular chemistry, or process engineering without a food-system quality or safety claim.", AGRICULTURE_VETERINARY
    ),
    subject!(
        "subject_agriculture_veterinary_forestry_fisheries", "Agriculture & Veterinary Science — Forestry, Fisheries & Aquaculture", "agriculture_veterinary", "Agriculture & Veterinary Science", Subfield,
        "Forestry, silviculture, fisheries science, aquaculture, rangelands, or renewable biological-resource management.",
        "the central contribution is conservation biology or environmental policy without a managed harvest, production, or resource-system claim.", AGRICULTURE_VETERINARY
    ),
    subject!(
        "subject_agriculture_veterinary_systems_rural", "Agriculture & Veterinary Science — Agricultural Systems & Rural Development", "agriculture_veterinary", "Agriculture & Veterinary Science", Subfield,
        "Farming systems, agricultural extension, rural development, food systems, farm management, or technology adoption.",
        "the contribution is development economics or rural sociology without material agricultural production, extension, or food-system expertise.", AGRICULTURE_VETERINARY
    ),

    // Communication and information
    subject!(
        "subject_communication_information_general", "Communication & Information — General", "communication_information", "Communication & Information", Discipline,
        "Communication, journalism, information, or knowledge-institution research spanning several areas or outside the listed specialties.",
        "a listed communication, information, risk-communication, or interpersonal lens clearly contains the central process.", COMMUNICATION_INFORMATION
    ),
    subject!(
        "subject_communication_information_media_journalism", "Communication & Information — Media, Journalism & Public Communication", "communication_information", "Communication & Information", Subfield,
        "Journalism studies, mass communication, news, political communication, public relations, or media institutions.",
        "the contribution is interpretive media studies without a communication-process claim or political behavior without a material media institution.", COMMUNICATION_INFORMATION
    ),
    subject!(
        "subject_communication_information_library", "Communication & Information — Library & Information Science", "communication_information", "Communication & Information", Subfield,
        "Library and information science, archives, knowledge organization, information behavior, scholarly communication, or information institutions.",
        "the central contribution is database engineering or digital humanities without an information-practice or institution claim.", COMMUNICATION_INFORMATION
    ),
    subject!(
        "subject_communication_information_science_health_risk", "Communication & Information — Science, Health & Risk Communication", "communication_information", "Communication & Information", Subfield,
        "Communication of science, medicine, environment, uncertainty, crisis, hazards, or public risk.",
        "the contribution is clinical, environmental, or engineering risk assessment without a communicative process or audience claim.", COMMUNICATION_INFORMATION
    ),
    subject!(
        "subject_communication_information_interpersonal_organizational", "Communication & Information — Interpersonal & Organizational", "communication_information", "Communication & Information", Subfield,
        "Interpersonal, organizational, health, family, group, or computer-mediated communication.",
        "the main contribution is social psychology or management without a material message, interaction, discourse, or communication-system claim.", COMMUNICATION_INFORMATION
    ),

    // Architecture, planning, and design
    subject!(
        "subject_architecture_design_general", "Architecture, Planning & Design — General", "architecture_design", "Architecture, Planning & Design", Discipline,
        "Architecture, planning, landscape, or design scholarship spanning several areas or outside the listed specialties.",
        "a listed built-environment, planning, landscape, or design lens clearly contains the central artifact or intervention.", ARCHITECTURE_DESIGN
    ),
    subject!(
        "subject_architecture_design_built_environment", "Architecture, Planning & Design — Architecture & Built Environment", "architecture_design", "Architecture, Planning & Design", Subfield,
        "Architectural design and theory, building science, housing design, interiors, heritage conservation, or built-environment research.",
        "the contribution is structural engineering or art history without a material architectural performance, use, or design argument.", ARCHITECTURE_DESIGN
    ),
    subject!(
        "subject_architecture_design_planning", "Architecture, Planning & Design — Urban & Regional Planning", "architecture_design", "Architecture, Planning & Design", Subfield,
        "Urban and regional planning, land use, transportation planning, housing, infrastructure governance, or community development.",
        "the central contribution is urban economics, geography, or civil engineering without a planning institution, process, or intervention.", ARCHITECTURE_DESIGN
    ),
    subject!(
        "subject_architecture_design_landscape", "Architecture, Planning & Design — Landscape Architecture", "architecture_design", "Architecture, Planning & Design", Subfield,
        "Landscape architecture, ecological design, public space, site planning, or landscape performance.",
        "the work is ecosystem science or geography without a designed landscape, site, or public-space claim.", ARCHITECTURE_DESIGN
    ),
    subject!(
        "subject_architecture_design_human_centered", "Architecture, Planning & Design — Human-Centered & Product Design", "architecture_design", "Architecture, Planning & Design", Subfield,
        "Industrial, product, service, interaction, participatory, or human-centered design where design knowledge is the contribution.",
        "the central contribution is HCI evaluation, mechanical engineering, or marketing without a material design inquiry or artifact claim.", ARCHITECTURE_DESIGN
    ),

    // Social work and social policy
    subject!(
        "subject_social_work_policy_general", "Social Work & Social Policy — General", "social_work_policy", "Social Work & Social Policy", Discipline,
        "Social work, human services, welfare, or community-intervention research spanning several areas or outside the listed specialties.",
        "a listed practice, welfare-policy, or community-organization lens clearly contains the intervention and outcome.", SOCIAL_WORK_POLICY
    ),
    subject!(
        "subject_social_work_policy_practice", "Social Work & Social Policy — Practice & Human Services", "social_work_policy", "Social Work & Social Policy", Subfield,
        "Clinical and direct social work, child and family services, mental-health services, case management, safeguarding, or human-service delivery.",
        "the contribution is clinical treatment efficacy or organizational management without a social-work practice, service, or person-in-environment claim.", SOCIAL_WORK_POLICY
    ),
    subject!(
        "subject_social_work_policy_welfare", "Social Work & Social Policy — Welfare & Social Policy", "social_work_policy", "Social Work & Social Policy", Subfield,
        "Welfare states, poverty policy, social protection, disability policy, family policy, housing support, or comparative social policy.",
        "the paper is public economics or political administration without a material welfare institution, service-user, or social-policy contribution.", SOCIAL_WORK_POLICY
    ),
    subject!(
        "subject_social_work_policy_community_nonprofit", "Social Work & Social Policy — Community & Nonprofit", "social_work_policy", "Social Work & Social Policy", Subfield,
        "Community practice, nonprofit and voluntary organizations, mutual aid, social development, advocacy, or collective service provision.",
        "the main contribution is organizational sociology or public administration without a community-practice, voluntary-sector, or service mission.", SOCIAL_WORK_POLICY
    ),

    // Health professions
    subject!(
        "subject_health_professions_general", "Health Professions — General", "health_professions", "Health Professions", Discipline,
        "Nursing, rehabilitation, pharmacy, oral health, nutrition, or allied-health research spanning several professions or outside the listed specialties.",
        "a listed health-profession lens or a medicine-and-health specialist clearly contains the care process and outcome.", HEALTH_PROFESSIONS
    ),
    subject!(
        "subject_health_professions_nursing", "Health Professions — Nursing & Care Science", "health_professions", "Health Professions", Subfield,
        "Nursing science, midwifery, care delivery, symptom management, patient safety, or nursing workforce research.",
        "the central contribution is physician-led clinical efficacy or health-services economics without a nursing or care-process claim.", HEALTH_PROFESSIONS
    ),
    subject!(
        "subject_health_professions_rehabilitation", "Health Professions — Rehabilitation, Disability & Occupational Therapy", "health_professions", "Health Professions", Subfield,
        "Physical, occupational, speech, vocational, or multidisciplinary rehabilitation; disability and functioning research.",
        "the contribution is basic motor science or acute medical treatment without a rehabilitation, participation, or functioning claim.", HEALTH_PROFESSIONS
    ),
    subject!(
        "subject_health_professions_pharmacy", "Health Professions — Pharmacy & Pharmacoepidemiology", "health_professions", "Health Professions", Subfield,
        "Pharmacy practice, medication use and safety, pharmacotherapy, pharmacoepidemiology, pharmacovigilance, or medicines policy.",
        "the central contribution is molecular pharmacology, a clinical trial, or health economics without a medication-use or pharmacy-practice claim.", HEALTH_PROFESSIONS
    ),
    subject!(
        "subject_health_professions_dentistry", "Health Professions — Dentistry & Oral Health", "health_professions", "Health Professions", Subfield,
        "Dentistry, oral medicine, dental materials in clinical use, oral epidemiology, prevention, or oral-health services.",
        "the main contribution is materials engineering or general epidemiology without an oral-health mechanism, procedure, or care claim.", HEALTH_PROFESSIONS
    ),
    subject!(
        "subject_health_professions_nutrition_exercise", "Health Professions — Nutrition, Dietetics & Exercise", "health_professions", "Health Professions", Subfield,
        "Human nutrition, dietetics, exercise science, sports medicine, physical activity, or lifestyle intervention research.",
        "the contribution is food chemistry, elite performance engineering, or population epidemiology without a material nutrition, exercise, or professional-practice claim.", HEALTH_PROFESSIONS
    ),

    // Interdisciplinary studies
    subject!(
        "subject_interdisciplinary_studies_general", "Interdisciplinary Studies — General", "interdisciplinary_studies", "Interdisciplinary Studies", Discipline,
        "Interdisciplinary scholarship whose central contribution cannot be evaluated adequately from one listed discipline or more specific interdisciplinary field.",
        "one or two listed disciplinary or interdisciplinary subfield reviewers can cover the paper's central contribution without a general fallback.", INTERDISCIPLINARY_STUDIES
    ),
    subject!(
        "subject_interdisciplinary_studies_sts", "Interdisciplinary Studies — Science, Technology & Society", "interdisciplinary_studies", "Interdisciplinary Studies", Subfield,
        "Science and technology studies, sociology or anthropology of knowledge, infrastructure studies, innovation studies, or critical data and algorithm studies.",
        "the contribution is philosophy or history of science, technical system design, or innovation economics without a material knowledge-practice or sociotechnical claim.", INTERDISCIPLINARY_STUDIES
    ),
    subject!(
        "subject_interdisciplinary_studies_gender_sexuality", "Interdisciplinary Studies — Gender & Sexuality", "interdisciplinary_studies", "Interdisciplinary Studies", Subfield,
        "Gender, sexuality, feminist, queer, masculinity, or intersectional studies across social-scientific and humanistic traditions.",
        "gender or sexuality appears only as a demographic covariate and no category, institution, identity, representation, or power relation is central.", INTERDISCIPLINARY_STUDIES
    ),
    subject!(
        "subject_interdisciplinary_studies_race_indigenous", "Interdisciplinary Studies — Race, Ethnicity & Indigenous", "interdisciplinary_studies", "Interdisciplinary Studies", Subfield,
        "Ethnic studies, race and diaspora studies, Indigenous studies, settler-colonial studies, or community-grounded scholarship across fields.",
        "race, ethnicity, or indigeneity is only a control variable and no historical, institutional, cultural, territorial, or knowledge-governance claim is central.", INTERDISCIPLINARY_STUDIES
    ),
    subject!(
        "subject_interdisciplinary_studies_area_global", "Interdisciplinary Studies — Area & Global", "interdisciplinary_studies", "Interdisciplinary Studies", Subfield,
        "Regional, area, transnational, postcolonial, development, or global studies integrating language, history, institutions, and contemporary evidence.",
        "the contribution fits a specific history, politics, economics, geography, literature, or anthropology subfield without material interdisciplinary regional synthesis.", INTERDISCIPLINARY_STUDIES
    ),
    subject!(
        "subject_interdisciplinary_metascience", "Interdisciplinary Studies — Metascience & Science of Science", "interdisciplinary_studies", "Interdisciplinary Studies", Subfield,
        "The scientific study of science itself: replication and reliability, publication and funding systems, research careers, peer review, or science-policy evidence.",
        "a bibliometric mapping exercise without a claim about how science works, or the history and sociology of one discipline reviewed by its own field.", INTERDISCIPLINARY_STUDIES
    ),
];
