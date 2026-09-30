// Development-only full-app fixture. See app-full-shim.ts for the fake IPC.
import "./app-full-shim";
import React from "react";
import { createRoot } from "react-dom/client";
import App from "../src/App";
import ErrorBoundary from "../src/components/ErrorBoundary";
import DialogService from "../src/components/DialogService";
import "katex/dist/katex.min.css";
import "../src/App.css";

createRoot(document.getElementById("root")!).render(
  <ErrorBoundary>
    <DialogService>
      <App />
    </DialogService>
  </ErrorBoundary>,
);
