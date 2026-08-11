export const ISSUES_SCHEMA = {
  type: "object",
  required: ["issues"],
  properties: {
    issues: {
      type: "array",
      items: {
        type: "object",
        required: ["title", "severity", "body"],
        properties: {
          id: { type: "string" },
          title: { type: "string" },
          severity: { type: "string" },
          section: { type: "string" },
          body: { type: "string" },
          evidence: {
            type: "array",
            items: {
              type: "object",
              properties: {
                page: { type: "integer" },
                node_id: { type: "string" },
                asset_id: { type: "string" },
                artifact_path: { type: "string" },
                description: { type: "string" },
                quote: { type: "string" },
              },
            },
          },
        },
      },
    },
  },
};
