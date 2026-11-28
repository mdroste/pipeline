import type { ProviderLimitNotice } from "../lib/reviewRuntimeTypes";
import { isMac } from "../lib/platform";

function providerName(provider: string | null): string {
  if (!provider) return "fallback provider";
  if (provider === "claude") return "Claude";
  if (provider === "codex") return "ChatGPT";
  if (provider === "antigravity") return "Antigravity";
  if (provider === "local") return "Local";
  return provider;
}

export default function ProviderLimitBanner({
  notices,
  onOpenSettings,
}: {
  notices: ProviderLimitNotice[];
  onOpenSettings: () => void;
}) {
  const blocking = [...notices]
    .reverse()
    .find(
      (notice) =>
        notice.status === "exhausted" || notice.status === "fallback_failed",
    );
  const latest = blocking ?? notices[notices.length - 1];
  if (!latest) return null;
  const recovered = notices.filter(
    (notice) => notice.status === "recovered",
  ).length;
  const fallback = latest.fallback_provider
    ? `${providerName(latest.fallback_provider)}${latest.fallback_model ? ` (${latest.fallback_model})` : ""}`
    : null;
  const summary = blocking
    ? latest.status === "fallback_failed"
      ? `${providerName(latest.provider)} reached its account usage limit, and ${fallback ?? "the fallback"} also failed.`
      : `${providerName(latest.provider)} reached its account usage limit. No backup provider is set.`
    : latest.status === "recovered"
      ? `${providerName(latest.provider)} reached its account usage limit. Pipeline continued with ${fallback}.`
      : `${providerName(latest.provider)} reached its account usage limit. Switching this call to ${fallback}.`;

  return (
    <div
      role="alert"
      // On macOS the banner row drags the window; its button still clicks.
      data-tauri-drag-region={isMac ? "deep" : undefined}
      className={`flex shrink-0 items-center gap-3 border-b px-5 py-2.5 text-xs ${
        blocking
          ? "border-danger bg-danger-soft text-danger"
          : "border-warning bg-warning-soft text-warning"
      }`}
    >
      <span className="min-w-0 flex-1">
        <span className="font-semibold">Model usage limit reached.</span>{" "}
        {summary}
        {recovered > 1 ? ` ${recovered} calls have used the fallback.` : ""}
      </span>
      <button
        type="button"
        onClick={onOpenSettings}
        className="shrink-0 rounded border border-[color:color-mix(in_srgb,currentColor_30%,transparent)] px-2 py-1 font-medium hover:bg-white/50 dark:hover:bg-black/20"
      >
        Fallback settings
      </button>
    </div>
  );
}
