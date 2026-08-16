# Auto Review catalog

Each specialist or document genre is one self-contained directory validated at
build time against `specialist.schema.json`.

- `subjects/{id}/specialist.json` contains routing metadata and
  `subjects/{id}/focus.md` contains the role-specific review focus. Subject
  roles reuse their group's discipline lens from `../subjects/{group.id}.md`.
- `methods/{id}/specialist.json` contains routing metadata and
  `methods/{id}/prompt.md` contains the method-specific review focus.
- `genres/{id}/specialist.json` contains routing metadata and
  `genres/{id}/prompt.md` contains the context injected into every reviewer.

To add a role, add one directory, give it a stable ID, and assign the next
contiguous `order` within its catalog. The build rejects malformed manifests,
duplicate IDs, missing prompts, non-contiguous groups, and duplicate group
fallbacks before the application compiles.
