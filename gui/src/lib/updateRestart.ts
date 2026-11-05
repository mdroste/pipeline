import { invoke } from "@tauri-apps/api/core";
import { confirmDialog } from "../components/DialogService";

export interface RestartWork {
  unsavedChanges: boolean;
  reviews: boolean;
  conversations: boolean;
  automations: boolean;
}

function joined(items: string[]): string {
  if (items.length < 2) return items.join("");
  return `${items.slice(0, -1).join(", ")} and ${items[items.length - 1]}`;
}

/** What a restart would interrupt, or null when nothing would be lost. */
export function restartWarning(work: RestartWork): string | null {
  const running = [
    work.reviews && "reviews",
    work.conversations && "conversations",
    work.automations && "automations",
  ].filter((item): item is string => Boolean(item));
  const effects = [
    work.unsavedChanges && "discard your unsaved changes",
    running.length > 0 && `stop running ${joined(running)}`,
  ].filter((item): item is string => Boolean(item));
  return effects.length > 0 ? `Restarting will ${joined(effects)}.` : null;
}

/** Ask before a restart that would interrupt work. */
export async function confirmUpdateRestart(
  work: Omit<RestartWork, "automations">,
): Promise<boolean> {
  const automations = await invoke<number>("update_running_automations").catch(
    () => 0,
  );
  const warning = restartWarning({ ...work, automations: automations > 0 });
  if (!warning) return true;
  return confirmDialog(warning, {
    title: "Restart Pipeline now?",
    confirmLabel: "Restart now",
    destructive: true,
  });
}
