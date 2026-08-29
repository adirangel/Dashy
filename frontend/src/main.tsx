import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";
import { positionDashyWindow } from "./windowPosition";

const isTauriRuntime = "__TAURI_INTERNALS__" in window;
document.documentElement.dataset.runtime = isTauriRuntime ? "tauri" : "web";

if (isTauriRuntime) {
  void positionDashyWindow().catch((error: unknown) => {
    console.warn("Dashy could not move to the top-right corner.", error);
  });
}

ReactDOM.createRoot(document.getElementById("root")!).render(<React.StrictMode><App /></React.StrictMode>);
