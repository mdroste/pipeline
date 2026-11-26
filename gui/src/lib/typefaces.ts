/** Device-only font choices; the Source pair reserves serif text for reports. */
export const TYPEFACE_OPTIONS = [
  {
    value: "inter",
    label: "Inter",
    description: "Default interface and report typeface.",
  },
  {
    value: "instrument-sans",
    label: "Instrument Sans",
    description: "A contemporary sans-serif for the interface and reports.",
  },
  {
    value: "ibm-plex-sans",
    label: "IBM Plex Sans",
    description: "A technical sans-serif for the interface and reports.",
  },
  {
    value: "source",
    label: "Source Sans 3 + Source Serif 4",
    description: "Source Sans 3 for the interface; Source Serif 4 for reports.",
  },
] as const;

export type Typeface = (typeof TYPEFACE_OPTIONS)[number]["value"];

export function typefaceOption(value: unknown) {
  return (
    TYPEFACE_OPTIONS.find((option) => option.value === value) ??
    TYPEFACE_OPTIONS[0]
  );
}
