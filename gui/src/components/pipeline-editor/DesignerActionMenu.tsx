import { useRef, useState, type ReactNode } from "react";
import { IconButton, Menu } from "../../ui";
import { Icon } from "../../ui/icons";

export default function DesignerActionMenu({
  label,
  disabled,
  children,
}: {
  label: string;
  disabled?: boolean;
  children: ReactNode;
}) {
  const anchor = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  return (
    <>
      <IconButton
        ref={anchor}
        label={label}
        size="sm"
        aria-haspopup="menu"
        aria-expanded={open}
        disabled={disabled}
        onClick={() => setOpen(!open)}
      >
        <Icon name="more" className="h-4 w-4" />
      </IconButton>
      <Menu
        open={open}
        onClose={() => setOpen(false)}
        anchorRef={anchor}
        label={label}
      >
        {children}
      </Menu>
    </>
  );
}
