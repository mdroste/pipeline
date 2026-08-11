# Auto Review specialist catalog

Paper Review (Auto) separates the stable workflow from the specialist catalog.
The saved profile has four steps: three universal reviews and one synthesis.
After orientation, Rust validates the routing plan and materializes only the
selected reviewers. A normal run therefore has six to ten steps:

- three universal reviewers;
- one primary subject specialist and, for genuinely interdisciplinary work,
  one secondary subject specialist;
- one to four method specialists; and
- synthesis.

The orientation model chooses stable IDs only. Prompts, tools, artifact access,
providers, model policy, phase, and synthesis selectors remain host-owned.

The Workflow Editor represents this without pretending the full catalog is a
saved workflow. It shows one combined **Orientation & Classification** stage,
then two read-only **Auto-filled from orientation** slots for subject and method
specialists. The slots link to the searchable catalog. At run time, each chosen
role becomes an ordinary parallel step, and every resulting specialist report
is added directly to synthesis beside the three universal reports.

## Prompt composition

A subject prompt has four layers:

1. `prompts/auto_review/subjects/_review_contract.md` defines the common
   referee, evidence, safety, and report-output contract.
2. One discipline lens in `prompts/auto_review/subjects/` supplies shared
   domain standards.
3. `auto_review/subjects.rs` supplies the subfield-specific review focus.
4. The same catalog entry supplies an explicit scope boundary used both in the
   routing prompt and the assembled reviewer prompt.

Method prompts combine one method-specific file under
`prompts/auto_review/methods/` with the shared evidence, calibration, safety,
and Markdown output contract in `_review_contract.md`.
`auto_review/methods.rs` supplies their routing descriptions and exclusions.
This keeps long-form reusable instructions in Markdown and compact routing
metadata in one typed catalog.

## Subject coverage

Each discipline has a broad fallback and more specific subfield roles. The
router must prefer a specific role and may not select a fallback together with
one of its own subfields.

| Discipline | Subfield coverage |
|---|---|
| Physics | theoretical and mathematical; particle and nuclear; condensed matter and materials; AMO and quantum optics; statistical and complex systems; astrophysics and cosmology; plasma and fluid; geophysics; quantum information |
| Mathematics | algebra and number theory; geometry and topology; analysis; PDE and calculus of variations; probability; combinatorics; logic and foundations; applied and numerical; optimization, control, and dynamics |
| Statistics | theory and asymptotics; Bayesian; causal and semiparametric; high-dimensional and learning; time series; spatial; survival and biostatistics; design and sampling; latent variables and computation |
| Computer science | algorithms and complexity; AI and machine learning; NLP; vision and graphics; systems and networking; programming languages and formal methods; databases; security and privacy; HCI; robotics; software engineering |
| Engineering | electrical and computer; mechanical; aerospace; civil and structural; chemical and process; materials; biomedical; environmental and energy; industrial, control, and operations |
| Biological science | molecular and cell; genetics and genomics; biochemistry and structural; development and neuroscience; physiology; microbiology and immunology; ecology and evolution; systems and computational; organismal and behavior; plant, marine, and conservation |
| Chemistry | organic and biological; inorganic and materials; physical and theoretical; analytical and environmental |
| Earth and environmental science | geology and geochemistry; climate and atmosphere; ocean and hydrology; environmental systems and sustainability |
| Medicine and health | clinical medicine; epidemiology and public health; clinical trials and therapeutics; diagnostics and prognosis; health services and global health |
| Sociology | inequality and demography; organizations and economic sociology; politics and movements; race and migration; culture and media; networks; urban sociology; medical sociology and crime |
| Political science | comparative politics; international relations and security; international political economy; institutions; behavior and elections; political theory; public policy and administration; conflict and environmental politics |
| History | ancient; medieval; early modern; modern; economic; political and diplomatic; social and cultural; intellectual; global and environmental |
| Economics | macroeconomics; microeconomic theory; econometrics; behavioral and experimental; public, labor, and education; development and international; industrial organization; finance; health, urban, and environmental; economic history |
| Psychology and cognitive science | cognitive; social and personality; developmental and clinical; behavioral neuroscience; industrial, organizational, and human factors |
| Anthropology and archaeology | sociocultural; biological and linguistic; archaeology |
| Geography and spatial science | human and urban; physical and environmental; GIS and spatial science |
| Philosophy | logic and epistemology; metaphysics and mind; ethics and political philosophy; philosophy and history of science |
| Linguistics | syntax and semantics; phonetics and phonology; sociolinguistics, historical, and computational |
| Education | policy and economics; learning and curriculum; higher, special, and assessment |
| Law | public law; private and criminal law; international law; empirical and economic analysis of law |
| Business and management | strategy and organizations; operations and information systems; accounting and finance; marketing |
| Humanities | literature and classics; religion; art and music; media and cultural studies; digital and public humanities |
| Agriculture and veterinary science | crop, soil, and horticulture; animal and veterinary; food science and safety; forestry, fisheries, and aquaculture; agricultural systems and rural development |
| Communication and information | media, journalism, and public communication; library and information science; science, health, and risk communication; interpersonal and organizational communication |
| Architecture, planning, and design | architecture and built environment; urban and regional planning; landscape architecture; human-centered and product design |
| Social work and social policy | practice and human services; welfare and social policy; community and nonprofit studies |
| Health professions | nursing and care science; rehabilitation, disability, and occupational therapy; pharmacy and pharmacoepidemiology; dentistry and oral health; nutrition, dietetics, and exercise |
| Interdisciplinary studies | science, technology, and society; gender and sexuality; race, ethnicity, and Indigenous studies; area and global studies |

## Method coverage

The method catalog contains:

- formal proofs;
- economic model logic;
- causal identification;
- randomized experiments;
- structural estimation;
- quantitative computation;
- statistical validity;
- measurement and data construction;
- simulation and numerics;
- algorithms and machine learning;
- network analysis;
- computational text analysis;
- qualitative and case evidence;
- conceptual argument;
- laboratory experiments;
- observational science and instrumentation;
- clinical studies and diagnostic validity;
- survey research;
- design-based and practice research;
- creative and practice-led research;
- participatory and community research;
- archival and primary-source criticism;
- historical and comparative reasoning;
- textual and interpretive analysis;
- doctrinal legal reasoning;
- geospatial and remote sensing;
- systematic reviews and meta-analysis;
- bibliometric and scientometric analysis;
- mixed-methods integration;
- reproducibility and research software;
- engineering validation and safety; and
- research ethics and governance.

Subject and method roles are intentionally orthogonal. A mathematics subject
reviewer does not replace proof verification; a clinical subject reviewer does
not replace clinical-study design review; and a history subject reviewer does
not replace primary-source criticism.

## Adding a specialist

1. Add a stable `subject_*` or method ID to the relevant typed catalog. IDs are
   durable provenance and rerun keys, so do not rename an existing ID casually.
2. Give the router a positive description and a concrete exclusion. The
   exclusion is important for avoiding attractive but irrelevant reviewers.
3. For a subject role, reuse its discipline lens and add a focused review
   paragraph. Add a new discipline Markdown lens only for a genuinely new
   discipline. For a method role, add a focused Markdown prompt; both role
   types inherit their shared evidence and report-output contract.
4. Keep the role's executable properties host-owned. Do not add catalog-wide
   steps or a general expression/template engine.
5. Run the Auto Review catalog, schema, materialization, migration, and UI
   preview tests. The saved profile must remain four steps and the materialized
   workflow must remain below the 100-step profile limit.

The legacy `prompts/auto_review/fields/` directory and `auto_review/legacy.rs`
exist only to recognize untouched Auto v1 profiles and rerun their saved
reports. New routing must use the subject hierarchy.
