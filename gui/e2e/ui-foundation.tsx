/** Development specimen for shared chrome and native top-layer checks. No IPC. */
import { useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import Button from "../src/ui/Button";
import Badge from "../src/ui/Badge";
import PageHeader from "../src/ui/PageHeader";
import SegmentedControl from "../src/ui/SegmentedControl";
import Select from "../src/ui/Select";
import Sheet from "../src/ui/Sheet";
import { Menu, MenuItem } from "../src/ui/Menu";
import InfoButton from "../src/components/InfoButton";
import DecimalField from "../src/components/DecimalField";
import { NumberField } from "../src/components/tasks/NumberField";
import { Toggle } from "../src/components/settings/controls";
import "../src/App.css";

const params = new URLSearchParams(location.search);
document.documentElement.classList.toggle(
  "dark",
  params.get("theme") === "dark",
);
document.documentElement.style.fontSize = `${(16 * Number(params.get("scale") ?? 100)) / 100}px`;
document.documentElement.dataset.density =
  params.get("density") ?? "comfortable";

function Specimen() {
  const [mode, setMode] = useState("first");
  const [checked, setChecked] = useState(false);
  const [amount, setAmount] = useState(-1.5);
  const [count, setCount] = useState(3);
  const [menu, setMenu] = useState(false);
  const [sheet, setSheet] = useState(false);
  const [action, setAction] = useState("");
  const trigger = useRef<HTMLButtonElement>(null);
  const native = useRef<HTMLDialogElement>(null);
  return (
    <main
      className="ui-page font-sans"
      style={{ minHeight: "100vh", background: "var(--ui-surface)" }}
    >
      <PageHeader
        title="UI foundation"
        description="Buttons, fields, status, and layers."
      />
      <div className="flex flex-wrap gap-3">
        <Button variant="primary">Primary</Button>
        <Button>Secondary</Button>
        <Button variant="danger">Remove</Button>
        <Button variant="link">Details</Button>
        <Button disabled>Unavailable</Button>
        <Button size="sm">Small action</Button>
        <InfoButton label="Fixture">
          Help stays inside the window and returns focus on Escape.
        </InfoButton>
      </div>
      <section className="mt-6 flex flex-wrap gap-4">
        <label className="ui-field-label">
          Text
          <input className="ui-input block mt-2" placeholder="Enter a value" />
        </label>
        <label className="ui-field-label">
          Decimal
          <DecimalField
            aria-label="Decimal"
            className="block mt-2"
            value={amount}
            onChange={setAmount}
          />
        </label>
        <label className="ui-field-label">
          Whole number
          <NumberField
            aria-label="Whole number"
            className="block mt-2"
            value={count}
            min={1}
            max={10}
            onChange={setCount}
          />
        </label>
        <label className="ui-field-label">
          Native choice
          <select aria-label="Native choice" className="block mt-2">
            <option>First choice</option>
            <option>Second choice</option>
          </select>
        </label>
        <label className="ui-field-label">
          Invalid value
          <input
            className="ui-input block mt-2"
            aria-invalid="true"
            aria-describedby="invalid-help"
            defaultValue="Incomplete"
          />
          <span className="ui-field-error" id="invalid-help">
            Enter a complete value.
          </span>
        </label>
      </section>
      <section className="mt-6 flex flex-wrap items-center gap-4">
        <SegmentedControl
          label="Mode"
          value={mode}
          onChange={setMode}
          options={[
            { value: "first", label: "First" },
            { value: "second", label: "Second" },
          ]}
        />
        <Select
          label="Custom choice"
          value={mode}
          onChange={setMode}
          options={[
            { value: "first", label: "First" },
            { value: "second", label: "Second" },
          ]}
        />
        <Toggle
          label="Enable option"
          description="A labeled toggle."
          checked={checked}
          onChange={setChecked}
        />
      </section>
      <section className="mt-6 flex flex-wrap gap-4">
        <Badge kind="running" label="Running" showLabel />
        <Badge kind="attention" label="Needs attention" showLabel />
        <Badge kind="ok" label="Complete" showLabel />
        <Badge kind="error" label="Failed" showLabel />
        <Badge kind="idle" label="Unknown" showLabel />
      </section>
      <div className="ui-notice ui-notice--warning mt-6">
        An action needs attention.
      </div>
      <div className="ui-notice ui-notice--error mt-3">
        The action could not finish.{" "}
        <Button variant="link" onClick={() => setAction("retry")}>
          Retry
        </Button>
      </div>
      <section className="mt-6 flex flex-wrap gap-3">
        <Button
          ref={trigger}
          aria-haspopup="menu"
          aria-expanded={menu}
          onClick={() => setMenu(!menu)}
        >
          Open menu
        </Button>
        <Menu
          open={menu}
          onClose={() => setMenu(false)}
          anchorRef={trigger}
          label="Fixture actions"
        >
          <MenuItem onSelect={() => setAction("open")}>Open</MenuItem>
          <MenuItem disabled>Unavailable</MenuItem>
          <MenuItem onSelect={() => setAction("remove")} danger>
            Remove
          </MenuItem>
        </Menu>
        <Button onClick={() => native.current?.showModal()}>
          Open native dialog
        </Button>
        <Button onClick={() => setSheet(true)}>Open pane sheet</Button>
        <output aria-label="Last action">{action}</output>
      </section>
      <div
        className="relative mt-6 border border-line"
        style={{ minHeight: 180 }}
      >
        <p className="p-4">A pane-scoped sheet keeps other panes available.</p>
        <Sheet
          open={sheet}
          onClose={() => setSheet(false)}
          title="Pane details"
        >
          <p className="p-4">Sheet content.</p>
        </Sheet>
      </div>
      <dialog
        ref={native}
        className="ui-overlay-frame m-auto p-4"
        aria-label="Native specimen"
      >
        <h2 className="text-ui-title font-semibold">Native dialog</h2>
        <div className="flex items-center gap-3 mt-3">
          <InfoButton label="Modal field">
            This help belongs to the native dialog.
          </InfoButton>
          <span>Help opens in the native top layer.</span>
        </div>
        <div className="ui-overlay-footer">
          <Button onClick={() => native.current?.close()}>Close dialog</Button>
        </div>
      </dialog>
    </main>
  );
}
createRoot(document.getElementById("root")!).render(<Specimen />);
