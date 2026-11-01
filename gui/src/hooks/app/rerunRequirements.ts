import { confirmDialog, promptDialog } from "../../components/DialogService";
import { appClient } from "../../lib/appClient";

/**
 * Settle what a past run needs before it is queued: the researcher's agreement
 * to anything they must know, and its secret variables, whose values a run
 * never keeps. Resolves to the secrets, or null when the researcher cancels.
 */
export async function confirmRerunRequirements(
  runId: string,
  onlyFailed: boolean,
): Promise<Record<string, string> | null> {
  const { secrets, notice } = await appClient.rerunRequirements(
    runId,
    onlyFailed,
  );
  if (
    notice &&
    !(await confirmDialog(notice, {
      title: "Review design not saved",
      confirmLabel: "Continue",
    }))
  ) {
    return null;
  }
  const variables: Record<string, string> = {};
  for (const secret of secrets) {
    const value = await promptDialog(
      `${secret.label} is not saved with the run. Enter it again to continue.`,
      {
        title: "Enter a secret value",
        label: secret.label,
        confirmLabel: "Continue",
        concealed: true,
      },
    );
    if (value === null) return null;
    variables[secret.key] = value;
  }
  return variables;
}
