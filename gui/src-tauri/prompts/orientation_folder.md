Build a structured survey of the input folder described below. The survey will be given to several independent analysis steps as shared context, so make it a faithful map of what the folder contains — not an assessment or critique.

The input below is a file inventory: paths and sizes only. Do not build the survey from file names alone. Use the Read tool to open the files you need: start with any README or documentation, then manifests and configuration (package.json, Cargo.toml, requirements.txt, Makefile, run scripts, ...), then the main entry points and a representative file from each major directory.

Populate the supplied folder-survey schema from files you actually inspect.

Survey rules:
- Be exhaustive on components: every major directory and top-level file should appear.
- Record locations as exact relative paths from the inventory root so later steps can open them directly.
- List in "key_files" only files you actually opened or that the documentation identifies as central.
- Trace important imports, includes, data reads, and generated outputs across files.
- Record referenced-but-missing, empty, unreadable, or truncated material as quality notes.
- Report what is there, not what you think of it.

<input>
{input_text}
</input>
