import React from "react";
import ReactDOM from "react-dom/client";
import { WebuiApp } from "@/app/WebuiApp";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <WebuiApp />
  </React.StrictMode>,
);
