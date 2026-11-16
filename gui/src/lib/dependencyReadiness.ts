import type { DepsReport, DepStatus } from "./settingsTypes";

export const PROVIDER_LABELS: Record<string, string> = {
  "Claude CLI": "Claude",
  "Codex CLI": "ChatGPT",
  "ChatGPT (reviews)": "ChatGPT",
  "Google API": "Gemini",
  "Local LLM server": "Local model",
};

export const isProviderDependency = (dep: DepStatus) =>
  Object.prototype.hasOwnProperty.call(PROVIDER_LABELS, dep.name);

export function dependencyAvailable(dep: DepStatus): boolean {
  if (!dep.found) return false;
  if (dep.authenticated !== undefined) return dep.authenticated;
  if (dep.cli_auth_status !== undefined)
    return dep.cli_auth_status === "signed_in";
  return true;
}

/** General setup needs one usable provider. A particular review still uses
 * the backend's report.ready, which requires every provider it will call. */
export function systemDependenciesReady(report: DepsReport): boolean {
  return (
    report.ready ||
    (report.deps.some(
      (dep) => isProviderDependency(dep) && dependencyAvailable(dep),
    ) &&
      report.deps.every(
        (dep) =>
          isProviderDependency(dep) ||
          !dep.required ||
          dependencyAvailable(dep),
      ))
  );
}
