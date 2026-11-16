import { expect, it } from "vitest";
import type { DepStatus } from "./settingsTypes";
import { systemDependenciesReady } from "./dependencyReadiness";

const provider = (name: string, authenticated: boolean): DepStatus => ({
  name,
  found: true,
  authenticated,
  required: true,
  version: "",
  path: "",
  hint: "",
});

it.each([
  [true, false, true],
  [false, true, true],
  [true, true, true],
  [false, false, false],
])(
  "Claude %s and ChatGPT %s give system readiness %s",
  (claude, chatgpt, ready) => {
    expect(
      systemDependenciesReady({
        ready: claude && chatgpt,
        deps: [
          provider("Claude CLI", claude),
          provider("ChatGPT (reviews)", chatgpt),
        ],
      }),
    ).toBe(ready);
  },
);

it.each(["signed_out", "unknown"] as const)(
  "does not count an installed CLI with %s authentication as connected",
  (cli_auth_status) => {
    expect(
      systemDependenciesReady({
        ready: false,
        deps: [
          {
            ...provider("Claude CLI", false),
            authenticated: undefined,
            cli_auth_status,
          },
        ],
      }),
    ).toBe(false);
  },
);

it("accepts direct API access even when the provider CLI is signed out", () => {
  expect(
    systemDependenciesReady({
      ready: false,
      deps: [
        {
          ...provider("Claude CLI", true),
          version: "direct API",
          cli_auth_status: "signed_out",
        },
        provider("ChatGPT (reviews)", false),
      ],
    }),
  ).toBe(true);
});

it("keeps required parsing failures visible with a connected provider", () => {
  expect(
    systemDependenciesReady({
      ready: false,
      deps: [
        provider("ChatGPT (reviews)", true),
        { ...provider("pdftotext", false), found: false },
      ],
    }),
  ).toBe(false);
});

it("accepts an available local model without requiring cloud sign-in", () => {
  expect(
    systemDependenciesReady({
      ready: false,
      deps: [
        provider("Claude CLI", false),
        { ...provider("Local LLM server", true), authenticated: undefined },
      ],
    }),
  ).toBe(true);
});
