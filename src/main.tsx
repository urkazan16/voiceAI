import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import { Bar } from "./Bar";
import "./styles.css";

const isBar = new URLSearchParams(window.location.search).get("window") === "bar";
if (isBar) {
  document.documentElement.classList.add("flow-bar");
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{isBar ? <Bar /> : <App />}</React.StrictMode>,
);
