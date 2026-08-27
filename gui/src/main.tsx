import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import ErrorBoundary from "./components/ErrorBoundary";
import DialogService from "./components/DialogService";
import "katex/dist/katex.min.css";
import "./App.css";

async function renderApp() {
  if (import.meta.env.MODE === "e2e") {
    await import("@wdio/tauri-plugin");
  }

  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <ErrorBoundary>
        <DialogService>
          <App />
        </DialogService>
      </ErrorBoundary>
    </React.StrictMode>,
  );
}

void renderApp();
