import { memo } from "react";
import { PROVIDERS } from "../../lib/providers";

function AgentChips({
  agents,
  multi,
  onChange,
}: {
  agents: string[];
  multi?: boolean;
  onChange: (agents: string[]) => void;
}) {
  return (
    <div>
      <label className="block text-xs font-medium text-gray-500 dark:text-gray-400 mb-1.5">
        LLM Agents
        <span className="font-normal text-gray-600 dark:text-gray-400 ml-1">
          (empty = global setting
          {multi ? "; select multiple to run in parallel" : ""})
        </span>
      </label>
      <div
        role="group"
        aria-label={multi ? "LLM agents (multiple allowed)" : "LLM agent"}
        className="flex gap-2"
      >
        {PROVIDERS.map((provider) => {
          const active = agents.includes(provider);
          return (
            <button
              key={provider}
              type="button"
              aria-pressed={active}
              onClick={() => {
                if (multi) {
                  onChange(
                    active
                      ? agents.filter((a) => a !== provider)
                      : [...agents, provider],
                  );
                } else {
                  onChange(active ? [] : [provider]);
                }
              }}
              className={`px-3 py-1 text-xs rounded-full border transition-colors ${
                active
                  ? "bg-gray-900 dark:bg-gray-100 text-white dark:text-gray-900 border-gray-900 dark:border-gray-100"
                  : "bg-white dark:bg-gray-800 text-gray-500 dark:text-gray-400 border-gray-300 dark:border-gray-600 hover:border-gray-400 dark:hover:border-gray-500"
              }`}
            >
              {provider.charAt(0).toUpperCase() + provider.slice(1)}
            </button>
          );
        })}
      </div>
    </div>
  );
}

// --- Prompt dialog ---

export default memo(AgentChips);
