import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import { HudPanel } from "./hud/HudPanel";
import { Snipper } from "./hud/Snipper";
import "./index.css";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

export const RootComponent: React.FC = () => {
  const [windowLabel, setWindowLabel] = useState<string>("main");

  useEffect(() => {
    try {
      const current = getCurrentWebviewWindow();
      if (current && current.label) {
        setWindowLabel(current.label);
      }
    } catch {
      // Fallback for browser dev mode
      const params = new URLSearchParams(window.location.search);
      const label = params.get("window") || "main";
      setWindowLabel(label);
    }
  }, []);

  if (windowLabel === "hud") {
    return <HudPanel />;
  }

  if (windowLabel === "overlay") {
    return <Snipper />;
  }

  return <App />;
};

const rootEl = typeof document !== "undefined" ? document.getElementById("root") : null;
if (rootEl) {
  ReactDOM.createRoot(rootEl).render(
    <React.StrictMode>
      <RootComponent />
    </React.StrictMode>
  );
}
