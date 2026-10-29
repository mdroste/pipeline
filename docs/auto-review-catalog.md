# Review specialists

Paper Review selects reviewers for the document's subject and methods. A
mathematics paper can receive both a subject review and a proof review; an
empirical paper can receive a field review and an assessment of its research
design. The tables below describe the available coverage.

For instructions on running a review, see [Reviews](reviews.md).

## How reviewers are selected

An initial classification identifies the field, methods, and document type.
Paper Review then runs:

- three core assessments;
- one or two subject specialists;
- one to four method specialists;
- a consolidation step that combines the reports into up to forty comments; and
- a validation step that checks those comments against the paper, corrects
  details where possible, and removes comments it cannot support.

This produces seven to eleven review steps, excluding the initial
classification. Paper Review (Fast) omits the separate contribution-and-literature
assessment and uses at most two method specialists, for six to eight steps.
Slide Review uses the same selection process with instructions tailored to a
slide deck.

A specialist's selection reason appears with its instructions. Reviewers also
receive guidance for the document type, such as a replication study, survey,
comment, or slide deck. Subject and method reviews serve different purposes:
a field specialist does not replace a separate assessment of the method.

In the designer, **Orientation & Classification** precedes the
**Auto-filled from orientation** subject and method slots. Open the catalog
from those slots to browse the available reviewers. The selected reviewers
appear as individual parallel steps when the run starts.

## Implementation reference

The saved Paper Review profile contains three core reviewers, consolidation,
and validation. Rust validates the classification and adds the selected
specialists for that run. Every reviewer report feeds consolidation. Validation
reads the primary input, orientation, and consolidated report; it does not read
the individual parallel reports.

The model selects catalog IDs. Pipeline supplies each role's prompt, tools,
file access, provider settings, and output routing. All generated steps declare
optional web search; whether it is available depends on the provider.

Document genres are validated against the catalog, with `research_article` as
the default. Genre guidance is included in each step and uses no specialist
slot. Slide Review fixes the genre to `genre_slide_deck` through its orientation
schema. The host accepts a fixed genre only if it exists in the current catalog.

Selection reasons are bounded, quoted model text that reviewers must check
against the document. They cannot change a step's permissions or configuration.

## Prompt composition

A subject prompt has four layers, all Markdown:

1. `prompts/auto_review/subjects/_review_contract.md` defines the common
   referee, evidence, safety, and report-output contract.
2. One discipline lens in `prompts/auto_review/subjects/` supplies shared
   domain standards.
3. `prompts/auto_review/catalog/subjects/{id}/focus.md` supplies the
   subfield-specific review focus; the adjacent `specialist.json` holds its
   compact routing metadata.
4. The same manifest supplies an explicit scope boundary used both in the
   routing prompt and the assembled reviewer prompt.

Method prompts combine one method-specific file under
`prompts/auto_review/catalog/methods/{id}/prompt.md` with the shared evidence,
calibration, safety, and Markdown output contract in
`prompts/auto_review/methods/_review_contract.md`.
Genre entries under `prompts/auto_review/catalog/genres/{id}/prompt.md` are compact context
paragraphs, not referee prompts: each states how to judge that kind of
document and what not to demand of it, and is injected verbatim into every
step's prompt when its genre is classified. Every role's adjacent
`specialist.json` supplies its classification description and exclusions.
The build validates every manifest and prompt, then generates the typed Rust
catalog consumed by routing and the editor.

## Catalog-backed orientation schema

The saved Auto Review orientation schema does not enumerate every role. Its
selection fields use compact `x-pipeline-catalog` references for subjects,
methods, and genres, with the live-catalog policy set explicitly at the schema
root. `selection_notes.id` does not repeat the combined subject-and-method
enum: host semantic validation already requires its IDs to match the selected
specialists exactly. Before any provider call, Pipeline resolves the three
catalog references to ordinary JSON Schema enums. The same resolved schema is
used for strict host validation, followed by routing-specific semantic checks
such as note coverage, fallback conflicts, and valid subject/method placement.

The Workflow Editor shows both the compact schema and the compact router prompt
template. The prompt contains subject, method, and genre catalog placeholders;
Pipeline expands them from the live manifests only when building the provider
call. The editor summarizes the catalog without crowding the canvas and offers
the fully resolved provider schema as an advanced view. Each Auto Review run
fingerprints all manifest metadata and prompt content; that catalog revision is
included in the launch fingerprint and recorded in the run manifest.

## Subject coverage

Each discipline has a broad fallback and more specific subfield roles. The
router must prefer a specific role and may not select a fallback together with
one of its own subfields. A few subfield entries — the economics General
roles, Machine Learning & AI (General), and Astrophysics & Cosmology
(General) — are themselves cross-field fallbacks whose exclusions defer to
narrower siblings.

| Discipline | Subfield coverage |
|---|---|
| Physics | theoretical and mathematical; particle and nuclear; condensed matter and materials; AMO and quantum optics; statistical and complex systems; astrophysics and cosmology (general); stellar and exoplanetary; galactic and extragalactic; cosmology and gravitation; plasma and fluid; geophysics; quantum information; biological and soft-matter physics; optics and photonics; applied physics and devices |
| Mathematics | algebra and number theory; geometry and topology; analysis; PDE and calculus of variations; probability; combinatorics; logic and foundations; applied and numerical; optimization, control, and dynamics; algebraic geometry and representation theory; stochastic processes and mathematical finance; mathematical physics |
| Statistics | theory and asymptotics; Bayesian; causal and semiparametric; high-dimensional and learning; time series; spatial; survival and biostatistics; design and sampling; latent variables and computation; nonparametric and robust methods; multivariate and functional data; experimental design and adaptive trials |
| Computer science | algorithms and complexity; machine learning and AI (general); LLMs and foundation models; reinforcement learning and decision-making; AI safety, alignment, and evaluation; ML systems and efficiency; NLP; vision and graphics; systems and networking; programming languages and formal methods; databases; security and privacy; HCI; robotics; software engineering; distributed and cloud systems; information retrieval and data mining; scientific computing and computational science |
| Engineering | electrical and computer; mechanical; aerospace; civil and structural; chemical and process; materials; biomedical; environmental and energy; industrial, control, and operations; robotics and mechatronics; nuclear engineering; power and energy systems |
| Biological science | molecular and cell; genetics and genomics; biochemistry and structural; development and neuroscience; physiology; microbiology and immunology; ecology and evolution; systems and computational; organismal and behavior; plant, marine, and conservation |
| Neuroscience | cellular and molecular; systems and circuits; cognitive and human; computational and theoretical; clinical and translational |
| Chemistry | organic and biological; inorganic and materials; physical and theoretical; analytical and environmental |
| Earth and environmental science | geology and geochemistry; climate and atmosphere; paleoclimate and reconstruction; biogeochemistry and carbon cycle; climate impacts and adaptation; ocean and hydrology; environmental systems and sustainability |
| Medicine and health | clinical medicine; epidemiology and public health; clinical trials and therapeutics; diagnostics and prognosis; health services and global health; cardiovascular, metabolic, and renal medicine; oncology and hematology; neurology, psychiatry, and mental health |
| Sociology | inequality and demography; demography and population; organizations and economic sociology; politics and movements; race and migration; culture and media; networks; urban sociology; medical sociology and crime; family and life course; crime, law, and punishment; science, knowledge, and professions |
| Political science | comparative politics; international relations and security; international political economy; institutions; behavior and elections; political theory; public policy and administration; conflict and environmental politics; political methodology; domestic political economy; identity, representation, and citizenship |
| History | ancient; medieval; early modern; modern; economic; political and diplomatic; social and cultural; intellectual; global and environmental; science, medicine, and technology; empire, colonialism, and decolonization; environmental and climate history |
| Economics | general and cross-field macroeconomics; monetary economics and banking; growth; international macroeconomics; labor macroeconomics; microeconomic theory; econometrics; behavioral and experimental; public finance; labor; education; development; international trade; industrial organization; finance; health; urban, regional, and transportation; environmental and energy; economic history |
| Psychology and cognitive science | cognitive; social and personality; developmental and clinical; behavioral neuroscience; industrial, organizational, and human factors |
| Anthropology and archaeology | sociocultural; biological and linguistic; archaeology; medical anthropology; political and economic anthropology; linguistic anthropology |
| Geography and spatial science | human and urban; physical and environmental; GIS and spatial science |
| Philosophy | logic and epistemology; metaphysics and mind; ethics and political philosophy; philosophy and history of science |
| Linguistics | syntax and semantics; phonetics and phonology; sociolinguistics, historical, and computational |
| Education | policy and economics; learning and curriculum; higher, special, and assessment |
| Law | public law; private and criminal law; international law; empirical and economic analysis of law |
| Business and management | strategy and organizations; operations and information systems; accounting and finance; marketing; organizational behavior and human resources; entrepreneurship and innovation; supply chain and operations management |
| Humanities | literature and classics; religion; art and music; media and cultural studies; digital and public humanities; film, theater, and performance; translation, philology, and book history; museums, heritage, and material culture |
| Agriculture and veterinary science | crop, soil, and horticulture; animal and veterinary; food science and safety; forestry, fisheries, and aquaculture; agricultural systems and rural development |
| Communication and information | media, journalism, and public communication; library and information science; science, health, and risk communication; interpersonal and organizational communication |
| Architecture, planning, and design | architecture and built environment; urban and regional planning; landscape architecture; human-centered and product design |
| Social work and social policy | practice and human services; welfare and social policy; community and nonprofit studies |
| Health professions | nursing and care science; rehabilitation, disability, and occupational therapy; pharmacy and pharmacoepidemiology; dentistry and oral health; nutrition, dietetics, and exercise |
| Interdisciplinary studies | science, technology, and society; gender and sexuality; race, ethnicity, and Indigenous studies; area and global studies; metascience and science of science |

## Method coverage

The method catalog is grouped into families. Within a family, entries marked
*(fallback)* are the family's broad role, selected only for approaches its
specific siblings do not list, for contributions spanning several of them, or
when a separate general issue in that family is central. Entries must stay
contiguous by family through manifest `order` values; the orientation prompt
and the catalog browser render the same grouping. Build validation rejects
gaps, duplicate IDs, malformed groups, and duplicate family fallbacks.

| Family | Roles |
|---|---|
| Formal Theory & Conceptual Analysis | formal proofs; economic model logic; conceptual argument |
| Causal Inference & Policy Evaluation | causal identification *(fallback)*; regression discontinuity; difference-in-differences and event studies; instrumental variables; matching and weighting; synthetic control; mediation and mechanism; bunching and notch designs; shift-share and exposure designs; time-varying treatments and g-methods; ML-based heterogeneous effects; Mendelian randomization; financial event studies; external validity and transportability; interference and spillovers; sufficient statistics and welfare analysis |
| Experiments & Trials | randomized experiments; field experiments; laboratory experiments; survey experiments and conjoint designs; audit and correspondence studies |
| Statistical Modeling & Inference | statistical validity *(fallback)*; Bayesian inference; time series and forecasting; panel and longitudinal; spatial dependence; survival and event history; missing data and attrition; multiplicity and selective reporting; high-dimensional regularization; latent variables and psychometrics; uncertainty and sensitivity analysis; partial identification and bounds; quantile and distributional effects; extreme values and tail risk |
| Measurement, Data & Records | measurement and data construction *(fallback)*; survey research; administrative data and record linkage; ML-derived measures and downstream inference |
| Computation, Simulation & Modeling | quantitative computation *(fallback)*; structural estimation; optimization and control; inverse problems and data assimilation; finite elements and discretization; simulation and numerics; agent-based modeling; policy microsimulation; electronic-structure computation; atomistic and molecular simulation; climate modeling and projection; energy systems and integrated assessment; scientific ML and surrogate models |
| Machine Learning & AI | algorithms and machine learning *(fallback)*; LLM and foundation-model evaluation; LLMs as research instruments; prediction-model development and validation; algorithmic fairness and audits; formal privacy and disclosure; benchmark and dataset construction; quantum-advantage claims |
| Networks, Text & Digital Traces | network analysis; computational text analysis; bibliometric and scientometric analysis; digital traces and platform data; corpus linguistics |
| Health & Clinical Research | clinical studies and diagnostic validity; observational epidemiology; genomic and omics assays; preclinical animal studies; neuroimaging analysis; PK/PD and dose-response; infectious-disease transmission modeling; health economic evaluation; implementation and process evaluation; GWAS and polygenic scores |
| Laboratory & Instrumentation | observational science and instrumentation *(fallback)*; microscopy and scientific imaging; sensors and signal processing; materials characterization; geospatial and remote sensing; analytical-chemistry validation; synthesis and compound characterization; crystallography and structure determination |
| Qualitative & Interpretive | qualitative and case evidence *(fallback)*; ethnography; interviews and focus groups; process tracing; comparative and configurational analysis; textual and interpretive analysis; mixed methods; participatory and community research; creative and practice-led research; discourse and conversation analysis |
| Sources, History & Law | archival and primary-source criticism; historical and comparative reasoning; doctrinal legal reasoning; textual criticism and critical editions; archaeological field methods |
| Synthesis, Evaluation & Meta-research | systematic reviews and meta-analysis; metascience and large-scale replication; theory-based program evaluation; expert elicitation and Delphi |
| Engineering, Design & Applied Evaluation | engineering validation and safety; design-based and practice research; HCI and usability studies; life-cycle assessment |
| Reproducibility, Integrity & Governance | reproducibility and research software; research ethics and governance; statistical-reporting consistency; image and data integrity signals; citation accuracy; replication-package audit |

## Document-genre context

Orientation classifies the manuscript as the kind of document it presents
itself as — `research_article` by default, or one of:

- replication and reanalysis studies;
- comments and replies;
- survey and review articles;
- data and resource descriptors;
- methods and tool papers;
- registered reports and preregistered studies;
- null and negative results;
- case reports and case series;
- research software papers; and
- slide decks.

The classification is not a reviewer selection and consumes no specialist
slot. For a non-article genre, every reviewer receives the same host-owned
"Document Genre" context section, so a comment is not faulted for lacking a
free-standing contribution and a null-result paper is not faulted for the
null itself.

Subject and method roles remain intentionally orthogonal. A mathematics
subject reviewer does not replace proof verification, and a clinical subject
reviewer does not replace clinical-study design review.

## Adding a specialist

1. Add one directory under `catalog/subjects/`, `catalog/methods/`, or
   `catalog/genres/`. Its directory name and `specialist.json` ID must match.
   IDs are durable provenance and rerun keys, so do not rename an existing ID
   casually. Subject and method entries declare a group; at most one method
   per family is `family`-level.
2. Give the router a positive description and a concrete exclusion. The
   exclusion is important for avoiding attractive but irrelevant reviewers.
3. For a subject role, reuse its discipline lens and add `focus.md` beside the
   manifest. Add a new discipline Markdown lens only for a new
   discipline. For a method role, add `prompt.md` beside the manifest; both
   reviewer types inherit their shared evidence and report-output contract.
   For a genre, add a compact `prompt.md` context paragraph with no output
   contract of its own.
4. Keep the role's executable properties host-owned. Do not add catalog-wide
   steps or a general expression/template engine.
5. Run the Auto Review catalog, schema, materialization, and UI preview tests.
   `specialist.schema.json` is compiled and applied to every manifest by
   `build.rs`; catalog-size tests derive their expectations from the manifests,
   so adding a valid role does not require updating a frozen count.
   The saved profile must remain five steps and the materialized workflow must
   remain below the 100-step profile limit.

The current manifests, schema references, and generated catalog define the
reviewer set. The app does not keep separate pinned or legacy catalogs.
