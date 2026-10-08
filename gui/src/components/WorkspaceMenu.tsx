import { useRef, useState, type ReactNode, type RefObject } from "react";
import IconButton from "../ui/IconButton";
import { Menu } from "../ui/Menu";
import { Icon } from "../ui/icons";

/** A "more" button with its action menu. Children are `MenuItem`s. */
export default function WorkspaceMenu({
  label,
  children,
  triggerRef,
  disabled = false,
}: {
  label: string;
  children: ReactNode;
  triggerRef?: RefObject<HTMLButtonElement | null>;
  disabled?: boolean;
}) {
  const ownTrigger = useRef<HTMLButtonElement>(null);
  const trigger = triggerRef ?? ownTrigger;
  const [open, setOpen] = useState(false);
  return (
    <>
      <IconButton
        ref={trigger}
        label={label}
        tooltipSide="bottom"
        aria-haspopup="menu"
        aria-expanded={open}
        disabled={disabled}
        onClick={() => setOpen((value) => !value)}
      >
        <Icon name="more" />
      </IconButton>
      <Menu
        open={open}
        onClose={() => setOpen(false)}
        anchorRef={trigger}
        label={label}
      >
        {children}
      </Menu>
    </>
  );
}
