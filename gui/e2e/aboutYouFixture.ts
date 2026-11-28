// Settings → About you for the app-full fixture. `?about=empty` shows the
// first-run state. Never connects to native IPC.
/* eslint-disable @typescript-eslint/no-explicit-any */

export function aboutYouCommands(params: URLSearchParams, now: string): Record<string, unknown> {
  const researcherProfile: any = {
    about: params.get("about") === "empty" ? "" : "Labor economist. Applied micro with administrative data, mostly difference-in-differences and event studies. I write for general-interest economics journals. British spelling.",
    style: "",
    samples: params.get("about") === "empty" ? [] : [
      { id: "3f2a9c1b7d4e8a60", title: "Minimum wages and firm entry", year: 2024, kind: "paper", fileName: "minimum_wages_final.pdf", extension: "pdf", contentHash: "3f2a9c1b7d4e8a60".repeat(4), words: 11200, useAsExample: true, addedAt: "2026-11-20T10:00:00Z" },
      { id: "8b1d5e0f2c7a9346", title: "NSF SES proposal", year: 2023, kind: "grant", fileName: "nsf-ses-2023.docx", extension: "docx", contentHash: "8b1d5e0f2c7a9346".repeat(4), words: 6400, useAsExample: true, addedAt: "2026-11-18T10:00:00Z" },
      { id: "c4e7a2906b3d1f58", title: "Reply to referees scan", year: null, kind: "response", fileName: "reply_scan.pdf", extension: "pdf", contentHash: "c4e7a2906b3d1f58".repeat(4), words: null, useAsExample: false, addedAt: "2026-11-10T10:00:00Z" },
    ],
    folder: "/Users/researcher/.pipeline/researcher",
    maxNoteBytes: 4096,
    maxSamples: 20,
  };
  return {
    researcher_profile: () => researcherProfile,
    researcher_save_note: (args: any) => {
      researcherProfile[args.note === "about" ? "about" : "style"] = args.text.trim();
      return researcherProfile;
    },
    researcher_add_sample: (args: any) => {
      const fileName = String(args.path).split(/[\\/]/).pop() ?? "paper.pdf";
      const id = (Date.now().toString(16) + "0000000000000000").slice(0, 16);
      researcherProfile.samples.unshift({
        id, title: fileName.replace(/\.[^.]+$/, "").replace(/[_-]+/g, " "), year: null, kind: "paper", fileName,
        extension: fileName.split(".").pop() ?? "pdf", contentHash: id.repeat(4), words: 9400, useAsExample: true, addedAt: now,
      });
      return researcherProfile;
    },
    researcher_update_sample: (args: any) => {
      researcherProfile.samples = researcherProfile.samples.map((sample: any) =>
        sample.id === args.update.id ? { ...sample, ...args.update } : sample);
      return researcherProfile;
    },
    researcher_remove_sample: (args: any) => {
      researcherProfile.samples = researcherProfile.samples.filter((sample: any) => sample.id !== args.id);
      return researcherProfile;
    },
    researcher_open_folder: () => null,
    workbench_draft_style_notes: () => ({
      text: [
        "Write in the first person plural.",
        "Open each paragraph with its claim in one short sentence.",
        "State the estimate and its units before interpreting it.",
        "Hedge with “suggests” or “is consistent with”, once per claim.",
        "Use British spelling.",
      ].join("\n"),
      papers: researcherProfile.samples.filter((sample: any) => sample.useAsExample && sample.words !== null).map((sample: any) => sample.title),
    }),
  };
}
