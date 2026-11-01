export interface DiffRow {
  kind: "hunk" | "add" | "del" | "context" | "note";
  text: string;
  oldLine: number | null;
  newLine: number | null;
}

const HUNK = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@(.*)$/;

const note = (text: string): DiffRow => ({
  kind: "note",
  text,
  oldLine: null,
  newLine: null,
});

function modeNote(from: string, to: string): DiffRow {
  if (to === "100755")
    return note("Permissions changed: this file can now be run as a program.");
  if (from === "100755")
    return note(
      "Permissions changed: this file can no longer be run as a program.",
    );
  return note(`Permissions changed from ${from} to ${to}.`);
}

/** Rows of one file's unified diff. Git's file header is dropped; the page
 * names the file, and a change the header alone records becomes a note.
 * Combined merge diffs have no `@@ ` hunks and come back empty. */
export function parseUnifiedDiff(text: string): DiffRow[] {
  const rows: DiffRow[] = [];
  let oldLine = 0;
  let newLine = 0;
  let inHunk = false;
  let oldMode = "";
  let renamed = false;
  let hunks = 0;
  for (const line of text.split("\n")) {
    const hunk = HUNK.exec(line);
    if (hunk) {
      inHunk = true;
      hunks++;
      oldLine = Number(hunk[1]);
      newLine = Number(hunk[2]);
      rows.push({
        kind: "hunk",
        text: hunk[3].trim(),
        oldLine: null,
        newLine: null,
      });
    } else if (!inHunk) {
      if (line.startsWith("old mode ")) oldMode = line.slice(9);
      else if (line.startsWith("new mode "))
        rows.push(modeNote(oldMode, line.slice(9)));
      else if (line.startsWith("rename to ")) renamed = true;
    } else if (line.startsWith("+")) {
      rows.push({
        kind: "add",
        text: line.slice(1),
        oldLine: null,
        newLine: newLine++,
      });
    } else if (line.startsWith("-")) {
      rows.push({
        kind: "del",
        text: line.slice(1),
        oldLine: oldLine++,
        newLine: null,
      });
    } else if (line.startsWith(" ")) {
      rows.push({
        kind: "context",
        text: line.slice(1),
        oldLine: oldLine++,
        newLine: newLine++,
      });
    } else if (line.startsWith("\\")) {
      rows.push(note("No newline at end of file"));
    } else if (line.startsWith("diff --git ")) {
      // A renamed file diffed by both names can produce a second section.
      inHunk = false;
    }
  }
  if (renamed && !hunks)
    rows.push(note("Renamed. The contents did not change."));
  return rows;
}

/** A new file's lines, shown as additions. */
export function newFileRows(text: string): DiffRow[] {
  const lines = text.endsWith("\n") ? text.slice(0, -1) : text;
  return lines.split("\n").map((line, index) => ({
    kind: "add",
    text: line,
    oldLine: null,
    newLine: index + 1,
  }));
}

/** A conflicted file with Git's markers marked out as the two sides. */
export function conflictRows(text: string): DiffRow[] {
  let side: "add" | "del" | "context" = "context";
  return text.split("\n").map((line, index) => {
    const row = { oldLine: null, newLine: index + 1, text: line };
    if (line.startsWith("<<<<<<< ")) {
      side = "del";
      return { ...row, kind: "hunk", text: "This branch" };
    }
    if (line.startsWith("=======") && side === "del") {
      side = "add";
      return { ...row, kind: "hunk", text: "Incoming" };
    }
    if (line.startsWith(">>>>>>> ") && side === "add") {
      side = "context";
      return { ...row, kind: "hunk", text: "End of conflict" };
    }
    return { ...row, kind: side };
  });
}
