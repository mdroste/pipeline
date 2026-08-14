Build a structured orientation map and a bounded review plan for this academic paper. Multiple independent reviewers will use this object. Your routing choices determine which specialist reviewers the host application assembles for this run, so classify the paper from its actual claims, evidence, and methods rather than keywords, author affiliations, or departmental labels.

Return a JSON object with exactly these top-level fields:

{
  "metadata": {
    "title": "...",
    "authors": ["..."],
    "date": "...",
    "paper_type": "theory" | "empirical" | "mixed",
    "page_count": null,
    "has_appendix": true,
    "has_online_appendix": false
  },
  "review_plan": {
    "primary_domain": "The paper's broad discipline",
    "subject": "A concise, specific field or subject description",
    "paper_forms": ["formal_theory", "causal_empirical", "quantitative_model", "descriptive", "experimental", "algorithmic", "qualitative", "interpretive", "historical", "clinical", "engineering_design"],
    "methods": ["Concise names of methods that actually support central claims"],
    "subject_specialist_ids": ["one primary subject ID and, only when necessary, one secondary subject ID"],
    "method_specialist_ids": ["one to four method IDs"],
    "selection_notes": [
      {"id": "one selected ID", "reason": "Concrete reason tied to a central claim, method, theorem, dataset, source base, experiment, or design"}
    ],
    "routing_uncertainty": ["Material ambiguity in classification, or an empty array"]
  },
  "sections": [
    {"number": "1", "title": "Introduction", "page_start": 1, "page_end": 4}
  ],
  "formal_results": [
    {"kind": "theorem", "number": "1", "page": 10, "summary": "...", "proof_location": "Appendix A, pp. 30-33"}
  ],
  "tables_figures": [
    {"kind": "table", "number": "1", "page": 14, "caption_summary": "...", "what_it_shows": "..."}
  ],
  "notation": [
    {"symbol": "beta", "definition": "...", "page_introduced": 5}
  ],
  "stated_contribution": "Quoted or closely paraphrased from the introduction",
  "key_references": ["Author (year)"],
  "extraction_quality_notes": [
    {"page_range": "pp. 10-15", "description": "equations garbled, subscripts missing"}
  ]
}

SUBJECT SPECIALIST CATALOG

The catalog is grouped by discipline. Within each group, the broad discipline entry is a fallback; prefer one more-specific subfield entry whenever it fits the paper's central contribution.

### Physics
- `subject_physics_general` — Broad physics research that does not fit a more specific physics subfield in this catalog. Exclude when: a listed physics subfield clearly carries the main contribution.
- `subject_physics_theoretical_mathematical` — Theoretical or mathematical physics centered on formal physical models, symmetries, fields, or exact structure. Exclude when: the result is primarily a pure mathematical theorem without a material physical interpretation.
- `subject_physics_particle_nuclear` — Particle, high-energy, nuclear, hadronic, accelerator, or fundamental-interaction research. Exclude when: astrophysical use of particle models is central but laboratory or nuclear physics is not.
- `subject_physics_condensed_materials` — Condensed-matter, soft-matter, mesoscopic, many-body, or materials-physics research. Exclude when: the primary contribution is materials synthesis or engineering performance rather than physical mechanism.
- `subject_physics_amo_quantum` — Atomic, molecular, optical, photonic, ultracold-matter, or quantum-optics research. Exclude when: the paper is chiefly about quantum algorithms or information protocols rather than a physical AMO platform.
- `subject_physics_statistical_complex` — Statistical mechanics, nonlinear dynamics, complex systems, networks in physics, or nonequilibrium phenomena. Exclude when: network analysis is primarily social or computational and lacks a physical statistical-mechanics claim.
- `subject_physics_astrophysics_cosmology` — Astrophysics, astronomy, cosmology, gravitation, or large-scale-universe research. Exclude when: the main object is terrestrial geophysics or the work uses astronomy data only as an incidental application.
- `subject_physics_plasma_fluid` — Plasma physics, fluid dynamics, magnetohydrodynamics, turbulence, or continuum-flow research. Exclude when: the main contribution is an engineering device with fluid behavior only as an input.
- `subject_physics_geophysics` — Physical study of Earth's interior, seismology, geomagnetism, geodynamics, or planetary interiors. Exclude when: the central contribution concerns climate, ecology, or descriptive geology rather than a physical Earth model.
- `subject_physics_quantum_information` — Quantum information, communication, sensing, error correction, or physically realized quantum computation. Exclude when: the contribution is a classical algorithm or an AMO experiment without an information-theoretic claim.

### Mathematics
- `subject_mathematics_general` — Pure or applied mathematics spanning several areas or outside the listed mathematical subfields. Exclude when: a listed mathematical subfield clearly contains the main theorem or construction.
- `subject_mathematics_algebra_number` — Algebra, representation theory, algebraic geometry, arithmetic geometry, or number theory. Exclude when: the central result is geometric or analytic without material algebraic or arithmetic structure.
- `subject_mathematics_geometry_topology` — Differential, algebraic, symplectic, metric, or discrete geometry and algebraic or geometric topology. Exclude when: geometric language is merely a representation of an analytic or applied problem.
- `subject_mathematics_analysis` — Real, complex, functional, harmonic, operator, or variational analysis not primarily organized around a PDE. Exclude when: the principal contribution is a differential-equation existence, regularity, or dynamics result.
- `subject_mathematics_pde` — Partial differential equations, calculus of variations, geometric flows, or continuum mathematical models. Exclude when: the equation is only a numerical test problem or routine applied model without a mathematical PDE contribution.
- `subject_mathematics_probability` — Probability theory, stochastic processes, random structures, concentration, or stochastic analysis. Exclude when: probability is used only for routine statistical inference rather than as the mathematical contribution.
- `subject_mathematics_combinatorics` — Combinatorics, graph theory, discrete geometry, extremal or probabilistic combinatorics. Exclude when: the graph or discrete representation serves only an algorithmic engineering objective.
- `subject_mathematics_logic_foundations` — Mathematical logic, set theory, model theory, proof theory, computability, or foundations. Exclude when: formal verification is used only as a computer-science implementation tool.
- `subject_mathematics_applied_numerical` — Applied mathematics, numerical analysis, inverse problems, scientific computing, or approximation theory. Exclude when: the work is principally an engineering application or software benchmark without mathematical analysis.
- `subject_mathematics_optimization_dynamics` — Optimization theory, optimal control, operations research mathematics, dynamical systems, or ergodic dynamics. Exclude when: optimization is merely a routine fitting algorithm or the main contribution is an engineering controller.

### Statistics
- `subject_statistics_general` — Statistical methodology spanning several areas or not covered by a narrower statistics specialist. Exclude when: statistics is only an application tool and another substantive discipline owns the contribution.
- `subject_statistics_theory` — Decision theory, minimax analysis, asymptotic theory, nonparametrics, or foundational statistical methodology. Exclude when: the paper primarily applies established theory to one empirical domain.
- `subject_statistics_bayesian` — Bayesian modeling, posterior theory, probabilistic programming, prior construction, or Bayesian computation. Exclude when: Bayesian software is used routinely but no Bayesian modeling or inferential contribution is made.
- `subject_statistics_causal_semiparametric` — Causal estimands, semiparametric efficiency, missing data, treatment effects, or robust causal methodology as the contribution. Exclude when: causal identification is only an application and the methodological contribution is not statistical.
- `subject_statistics_highdim_learning` — High-dimensional inference, sparsity, statistical learning theory, prediction, or modern nonparametrics. Exclude when: the central contribution is a computer-science algorithm or application benchmark rather than statistical understanding.
- `subject_statistics_time_series` — Temporal dependence, forecasting, state-space models, longitudinal stochastic processes, or frequency-domain methods. Exclude when: time appears only as a fixed covariate or panel index with no temporal methodological issue.
- `subject_statistics_spatial` — Spatial statistics, point processes, geostatistics, spatial fields, or areal data methodology. Exclude when: space is merely a location label and the paper has no spatial model or spatial inferential contribution.
- `subject_statistics_survival_biostat` — Survival, event-history, competing-risk, longitudinal biomedical, diagnostic, or clinical statistical methodology. Exclude when: clinical substance dominates and the statistical methods are entirely standard.
- `subject_statistics_design_sampling` — Experimental design, adaptive design, survey sampling, randomization theory, or finite-population inference. Exclude when: the design is an application detail and no design or sampling methodology is contributed.
- `subject_statistics_psychometrics_computational` — Psychometrics, latent-variable models, item response, mixture models, or statistical computation as a methodological contribution. Exclude when: latent constructs or computation are routine components of a primarily substantive application.

### Computer Science
- `subject_computer_science_general` — Computer-science research spanning several areas or outside the listed subfields. Exclude when: a listed computer-science subfield clearly owns the central artifact or theorem.
- `subject_computer_science_algorithms` — Algorithms, data structures, complexity theory, approximation, online algorithms, or theoretical computer science. Exclude when: the algorithm is a routine implementation device for an applied system or statistical model.
- `subject_computer_science_ai_ml` — Machine learning, artificial intelligence, reinforcement learning, generative models, or learning systems as the primary contribution. Exclude when: standard machine learning is used only to measure a substantive phenomenon in another field.
- `subject_computer_science_nlp` — Natural-language processing, computational linguistics systems, language models, or text generation and evaluation. Exclude when: texts are analyzed as social or historical evidence and no NLP method is contributed.
- `subject_computer_science_vision_graphics` — Computer vision, image/video understanding, computer graphics, rendering, or visual computing. Exclude when: imaging is chiefly a scientific measurement instrument or clinical diagnostic rather than a visual-computing contribution.
- `subject_computer_science_systems` — Operating, distributed, cloud, storage, networking, mobile, or high-performance systems. Exclude when: the contribution is a hardware circuit or an application whose systems layer is conventional.
- `subject_computer_science_programming_languages` — Programming languages, type systems, compilers, semantics, program verification, or formal methods. Exclude when: formal proof concerns a mathematical theorem rather than a programming-language or software property.
- `subject_computer_science_databases` — Databases, query processing, transactions, data integration, knowledge bases, or data-management systems. Exclude when: a dataset is created for scientific analysis but no data-management contribution is made.
- `subject_computer_science_security` — Computer security, cryptography applications, privacy, adversarial robustness, or usable security. Exclude when: security appears only as motivation and no threat, attack, defense, or privacy claim is evaluated.
- `subject_computer_science_hci` — Human-computer interaction, CSCW, information visualization, accessibility, or interactive-system research. Exclude when: humans appear only as annotators or users of a system whose contribution is otherwise algorithmic.
- `subject_computer_science_robotics` — Robotics, autonomous agents, planning, control software, embodied AI, or multi-agent systems. Exclude when: the primary contribution is mechanical hardware or control theory without a computational autonomy claim.
- `subject_computer_science_software` — Software engineering, testing, debugging, program analysis, development tools, repositories, or empirical software research. Exclude when: custom research code is merely an implementation artifact and no software-engineering claim is made.

### Engineering
- `subject_engineering_general` — Engineering research spanning several systems or outside the listed engineering subfields. Exclude when: a listed engineering specialty clearly owns the design and performance claim.
- `subject_engineering_electrical_computer` — Circuits, electronics, communications, signal processing, embedded systems, or computer hardware. Exclude when: the contribution is primarily a computer-science system or a physical-material mechanism.
- `subject_engineering_mechanical` — Mechanical design, mechanics, thermofluids, heat transfer, tribology, or mechanical systems. Exclude when: the main result is fundamental fluid physics or materials science without an engineered design claim.
- `subject_engineering_aerospace` — Aeronautical, astronautical, propulsion, flight, spacecraft, or aerospace-systems research. Exclude when: the work concerns atmospheric or plasma physics with no vehicle or mission design objective.
- `subject_engineering_civil` — Civil, structural, geotechnical, construction, infrastructure, or transportation engineering. Exclude when: the paper is primarily urban social science, economics, or descriptive geology.
- `subject_engineering_chemical_process` — Chemical engineering, reactors, separations, catalysis processes, process systems, or scale-up. Exclude when: the central contribution is molecular chemistry without a process or transport claim.
- `subject_engineering_materials` — Materials engineering, metallurgy, ceramics, polymers, composites, processing, or performance design. Exclude when: the work is fundamental condensed-matter physics or synthetic chemistry without an engineering property target.
- `subject_engineering_biomedical` — Medical devices, biomaterials, tissue engineering, biosensors, biomechanics, or biomedical systems. Exclude when: the primary claim is clinical efficacy or basic biology rather than an engineered biomedical artifact.
- `subject_engineering_environmental_energy` — Environmental engineering, energy conversion and storage, water treatment, emissions control, or sustainable systems. Exclude when: the paper is climate or environmental science without a treatment, conversion, or systems-design contribution.
- `subject_engineering_industrial_control` — Industrial engineering, systems engineering, control, operations, manufacturing, automation, or reliability. Exclude when: the main contribution is abstract optimization or a robot-learning algorithm without an industrial system claim.

### Biological Science
- `subject_biology_general` — Biological research spanning several levels of organization or outside the listed biological subfields. Exclude when: a listed biological specialty clearly contains the mechanism and evidence.
- `subject_biology_molecular_cell` — Molecular biology, cell biology, cell signaling, organelles, trafficking, or cellular mechanisms. Exclude when: the primary contribution is organismal physiology, ecology, or a clinical endpoint.
- `subject_biology_genetics_genomics` — Genetics, genomics, epigenomics, population genetics, genome regulation, or functional genomics. Exclude when: sequence data are only a measurement input and no genetic or genomic claim is central.
- `subject_biology_biochemistry_structural` — Biochemistry, enzymology, metabolism, structural biology, biophysics of macromolecules, or molecular interactions. Exclude when: the contribution is synthetic chemistry or materials characterization rather than biological molecular function.
- `subject_biology_development_neuroscience` — Developmental biology, stem cells, neurobiology, neural circuits, or developmental neuroscience. Exclude when: the main contribution is psychological behavior without a biological developmental or neural mechanism.
- `subject_biology_physiology` — Organismal, comparative, integrative, endocrine, cardiovascular, respiratory, or metabolic physiology. Exclude when: the paper is primarily clinical medicine or cell biology without an organism-level functional claim.
- `subject_biology_microbiology_immunology` — Microbiology, virology, host-pathogen biology, immunology, or microbial communities. Exclude when: the central claim is population epidemiology without a microbial or immune mechanism.
- `subject_biology_ecology_evolution` — Ecology, evolutionary biology, behavior, population biology, community ecology, or macroevolution. Exclude when: the work is environmental monitoring without an ecological or evolutionary inference.
- `subject_biology_systems_computational` — Systems biology, bioinformatics, computational biology, network biology, or multi-omics integration. Exclude when: the principal contribution is a general computer-science algorithm with biology only as a benchmark.
- `subject_biology_organismal_behavior` — Zoology, organismal biology, comparative anatomy, functional morphology, animal behavior, or integrative biology. Exclude when: the central contribution is cellular physiology, ecology, or human psychology rather than whole-organism biological function.
- `subject_biology_plant_marine_conservation` — Plant science, marine biology, conservation biology, biodiversity, or applied organismal ecology. Exclude when: the central contribution is agricultural engineering or environmental policy rather than organismal biology.

### Chemistry
- `subject_chemistry_general` — Chemical research spanning several areas or outside the listed chemistry subfields. Exclude when: a listed chemistry specialty clearly contains the main transformation, measurement, or molecular claim.
- `subject_chemistry_organic_biological` — Organic synthesis, reaction methodology, catalysis, medicinal chemistry, or chemical biology. Exclude when: the main result is a biological mechanism with standard chemical probes or an industrial process scale-up.
- `subject_chemistry_inorganic_materials` — Inorganic, organometallic, solid-state, coordination, or materials chemistry. Exclude when: the contribution is primarily device engineering or condensed-matter physics rather than chemical composition and bonding.
- `subject_chemistry_physical_theoretical` — Physical chemistry, spectroscopy, chemical dynamics, quantum chemistry, statistical chemistry, or theoretical chemistry. Exclude when: the contribution is a general physics theory or numerical method without a chemical question.
- `subject_chemistry_analytical_environmental` — Analytical chemistry, separations, sensors, mass spectrometry, electrochemistry, or environmental chemistry. Exclude when: measurement is routine and the substantive contribution lies entirely in another scientific field.

### Earth & Environmental Science
- `subject_earth_environment_general` — Earth, planetary, or environmental research spanning several systems or outside the listed specialties. Exclude when: a listed Earth or environmental subfield clearly owns the principal process and evidence.
- `subject_earth_environment_geology` — Geology, geochemistry, geomorphology, paleoclimate proxies, sedimentology, or tectonics. Exclude when: the central contribution is a physical geophysics inverse problem or modern atmospheric process.
- `subject_earth_environment_climate_atmosphere` — Climate science, meteorology, atmospheric chemistry, weather, or Earth-system dynamics. Exclude when: the paper concerns policy impacts without a material climate or atmospheric scientific contribution.
- `subject_earth_environment_ocean_hydrology` — Oceanography, hydrology, cryosphere, limnology, groundwater, or watershed science. Exclude when: water is only an engineering input or the central mechanism is atmospheric rather than oceanic or hydrologic.
- `subject_earth_environment_sustainability` — Environmental science, biogeochemistry, pollution, ecosystem services, sustainability, or coupled human-natural systems. Exclude when: the contribution is primarily ecological biology, engineering treatment, or policy evaluation.

### Medicine & Health
- `subject_medicine_health_general` — Medical, clinical, or health research spanning several specialties or outside the listed health subfields. Exclude when: a listed clinical, epidemiological, diagnostic, therapeutic, or health-systems lens clearly fits.
- `subject_medicine_health_clinical` — Clinical observational research, prognosis, treatment outcomes, patient management, or specialty medicine. Exclude when: the contribution is a formal trial, diagnostic technology, or population-health study better covered below.
- `subject_medicine_health_epidemiology` — Epidemiology, population health, prevention, infectious-disease spread, environmental health, or health disparities. Exclude when: the paper is a patient-level clinical efficacy study or a biological transmission mechanism without population inference.
- `subject_medicine_health_trials` — Clinical trials, therapeutic development, comparative treatment, dosing, or intervention efficacy and safety. Exclude when: the intervention is nonclinical or the study is purely observational with no trial design.
- `subject_medicine_health_diagnostics` — Diagnostic tests, biomarkers, pathology, medical imaging, screening, prognostic models, or clinical decision support. Exclude when: the main contribution is an imaging algorithm without a clinical diagnostic claim.
- `subject_medicine_health_services` — Health services, implementation, delivery systems, quality, cost, access, policy, or global health. Exclude when: the central contribution is a biomedical treatment effect under tightly controlled clinical conditions.

### Sociology
- `subject_sociology_general` — Sociological research spanning several areas or outside the listed sociological subfields. Exclude when: a listed sociological subfield clearly contains the principal social process.
- `subject_sociology_inequality_demography` — Social stratification, mobility, inequality, demography, family, life course, or population change. Exclude when: the central contribution is a labor or public-economics estimate without a sociological account of structure or group process.
- `subject_sociology_organizations_economic` — Organizations, occupations, professions, markets, work, firms, economic sociology, or institutional fields. Exclude when: the paper models firms or markets without a sociological organizational or relational contribution.
- `subject_sociology_political_movements` — Political sociology, states, citizenship, collective action, protest, social movements, or power. Exclude when: the contribution is primarily electoral behavior, formal institutions, or international relations without a sociological mechanism.
- `subject_sociology_race_migration` — Race, ethnicity, indigeneity, immigration, citizenship, assimilation, boundaries, or transnational communities. Exclude when: group categories are incidental controls and no racial, ethnic, migration, or boundary process is central.
- `subject_sociology_culture_media` — Culture, meaning, classification, knowledge, religion, media, consumption, or cultural production. Exclude when: the paper is primarily textual interpretation without a sociological claim about actors, institutions, or social distribution.
- `subject_sociology_networks` — Social networks, diffusion, relational inequality, social capital, peer structure, or network organizations. Exclude when: graphs are purely technological or biological and social relations are not the substantive object.
- `subject_sociology_urban_community` — Urban sociology, neighborhoods, housing, place, communities, segregation, or local institutions. Exclude when: the main contribution is urban economics, planning engineering, or geography without a sociological community process.
- `subject_sociology_medical_crime` — Medical sociology, health inequality, professions and care, criminology, punishment, law, or deviance. Exclude when: clinical efficacy, epidemiology, or legal doctrine is central without a sociological institution or inequality claim.

### Political Science
- `subject_political_science_general` — Political research spanning several areas or outside the listed political-science subfields. Exclude when: a listed political-science subfield clearly contains the actors, institution, or outcome.
- `subject_political_science_comparative` — Comparative institutions, regimes, democratization, parties, state capacity, governance, or political development. Exclude when: the paper concerns international interactions or one policy without comparative institutional inference.
- `subject_political_science_ir_security` — International relations, security, war, alliances, diplomacy, international organizations, or foreign policy. Exclude when: cross-border economic exchange is central but strategic international politics is not.
- `subject_political_science_ipe` — Trade politics, international finance, sanctions, development institutions, globalization, or cross-border political economy. Exclude when: the contribution is a purely economic trade or finance result without a political institution or distributional mechanism.
- `subject_political_science_institutions` — Legislatures, executives, courts, bureaucracy, federalism, constitutions, corruption, or governance. Exclude when: the central contribution is legal doctrine or organizational sociology without a political institutional claim.
- `subject_political_science_behavior_elections` — Political behavior, public opinion, voting, campaigns, parties, representation, or political communication. Exclude when: the paper studies general social attitudes without a material political behavior or representation claim.
- `subject_political_science_theory` — Normative, analytic, historical, or critical political theory concerning justice, authority, democracy, liberty, or power. Exclude when: the central contribution is empirical political behavior or general moral philosophy without a political institutional object.
- `subject_political_science_policy_admin` — Policy design, implementation, bureaucracy, regulation, public administration, or program governance. Exclude when: the paper estimates a program effect without a material claim about political design, administration, or implementation.
- `subject_political_science_conflict_environment` — Civil conflict, peacebuilding, repression, political violence, resource politics, or environmental governance. Exclude when: the paper is chiefly climate science, criminology, or international war without the relevant domestic conflict or governance mechanism.

### History
- `subject_history_general` — Historical scholarship spanning several periods or themes or outside the listed historical subfields. Exclude when: a listed period or thematic history specialist clearly fits the main intervention.
- `subject_history_ancient` — Ancient Mediterranean, Near Eastern, African, Asian, American, or other pre-medieval history. Exclude when: the central evidence and historiography belong to a later period.
- `subject_history_medieval` — Medieval history across regions, including institutions, religion, economy, society, and material culture. Exclude when: the paper is primarily ancient or early-modern and does not turn on medieval periodization.
- `subject_history_early_modern` — Early-modern state formation, empire, religion, science, commerce, culture, or social change. Exclude when: the central intervention belongs clearly to medieval or modern historiography.
- `subject_history_modern` — Modern or contemporary history, including industrialization, nation-states, colonialism, war, and mass politics. Exclude when: the paper's intervention is primarily a social-science analysis of current outcomes rather than historical explanation.
- `subject_history_economic` — Economic, business, labor, financial, technological, or quantitative history. Exclude when: the contribution is principally an economics estimate with little historiographic or source-based historical argument.
- `subject_history_political_diplomatic` — Political, diplomatic, legal-institutional, military, state, or international history. Exclude when: the paper is an international-relations model using historical cases without a primary historical intervention.
- `subject_history_social_cultural` — Social, cultural, gender, race, family, everyday-life, or subaltern history. Exclude when: culture or inequality is analyzed without a historical source base or historiographic contribution.
- `subject_history_intellectual_global` — Intellectual, religious, science, medicine, global, transnational, colonial, or environmental history. Exclude when: the paper is purely philosophical, literary, or environmental-scientific without a historical intervention.

### Economics
- `subject_economics_general` — Economic research spanning several fields or outside the listed economics subfields. Exclude when: a listed economics field clearly contains the model, data, and contribution.
- `subject_economics_macro` — Macroeconomics, monetary or fiscal policy, growth, business cycles, labor macro, or aggregate dynamics. Exclude when: the paper is household or firm microeconomics without an aggregate equilibrium or macro policy claim.
- `subject_economics_micro_theory` — Microeconomic theory, games, information, contracts, mechanism design, matching, networks, or market design. Exclude when: the formal result is mathematical but has no material economic incentives, allocation, or welfare content.
- `subject_economics_econometrics` — Econometric theory or methodology is itself the main contribution. Exclude when: established econometric tools are applied without a methodological contribution.
- `subject_economics_behavioral_experimental` — Behavioral economics, experimental economics, decision theory with behavioral content, or field and laboratory evidence on economic choice. Exclude when: the contribution is general psychology without an economic choice, incentive, market, or welfare object.
- `subject_economics_public_labor` — Public finance, taxation, social insurance, labor, inequality, education, family, or personnel economics. Exclude when: the principal contribution is sociological stratification or education practice without economic behavior or policy incidence.
- `subject_economics_development_trade` — Development, international trade, migration, political economy of development, or cross-country economic change. Exclude when: international politics or historical narrative is central without an economic allocation or development mechanism.
- `subject_economics_io` — Industrial organization, demand, firm conduct, market power, entry, platforms, auctions, or competition policy. Exclude when: the main contribution is a management strategy or computer platform system without market equilibrium analysis.
- `subject_economics_finance` — Asset pricing, corporate finance, banking, household finance, intermediaries, or market microstructure. Exclude when: the contribution is accounting description or business valuation without a finance mechanism or asset-market claim.
- `subject_economics_health_urban_environment` — Health economics, urban and regional economics, transportation, housing, environmental or energy economics. Exclude when: the contribution is clinical, engineering, geographic, or environmental science without an economic behavior or welfare object.
- `subject_economics_history` — Economic history using economic theory or empirical methods to explain historical development. Exclude when: the contribution is primarily historiographic and source-interpretive without an economic mechanism or estimand.

### Psychology & Cognitive Science
- `subject_psychology_general` — Psychological or cognitive research spanning several areas or outside the listed specialties. Exclude when: a listed psychological subfield clearly contains the construct and evidence.
- `subject_psychology_cognitive` — Cognition, perception, memory, attention, language, learning, or computational cognition. Exclude when: the primary contribution is a neural mechanism, social process, or NLP system rather than cognition.
- `subject_psychology_social_personality` — Social psychology, personality, attitudes, identity, interpersonal behavior, judgment, or decision-making. Exclude when: the main contribution is sociological institutions or political behavior at a collective level.
- `subject_psychology_development_clinical` — Developmental, educational, clinical, health, or psychopathology research focused on psychological processes. Exclude when: the central object is clinical treatment efficacy or biological development rather than psychological theory.
- `subject_psychology_behavioral_neuroscience` — Behavioral neuroscience, cognitive neuroscience, neuropsychology, psychophysiology, or brain-behavior research. Exclude when: the central contribution is cellular neuroscience or medical imaging diagnosis rather than psychological function.
- `subject_psychology_industrial_human_factors` — Work psychology, personnel selection, teams, leadership, occupational behavior, ergonomics, or human factors. Exclude when: the main contribution is management strategy, organizational sociology, or interface design without a psychological construct or human-performance claim.

### Anthropology & Archaeology
- `subject_anthropology_general` — Anthropological or archaeological scholarship spanning several traditions or outside the listed specialties. Exclude when: a sociocultural, biological-linguistic, or archaeological lens clearly fits.
- `subject_anthropology_sociocultural` — Ethnographic, sociocultural, medical, political, economic, or linguistic-cultural anthropology. Exclude when: the paper is primarily survey sociology or textual humanities without ethnographic or anthropological comparison.
- `subject_anthropology_biological_linguistic` — Biological anthropology, human evolution, primatology, linguistic anthropology, or language and culture. Exclude when: the main result is general evolutionary biology or formal linguistics without an anthropological human context.
- `subject_anthropology_archaeology` — Archaeology, material culture, heritage, bioarchaeology, or archaeological science. Exclude when: material objects are chiefly art-historical texts or geological specimens without archaeological context.

### Geography
- `subject_geography_general` — Geographic research spanning human, physical, and geospatial approaches or outside the listed specialties. Exclude when: a listed geographic subfield clearly contains the spatial process.
- `subject_geography_human_urban` — Human, economic, political, urban, cultural, or development geography. Exclude when: the contribution is primarily sociology, economics, or political science without a geographic account of space and place.
- `subject_geography_physical` — Physical geography, biogeography, geomorphology, landscape, human-environment, or environmental change. Exclude when: the main contribution is a narrow Earth-system mechanism without a geographic landscape or spatial synthesis.
- `subject_geography_gis` — Geographic information science, cartography, spatial data infrastructure, remote sensing, or geocomputation. Exclude when: GIS is a routine tool and no geographic measurement, representation, or spatial-method contribution is made.

### Philosophy
- `subject_philosophy_general` — Philosophical work spanning several areas or outside the listed philosophical specialties. Exclude when: a listed philosophical area clearly contains the principal argument.
- `subject_philosophy_logic_epistemology` — Philosophical logic, epistemology, rational belief, formal epistemology, or philosophy of language. Exclude when: the central result is mathematical logic or empirical cognition rather than a philosophical account of knowledge or meaning.
- `subject_philosophy_metaphysics_mind` — Metaphysics, ontology, modality, causation, time, personal identity, consciousness, or philosophy of mind. Exclude when: the contribution is empirical neuroscience or psychology without a substantive metaphysical or philosophy-of-mind argument.
- `subject_philosophy_ethics_political` — Normative ethics, metaethics, applied ethics, political philosophy, justice, rights, or responsibility. Exclude when: the central contribution is empirical policy analysis or political theory grounded primarily in historical interpretation.
- `subject_philosophy_science_history` — Philosophy of science, biology, physics, social science, medicine, or historically grounded philosophy. Exclude when: the paper is history of science without a philosophical claim or science without conceptual analysis.

### Linguistics
- `subject_linguistics_general` — Linguistic research spanning several levels or outside the listed linguistic specialties. Exclude when: a formal, sound-structure, or social-historical linguistic lens clearly fits.
- `subject_linguistics_syntax_semantics` — Syntax, formal semantics, pragmatics, discourse, morphology-syntax, or grammatical theory. Exclude when: the primary contribution is an NLP model or literary interpretation without a linguistic grammar claim.
- `subject_linguistics_sound_structure` — Phonetics, phonology, morphology, speech production or perception, and sound or word structure. Exclude when: speech is merely an engineering signal and no linguistic sound-structure claim is made.
- `subject_linguistics_social_historical` — Sociolinguistics, language variation and change, historical linguistics, documentation, corpus or computational linguistics with a linguistic contribution. Exclude when: the contribution is chiefly sociology, history, or computer science without a linguistic account of language structure or use.

### Education
- `subject_education_general` — Education research spanning policy, learning, instruction, institutions, or assessment. Exclude when: a listed education specialty clearly contains the intervention or institution.
- `subject_education_policy` — Education policy, school choice, finance, accountability, teacher labor markets, access, or inequality. Exclude when: the paper is economics of education without material educational institutions or practice to assess.
- `subject_education_learning` — Learning sciences, pedagogy, curriculum, classroom instruction, teacher practice, or educational technology. Exclude when: the contribution is a general cognitive theory or technology with no educational learning claim.
- `subject_education_higher_measurement` — Higher education, special education, educational measurement, assessment, admissions, or institutional student support. Exclude when: the contribution is psychometric theory or organizational policy without a substantive education question.

### Law
- `subject_law_general` — Legal scholarship spanning several domains or outside the listed legal specialties. Exclude when: a listed doctrinal, private-criminal, international, or empirical-legal lens clearly fits.
- `subject_law_public` — Constitutional, administrative, regulatory, legislation, courts, public law, or governance doctrine. Exclude when: the primary question is international, private, or criminal law without a material public-law issue.
- `subject_law_private_criminal` — Contracts, torts, property, corporations, commercial, family, criminal law, procedure, or punishment. Exclude when: the central contribution is public regulation or empirical crime analysis without doctrinal private or criminal law.
- `subject_law_international` — Public or private international law, human rights, trade law, comparative law, transnational regulation, or conflict of laws. Exclude when: international relations or comparative politics is central but no legal-authority or doctrinal claim is made.
- `subject_law_empirical_economic` — Empirical legal studies, law and economics, legal institutions, judicial behavior, or quantitative doctrinal consequences. Exclude when: the contribution is an economics or political-science result with law only as background.

### Business & Management
- `subject_business_general` — Business or management research spanning several functions or outside the listed specialties. Exclude when: a listed strategy, operations, accounting-finance, or marketing lens clearly contains the contribution.
- `subject_business_strategy_organization` — Strategy, organization theory, organizational behavior, entrepreneurship, innovation, or human resources. Exclude when: the contribution is industrial organization economics or sociology without a managerial or firm-strategy object.
- `subject_business_operations_information` — Operations management, supply chains, service systems, analytics, information systems, or digital operations. Exclude when: the central contribution is an engineering or computer-science system without an organizational operating decision.
- `subject_business_accounting_finance` — Accounting, auditing, disclosure, governance, corporate finance, capital markets, or taxation in firms. Exclude when: the primary contribution is asset-pricing or public-finance economics without a firm reporting or governance question.
- `subject_business_marketing` — Marketing, consumer behavior, branding, pricing, channels, sales, advertising, or customer analytics. Exclude when: the main contribution is general psychology or demand estimation without a marketing decision or market context.

### Humanities
- `subject_humanities_general` — Humanities scholarship spanning several traditions or outside the listed literary, religious, visual, media, or digital fields. Exclude when: a listed humanities specialty clearly contains the primary objects and scholarly conversation.
- `subject_humanities_literature_classics` — Literary studies, comparative literature, classics, philology, rhetoric, or book history. Exclude when: the central contribution is historical fact, formal linguistics, or automated text analysis without literary interpretation.
- `subject_humanities_religion` — Religious studies, theology, scriptural interpretation, ritual, doctrine, or religion and society. Exclude when: religion is only a demographic category and no religious text, practice, institution, or theological argument is central.
- `subject_humanities_art_music` — Art history, architectural history, visual culture, musicology, performance, or material aesthetics. Exclude when: images or music are data inputs without an art-historical, architectural, or musicological contribution.
- `subject_humanities_media_cultural` — Film, television, media, communication, cultural studies, popular culture, games, or performance studies. Exclude when: the contribution is a technical media system or quantitative communication effect without interpretive cultural analysis.
- `subject_humanities_digital_public` — Digital humanities, public humanities, archives and editions, cultural heritage, museums, or computational cultural analysis. Exclude when: the central contribution is a general computer-science method or public-history narrative without a humanities research object.

### Agriculture & Veterinary Science
- `subject_agriculture_veterinary_general` — Agricultural, food, forestry, fisheries, animal, or veterinary research spanning several systems or outside the listed specialties. Exclude when: a listed agricultural, food, animal, veterinary, forestry, or rural-systems lens clearly contains the main contribution.
- `subject_agriculture_veterinary_crop_soil` — Agronomy, crop science, horticulture, soil science, agroecology, plant breeding, or crop protection. Exclude when: the contribution is basic plant biology without a managed production or agroecosystem claim.
- `subject_agriculture_veterinary_animal` — Animal science, veterinary medicine, livestock systems, animal health, breeding, welfare, or comparative clinical research. Exclude when: the central contribution is human clinical medicine, wildlife ecology, or cellular biology without an animal-health or production-system claim.
- `subject_agriculture_veterinary_food` — Food chemistry, processing, preservation, sensory science, food microbiology, quality, or food safety. Exclude when: the contribution is human nutrition, molecular chemistry, or process engineering without a food-system quality or safety claim.
- `subject_agriculture_veterinary_forestry_fisheries` — Forestry, silviculture, fisheries science, aquaculture, rangelands, or renewable biological-resource management. Exclude when: the central contribution is conservation biology or environmental policy without a managed harvest, production, or resource-system claim.
- `subject_agriculture_veterinary_systems_rural` — Farming systems, agricultural extension, rural development, food systems, farm management, or technology adoption. Exclude when: the contribution is development economics or rural sociology without material agricultural production, extension, or food-system expertise.

### Communication & Information
- `subject_communication_information_general` — Communication, journalism, information, or knowledge-institution research spanning several areas or outside the listed specialties. Exclude when: a listed communication, information, risk-communication, or interpersonal lens clearly contains the central process.
- `subject_communication_information_media_journalism` — Journalism studies, mass communication, news, political communication, public relations, or media institutions. Exclude when: the contribution is interpretive media studies without a communication-process claim or political behavior without a material media institution.
- `subject_communication_information_library` — Library and information science, archives, knowledge organization, information behavior, scholarly communication, or information institutions. Exclude when: the central contribution is database engineering or digital humanities without an information-practice or institution claim.
- `subject_communication_information_science_health_risk` — Communication of science, medicine, environment, uncertainty, crisis, hazards, or public risk. Exclude when: the contribution is clinical, environmental, or engineering risk assessment without a communicative process or audience claim.
- `subject_communication_information_interpersonal_organizational` — Interpersonal, organizational, health, family, group, or computer-mediated communication. Exclude when: the main contribution is social psychology or management without a material message, interaction, discourse, or communication-system claim.

### Architecture, Planning & Design
- `subject_architecture_design_general` — Architecture, planning, landscape, or design scholarship spanning several areas or outside the listed specialties. Exclude when: a listed built-environment, planning, landscape, or design lens clearly contains the central artifact or intervention.
- `subject_architecture_design_built_environment` — Architectural design and theory, building science, housing design, interiors, heritage conservation, or built-environment research. Exclude when: the contribution is structural engineering or art history without a material architectural performance, use, or design argument.
- `subject_architecture_design_planning` — Urban and regional planning, land use, transportation planning, housing, infrastructure governance, or community development. Exclude when: the central contribution is urban economics, geography, or civil engineering without a planning institution, process, or intervention.
- `subject_architecture_design_landscape` — Landscape architecture, ecological design, public space, site planning, or landscape performance. Exclude when: the work is ecosystem science or geography without a designed landscape, site, or public-space claim.
- `subject_architecture_design_human_centered` — Industrial, product, service, interaction, participatory, or human-centered design where design knowledge is the contribution. Exclude when: the central contribution is HCI evaluation, mechanical engineering, or marketing without a material design inquiry or artifact claim.

### Social Work & Social Policy
- `subject_social_work_policy_general` — Social work, human services, welfare, or community-intervention research spanning several areas or outside the listed specialties. Exclude when: a listed practice, welfare-policy, or community-organization lens clearly contains the intervention and outcome.
- `subject_social_work_policy_practice` — Clinical and direct social work, child and family services, mental-health services, case management, safeguarding, or human-service delivery. Exclude when: the contribution is clinical treatment efficacy or organizational management without a social-work practice, service, or person-in-environment claim.
- `subject_social_work_policy_welfare` — Welfare states, poverty policy, social protection, disability policy, family policy, housing support, or comparative social policy. Exclude when: the paper is public economics or political administration without a material welfare institution, service-user, or social-policy contribution.
- `subject_social_work_policy_community_nonprofit` — Community practice, nonprofit and voluntary organizations, mutual aid, social development, advocacy, or collective service provision. Exclude when: the main contribution is organizational sociology or public administration without a community-practice, voluntary-sector, or service mission.

### Health Professions
- `subject_health_professions_general` — Nursing, rehabilitation, pharmacy, oral health, nutrition, or allied-health research spanning several professions or outside the listed specialties. Exclude when: a listed health-profession lens or a medicine-and-health specialist clearly contains the care process and outcome.
- `subject_health_professions_nursing` — Nursing science, midwifery, care delivery, symptom management, patient safety, or nursing workforce research. Exclude when: the central contribution is physician-led clinical efficacy or health-services economics without a nursing or care-process claim.
- `subject_health_professions_rehabilitation` — Physical, occupational, speech, vocational, or multidisciplinary rehabilitation; disability and functioning research. Exclude when: the contribution is basic motor science or acute medical treatment without a rehabilitation, participation, or functioning claim.
- `subject_health_professions_pharmacy` — Pharmacy practice, medication use and safety, pharmacotherapy, pharmacoepidemiology, pharmacovigilance, or medicines policy. Exclude when: the central contribution is molecular pharmacology, a clinical trial, or health economics without a medication-use or pharmacy-practice claim.
- `subject_health_professions_dentistry` — Dentistry, oral medicine, dental materials in clinical use, oral epidemiology, prevention, or oral-health services. Exclude when: the main contribution is materials engineering or general epidemiology without an oral-health mechanism, procedure, or care claim.
- `subject_health_professions_nutrition_exercise` — Human nutrition, dietetics, exercise science, sports medicine, physical activity, or lifestyle intervention research. Exclude when: the contribution is food chemistry, elite performance engineering, or population epidemiology without a material nutrition, exercise, or professional-practice claim.

### Interdisciplinary Studies
- `subject_interdisciplinary_studies_general` — Interdisciplinary scholarship whose central contribution cannot be evaluated adequately from one listed discipline or more specific interdisciplinary field. Exclude when: one or two listed disciplinary or interdisciplinary subfield reviewers can cover the paper's central contribution without a general fallback.
- `subject_interdisciplinary_studies_sts` — Science and technology studies, sociology or anthropology of knowledge, infrastructure studies, innovation studies, or critical data and algorithm studies. Exclude when: the contribution is philosophy or history of science, technical system design, or innovation economics without a material knowledge-practice or sociotechnical claim.
- `subject_interdisciplinary_studies_gender_sexuality` — Gender, sexuality, feminist, queer, masculinity, or intersectional studies across social-scientific and humanistic traditions. Exclude when: gender or sexuality appears only as a demographic covariate and no category, institution, identity, representation, or power relation is central.
- `subject_interdisciplinary_studies_race_indigenous` — Ethnic studies, race and diaspora studies, Indigenous studies, settler-colonial studies, or community-grounded scholarship across fields. Exclude when: race, ethnicity, or indigeneity is only a control variable and no historical, institutional, cultural, territorial, or knowledge-governance claim is central.
- `subject_interdisciplinary_studies_area_global` — Regional, area, transnational, postcolonial, development, or global studies integrating language, history, institutions, and contemporary evidence. Exclude when: the contribution fits a specific history, politics, economics, geography, literature, or anthropology subfield without material interdisciplinary regional synthesis.

METHOD SPECIALIST CATALOG

- `formal_proofs` — Central theorems, propositions, lemmas, or nontrivial formal derivations require proof verification. Exclude when: routine algebra, a purely conceptual argument, or results whose proof is not material to the main claims.
- `economic_model_logic` — Economic assumptions, equilibrium, incentives, mechanisms, comparative statics, incidence, or welfare carry central claims. Exclude when: non-economic formal models or empirical economics with no material model-based mechanism.
- `causal_identification` — Observational or quasi-experimental variation is used for a material causal claim. Exclude when: descriptive association, prediction, randomized treatment assignment, or causal language confined to motivation.
- `randomized_experiment` — Random assignment, encouragement, or a randomized intervention supports a central conclusion. Exclude when: natural experiments, simulation experiments, or laboratory work without randomized assignment of the focal intervention.
- `structural_estimation` — Estimated structural parameters or model-based counterfactuals are central. Exclude when: reduced-form estimation, calibration without estimation, or a theoretical structural model with no fitted parameters.
- `quantitative_computation` — Calibration, numerical solution, dynamic computation, inverse problems, or model counterfactuals support central results. Exclude when: routine data processing or computation that does not affect the substantive result.
- `statistical_validity` — Statistical inference, estimation, uncertainty, prediction, or sampling claims require dedicated scrutiny. Exclude when: a paper with no stochastic evidence, estimated quantities, or uncertainty claims.
- `measurement_data` — Data construction, measurement, sampling frames, linkage, coding, or descriptive facts materially support the paper. Exclude when: a paper that uses only standard public measures without a material measurement or data-construction claim.
- `simulation_numerics` — Monte Carlo evidence, numerical approximation, discretization, simulation accuracy, or synthetic experiments are central. Exclude when: numerical implementation is routine and no conclusion depends on approximation or simulation behavior.
- `algorithmic_ml` — Algorithms, prediction systems, learning procedures, benchmarks, or computational complexity are central. Exclude when: a standard classifier or software package is merely used as a control or preprocessing device.
- `network_analysis` — Relational data, graph construction, centrality, communities, diffusion, link prediction, or network dependence materially support a central claim. Exclude when: a graph is only a computational data structure or visual illustration with no substantive relational inference.
- `computational_text_analysis` — Dictionary, topic, embedding, classifier, large-language-model, corpus, or other text-as-data measurements support substantive claims about documents, speakers, or discourse. Exclude when: the contribution is an NLP algorithm itself or a close reading that does not rely on automated text measurement.
- `qualitative_case_study` — Interviews, ethnography, process tracing, qualitative coding, participant observation, or comparative cases carry central claims. Exclude when: archival source criticism alone, purely textual interpretation, or cases used only as illustrations.
- `conceptual_argument` — Conceptual, synthetic, normative, or argumentative reasoning carries the central contribution. Exclude when: the central claims instead turn on formal proof, measured evidence, experiments, or source interpretation.
- `laboratory_experiment` — Controlled laboratory manipulation, physical or biological assay, bench experiment, or apparatus-based experiment supplies central evidence. Exclude when: randomized social interventions, purely observational measurements, or computational simulations without physical experiments.
- `observational_science` — Instrument-derived observational data, field measurements, surveys of natural systems, detection pipelines, or observational selection functions support central claims. Exclude when: designed laboratory interventions or social-science causal identification where the main issue is treatment assignment.
- `clinical_study` — Patient cohorts, diagnostic or prognostic models, clinical interventions, medical endpoints, or translational claims are central. Exclude when: nonclinical laboratory biology or population research with no patient-facing or diagnostic inference.
- `survey_research` — Questionnaire design, respondent sampling, weighting, nonresponse, reporting behavior, or survey experiments materially determine the evidence. Exclude when: administrative or sensor data with no survey instrument, or surveys used only for a minor control variable.
- `design_based_research` — Iterative design, research-through-design, design-based implementation, prototyping with users, or practice-based inquiry is itself the evidentiary strategy. Exclude when: ordinary product engineering, a one-shot usability test, or an intervention evaluated without a material iterative-design claim.
- `creative_practice_research` — Artistic, performative, curatorial, compositional, literary, or other creative practice is used to generate and substantiate a central research claim. Exclude when: a creative work is only the object of interpretation, or an artifact is evaluated primarily as a functional design intervention.
- `participatory_community_research` — Community-based participatory research, action research, co-production, citizen science, or stakeholder-governed inquiry materially shapes the evidence and claims. Exclude when: participants merely provide data or feedback without shared agenda setting, interpretation, governance, or action.
- `archival_source_criticism` — Archival records, manuscripts, legal or administrative documents, material archives, or primary-source provenance carry historical claims. Exclude when: secondary-source synthesis or interviews and ethnography without a material archival evidentiary problem.
- `historical_comparative` — Periodization, sequence, path dependence, comparative cases, or process-based historical explanation supports the central argument. Exclude when: a single contemporaneous case with no historical or comparative explanatory claim.
- `textual_interpretive` — Close reading, hermeneutics, discourse analysis, rhetoric, translation, or interpretation of cultural texts carries the contribution. Exclude when: automated text measurement alone or texts used only as sources of factual observations.
- `legal_doctrinal` — Interpretation of cases, statutes, regulations, constitutional provisions, precedent, or institutional legal authority is central. Exclude when: empirical legal studies whose main claims do not depend on doctrinal interpretation.
- `geospatial_remote_sensing` — GIS construction, spatial resolution, remote-sensing retrievals, map projections, geolocation, or spatial dependence materially support results. Exclude when: a paper that merely includes a map or location fixed effects without a substantive geospatial measurement problem.
- `systematic_review_meta_analysis` — Evidence search, study inclusion, effect harmonization, evidence grading, or quantitative synthesis across studies is central. Exclude when: an ordinary narrative literature review or paper citing several prior estimates without systematic synthesis.
- `bibliometric_scientometric` — Publication, citation, authorship, collaboration, patent, or scholarly-communication records are analyzed to make central claims about research systems or knowledge structure. Exclude when: citations are used only for literature positioning or a systematic review synthesizes study findings rather than publication-system patterns.
- `mixed_methods` — The contribution depends on integrating qualitative and quantitative evidence rather than presenting them as independent appendages. Exclude when: a paper with multiple methods whose conclusions do not rely on their integration.
- `reproducibility_software` — Custom software, computational workflows, data pipelines, package behavior, or reproducible artifacts materially support the scientific result. Exclude when: routine use of standard software with no software, workflow, or reproducibility claim.
- `engineering_validation` — Prototype testing, tolerances, reliability, standards, verification, failure modes, scale-up, or safety margins support an engineering claim. Exclude when: basic scientific experiments with no design-performance, reliability, or safety claim.
- `research_ethics_governance` — Consent, participant or animal welfare, community authority, data governance, conflicts, dual-use risk, or responsible deployment materially affects the validity or permissible scope of the research claim. Exclude when: ethics approval is routine, adequately documented, and not material to interpreting or disseminating the central findings.

Routing rules:

1. Select one primary subject specialist. Select a second subject specialist only when the central contribution genuinely crosses two bodies of subject-matter expertise and one reviewer would predictably miss a material issue. The array is ordered: primary first, secondary second.
2. Prefer the most specific fitting subfield. Use a discipline-level fallback only when no listed subfield fits. Never select both a discipline fallback and one of its own subfields.
3. Select between one and four method specialists. Select a method only when it materially supports or tests a central claim; a technique mentioned in passing is not enough.
4. Subject and method roles are complementary. A mathematics subject reviewer does not replace formal-proof review when proofs carry the result; a clinical subject reviewer does not replace clinical-study review when patient evidence carries the conclusion.
5. Pure theory and pure mathematics papers must not receive causal, randomized, clinical, measurement, survey, qualitative, archival, or laboratory reviewers unless the paper truly contains that component.
6. Descriptive empirical work without a causal claim should generally receive measurement/data and statistical-validity review, not causal-identification review. Randomized interventions need randomized-experiment review; observational causal designs need causal-identification review; structural counterfactuals need structural-estimation review.
7. Distinguish physical laboratory work, observational scientific instrumentation, clinical patient research, survey research, archival sources, qualitative cases, textual interpretation, and computational simulation. Select the role matching the evidence actually used.
8. Formal-proofs is for results whose validity depends on proofs or nontrivial derivations. Economic-model-logic is for incentives, equilibrium, mechanisms, incidence, welfare, or comparative statics. Conceptual-argument is for conceptual, synthetic, or normative reasoning. These roles may coexist when each is central.
9. Quantitative-computation covers calibrated or numerically solved models and counterfactuals. Simulation-numerics covers approximation error, discretization, Monte Carlo behavior, or synthetic experiments. Algorithmic-ML covers claims about algorithms, learning systems, benchmarks, or complexity. Engineering-validation covers performance against requirements, prototypes, tolerances, and failure modes.
10. Use network-analysis only when relational construction or network dependence is substantively important. Use computational-text-analysis when automated text measurement supports a substantive conclusion, not when the paper contributes an NLP system. Use bibliometric-scientometric only for claims about publication, citation, collaboration, patent, or knowledge-system records.
11. Use systematic-review/meta-analysis only when evidence synthesis itself is a central method. Use reproducibility/software when central conclusions depend on a nontrivial computational implementation, pipeline, or released research artifact.
12. Use design-based/practice research when iterative functional, educational, or service design is the evidentiary strategy. Use creative/practice-led research when artistic or performative practice generates the scholarly claim. Use participatory/community research when shared authority or co-production is methodologically material. Use research-ethics/governance only when consent, welfare, data authority, conflicts, responsible release, or dual-use risk materially affects validity or permissible scope; routine approval alone is not enough.
13. The catalog contains academic-review roles only. Classify sensitive or dual-use work at a high level from the paper's stated discipline and evidence. Do not reproduce, extend, or infer operationally harmful procedures in the routing notes.
14. If classification is genuinely ambiguous, record the ambiguity. Do not compensate by indiscriminately selecting reviewers; choose the smallest set that covers the material uncertainty.
15. `selection_notes` must contain exactly one entry for every selected subject and method ID, and no unselected IDs. Give paper-specific reasons rather than restating catalog labels.

Inventory rules:

- List every proposition, theorem, lemma, corollary, definition, and explicit assumption in `formal_results`.
- List every explicitly defined symbol in `notation`; do not invent definitions.
- Record the paper's own contribution claim in `stated_contribution`.
- Flag garbled or incomplete extraction rather than treating it as an error in the paper.
- Return ONLY valid JSON. No markdown fences or commentary.

<paper>
{paper_text}
</paper>
