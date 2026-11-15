import { useId, useRef, useState } from "react";
import IconButton from "../ui/IconButton";
import Popover from "../ui/Popover";

interface Props {
  label: string;
  children: React.ReactNode;
}

export default function InfoButton({ label, children }: Props) {
  const [open, setOpen] = useState(false);
  const dialogId = useId();
  const buttonRef = useRef<HTMLButtonElement>(null);

  return (
    <span
      onClick={(event) => event.stopPropagation()}
      className="relative inline-flex shrink-0 align-middle"
    >
      <IconButton
        ref={buttonRef}
        label={`More information about ${label}`}
        tooltip={null}
        size="sm"
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-controls={dialogId}
        onClick={() => setOpen((value) => !value)}
      >
        <svg
          aria-hidden="true"
          viewBox="0 0 20 20"
          fill="currentColor"
          className="h-3.5 w-3.5"
        >
          <path
            fillRule="evenodd"
            d="M10 18a8 8 0 100-16 8 8 0 000 16zm.75-10.75a.75.75 0 00-1.5 0v.1a.75.75 0 001.5 0v-.1zm0 2.75a.75.75 0 00-1.5 0v3a.75.75 0 001.5 0v-3z"
            clipRule="evenodd"
          />
        </svg>
      </IconButton>
      <Popover
        id={dialogId}
        open={open}
        onClose={() => setOpen(false)}
        anchorRef={buttonRef}
        label={`${label} help`}
        width={288}
        className="px-3 py-2.5 text-left font-normal normal-case leading-relaxed tracking-normal"
      >
        {children}
      </Popover>
    </span>
  );
}
