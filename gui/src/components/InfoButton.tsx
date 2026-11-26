import { Icon } from "../ui/icons";
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
        <Icon name="info" className="h-3.5 w-3.5" />
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
