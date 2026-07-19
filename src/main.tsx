import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { error } from "@tauri-apps/plugin-log";
import "./index.css";
import App from "./App.tsx";
import ErrorBoundary from "./components/ErrorBoundary";
import { installGlobalDiagnostics } from "./diagnostics";

document.addEventListener("contextmenu", (e) => e.preventDefault());

window.addEventListener(
  "keydown",
  (event) => {
    if (event.key === "F7") {
      event.preventDefault();
    }
  },
  { capture: true },
);

installGlobalDiagnostics();

createRoot(document.getElementById("root")!, {
  onCaughtError: (err, errorInfo) => {
    error(
      JSON.stringify({
        source: "createRoot.onCaughtError",
        error: err instanceof Error ? err.message : String(err),
        componentStack: errorInfo.componentStack,
        errorBoundary: errorInfo.errorBoundary?.constructor?.name,
      }),
    );
  },
}).render(
  <StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </StrictMode>,
);
