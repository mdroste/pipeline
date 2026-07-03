Build a structured survey of the input below. The survey will be given to several independent analysis steps as shared JSON context, so make it a faithful map of what the input contains — not an assessment or critique.

Produce a single JSON object. Choose keys that fit the input. A useful survey typically covers:

{
  "overview": "one-paragraph summary of what the input is and what it is for",
  "structure": [
    {"part": "name of a major part", "role": "what it does or covers", "location": "file, section, or page"}
  ],
  "key_elements": [
    {"name": "an important element (definition, function, claim, dataset, ...)", "what_it_is": "...", "location": "..."}
  ],
  "conventions": ["notable notation, naming, or formatting conventions used in the input"],
  "cross_references": ["places where one part depends on or refers to another"],
  "quality_notes": ["gaps, garbled text, or missing pieces in the input itself"]
}

Rules:
- Return ONLY valid JSON — no markdown fences, no commentary.
- Be exhaustive on structure: every major part of the input should appear.
- Record locations precisely (file path, section number, page) so later steps can navigate directly to them.
- Report what is there, not what you think of it.

<input>
{input_text}
</input>
