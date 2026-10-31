import ToolSetup from "./ToolSetup";
export default function ShellSetup({ disabled }: { disabled: boolean }) {
  return <ToolSetup kind="shell" disabled={disabled} />;
}
