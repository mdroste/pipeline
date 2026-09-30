// The app's one icon set (18px stroke icons). NavRail, Home, and new
// surfaces draw from here; do not redraw these paths locally.

export type IconName =
  | "home"
  | "new"
  | "assistant"
  | "review"
  | "tasks"
  | "activity"
  | "history"
  | "project"
  | "designer"
  | "help"
  | "settings"
  | "arrow";

export function Icon({
  name,
  className = "h-[18px] w-[18px]",
}: {
  name: IconName;
  className?: string;
}) {
  const common = {
    className,
    fill: "none",
    viewBox: "0 0 24 24",
    stroke: "currentColor",
    strokeWidth: 1.65,
    "aria-hidden": true as const,
  };

  switch (name) {
    case "home":
      return (
        <svg {...common}>
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="m4 10 8-6.5 8 6.5v8.25A1.75 1.75 0 0 1 18.25 20H5.75A1.75 1.75 0 0 1 4 18.25V10Z"
          />
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M9.25 20v-6.25h5.5V20"
          />
        </svg>
      );
    case "new":
      return (
        <svg {...common}>
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M12 5v14M5 12h14"
          />
        </svg>
      );
    case "assistant":
      return (
        <svg {...common}>
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M5 18.5 3.5 21V5A2 2 0 0 1 5.5 3h13a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H7l-2 1.5Z"
          />
          <path strokeLinecap="round" d="M7.5 8h9M7.5 12h6" />
        </svg>
      );
    case "review":
      return (
        <svg {...common}>
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M14 3.5H6A1.5 1.5 0 0 0 4.5 5v14A1.5 1.5 0 0 0 6 20.5h12a1.5 1.5 0 0 0 1.5-1.5V9L14 3.5ZM14 3.5V9h5.5M8 14l2.5 2.5L16 11"
          />
        </svg>
      );
    case "tasks":
      return (
        <svg {...common}>
          <rect x="4" y="4" width="16" height="16" rx="3" />
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="m8 12 2.5 2.5L16 9"
          />
        </svg>
      );
    case "activity":
      return (
        <svg {...common}>
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M3 12h4l3-7 4 14 3-7h4"
          />
        </svg>
      );
    case "history":
      return (
        <svg {...common}>
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M3.5 4.5v5h5M3.8 9a8.25 8.25 0 1 1 .3 6M12 7.5V12l3 2"
          />
        </svg>
      );
    case "project":
      return (
        <svg {...common}>
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M3.75 7.25h6l1.5 2h9v8.5a2 2 0 0 1-2 2H5.75a2 2 0 0 1-2-2V7.25Z"
          />
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M5.75 7.25V5.5a1.5 1.5 0 0 1 1.5-1.5h4.25l1.5 2h4.75a1.5 1.5 0 0 1 1.5 1.5v1.75"
          />
        </svg>
      );
    case "designer":
      return (
        <svg {...common}>
          <circle cx="6" cy="6" r="2.25" />
          <circle cx="18" cy="12" r="2.25" />
          <circle cx="6" cy="18" r="2.25" />
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M8.25 6h2.25A3.5 3.5 0 0 1 14 9.5v0A2.5 2.5 0 0 0 16.5 12M8.25 18h2.25A3.5 3.5 0 0 0 14 14.5v0A2.5 2.5 0 0 1 16.5 12"
          />
        </svg>
      );
    case "help":
      return (
        <svg {...common}>
          <circle cx="12" cy="12" r="8.25" />
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M9.85 9.35a2.3 2.3 0 1 1 3.32 2.07c-.72.4-1.17.83-1.17 1.58v.25M12 16.75h.01"
          />
        </svg>
      );
    case "settings":
      return (
        <svg {...common}>
          <circle cx="12" cy="12" r="2.75" />
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M19 13.25v-2.5l-2.05-.52a5.7 5.7 0 0 0-.55-1.32l1.08-1.82-1.77-1.77-1.82 1.08a5.7 5.7 0 0 0-1.32-.55L12.05 3h-2.5l-.52 2.05a5.7 5.7 0 0 0-1.32.55L5.9 4.52 4.12 6.29 5.2 8.11a5.7 5.7 0 0 0-.55 1.32L2.6 9.95v2.5l2.05.52c.13.46.31.9.55 1.32l-1.08 1.82 1.77 1.77 1.82-1.08c.42.24.86.42 1.32.55l.52 2.05h2.5l.52-2.05c.46-.13.9-.31 1.32-.55l1.82 1.08 1.77-1.77-1.08-1.82c.24-.42.42-.86.55-1.32L19 13.25Z"
          />
        </svg>
      );
    case "arrow":
      return (
        <svg {...common}>
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M5 12h14m-5-5 5 5-5 5"
          />
        </svg>
      );
  }
}
