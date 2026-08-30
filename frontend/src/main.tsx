import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";

const isTauriRuntime = "__TAURI_INTERNALS__" in window;
document.documentElement.dataset.runtime = isTauriRuntime ? "tauri" : "web";

ReactDOM.createRoot(document.getElementById("root")!).render(<React.StrictMode><App /></React.StrictMode>);
