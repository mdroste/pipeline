# Review specialist catalog

Paper Review picks its subject and method specialists from this catalog. Each
specialist, and each document type, is one folder with a `specialist.json`
manifest and a Markdown prompt. The build checks every manifest against
`specialist.schema.json` and generates the Rust catalog the app uses.

| Folder | Manifest | Prompt |
| --- | --- | --- |
| `subjects/{id}/` | Routing details for one subfield | `focus.md`, the subfield's review focus |
| `methods/{id}/` | Routing details for one method | `prompt.md`, the method's review focus |
| `genres/{id}/` | Routing details for one document type | `prompt.md`, a short paragraph added to every reviewer's prompt |

## How prompts are assembled

A subject reviewer's prompt has four parts:

1. the shared reviewer instructions in `../subjects/_review_contract.md`;
2. the discipline's lens in `../subjects/{group}.md`;
3. the subfield's `focus.md`; and
4. the scope note from its manifest, which the router also sees.

A method reviewer's prompt combines its `prompt.md` with the shared
instructions in `../methods/_review_contract.md`. A genre prompt says how to
judge that kind of document and what not to ask of it. It's added to every
step when the paper is classified as that genre.

## Add a specialist

1. Add a folder under `subjects/`, `methods/`, or `genres/`. The folder name
   must match the `id` in `specialist.json`. Never rename an existing ID,
   because saved runs and reruns refer to it.
2. Assign the next `order` number in its group. A method family has at most
   one family-level fallback.
3. Write a description of when to pick the specialist and a concrete
   exclusion saying when not to. The exclusion keeps the router from picking
   reviewers that sound relevant but aren't.
4. Add the prompt. A subject uses its discipline's existing lens plus a new
   `focus.md`. Add a new lens only for a new discipline. A method adds
   `prompt.md`. A genre adds a short `prompt.md` with no report format of its
   own.
5. Run the review catalog, schema, and designer preview tests. The tests read
   their expected counts from the manifests, so a new specialist needs no
   test update.

The build rejects malformed manifests, duplicate IDs, missing prompts, gaps
in a group's order, and a second fallback in one family. The saved Paper
Review workflow stays at five steps. The specialists are added when a run
starts.
