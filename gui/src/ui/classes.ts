// Canonical utility-class vocabulary for the app's shared look. The legacy
// per-directory `shared` modules re-export these, so every feature area draws
// one button, input, and card. The Tailwind config aliases `gray` to the
// neutral scale, so gray-* here is the app's single neutral vocabulary.

export const button =
  "rounded-md border border-gray-300 px-3 py-1.5 text-xs hover:bg-gray-50 disabled:opacity-40 dark:border-gray-700 dark:hover:bg-gray-800";
export const primaryButton =
  "rounded-md bg-gray-900 px-3 py-1.5 text-xs font-medium text-white hover:bg-gray-700 disabled:opacity-40 dark:bg-gray-100 dark:text-gray-900 dark:hover:bg-gray-300";
export const dangerButton =
  "rounded-md border border-red-300 px-3 py-1.5 text-xs text-red-700 hover:bg-red-50 disabled:opacity-40 dark:border-red-900 dark:text-red-300 dark:hover:bg-red-950/40";
export const linkButton =
  "text-xs underline decoration-gray-400 underline-offset-2 hover:decoration-current disabled:opacity-40";
export const input =
  "w-full rounded-md border border-gray-300 bg-transparent px-3 py-2 text-sm dark:border-gray-700";
export const card =
  "rounded-xl border border-gray-200 bg-white p-5 dark:border-gray-800 dark:bg-gray-950";
export const panel =
  "space-y-3 rounded-lg border border-gray-200 p-4 dark:border-gray-800";
export const muted = "text-xs text-gray-500 dark:text-gray-400";
export const notice =
  "rounded-md border border-amber-300 bg-amber-50 p-3 text-xs text-amber-900 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-200";
export const errorNotice =
  "rounded-md border border-red-300 bg-red-50 p-3 text-xs text-red-800 dark:border-red-900 dark:bg-red-950/30 dark:text-red-300";
