import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { EngineStatus } from "../../lib/types";
import { NO_PARSER_CONTROLS, parserControls } from "./search";

/**
 * Which machine-dependent parser controls exist, read while the researcher
 * searches so that a result never names a control this machine does not have.
 * Until the answer arrives, and if it fails, those results stay out of the list.
 */
export function useParserControls(searching: boolean) {
  const [controls, setControls] = useState(NO_PARSER_CONTROLS);
  useEffect(() => {
    if (!searching) return;
    let current = true;
    void invoke<EngineStatus[]>("list_engines").then(
      (engines) => {
        if (current) setControls(parserControls(engines));
      },
      () => {
        if (current) setControls(NO_PARSER_CONTROLS);
      },
    );
    return () => {
      current = false;
    };
  }, [searching]);
  return controls;
}
