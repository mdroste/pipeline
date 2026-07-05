Build a structured survey of the input folder described below. The survey will be given to several independent analysis steps as shared JSON context, so make it a faithful map of what the folder contains — not an assessment or critique.

The input below is a file inventory: paths and sizes only. Do not build the survey from file names alone. Use the Read tool to open the files you need: start with any README or documentation, then manifests and configuration (package.json, Cargo.toml, requirements.txt, Makefile, run scripts, ...), then the main entry points and a representative file from each major directory.

Produce a single JSON object. Choose keys that fit the folder. A useful survey typically covers:

{
  "overview": "one-paragraph summary of what the folder is and what it is for",
  "components": [
    {"path": "directory or top-level file", "role": "what it does or covers", "notes": "languages, frameworks, or data formats"}
  ],
  "entry_points": [
    {"path": "...", "purpose": "where execution, the build, or reading starts"}
  ],
  "key_files": [
    {"path": "...", "what_it_is": "important script, definition, dataset, document, ..."}
  ],
  "conventions": ["notable naming, layout, or formatting conventions"],
  "cross_references": [
    "places where one file depends on or refers to another (imports, \\input, data files read by scripts, figures written by code)"
  ],
  "quality_notes": ["files referenced but missing, empty directories, unreadable or truncated files"]
}

Rules:
- Return ONLY valid JSON — no markdown fences, no commentary.
- Be exhaustive on components: every major directory and top-level file should appear.
- Record locations as exact relative paths from the inventory root so later steps can open them directly.
- List in "key_files" only files you actually opened or that the documentation identifies as central.
- Report what is there, not what you think of it.

<input>
{input_text}
</input>
