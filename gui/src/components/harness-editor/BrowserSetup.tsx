import ToolSetup from "./ToolSetup";
export default function BrowserSetup({ disabled }: { disabled: boolean }) {
  return <ToolSetup kind="browser" disabled={disabled} />;
}
