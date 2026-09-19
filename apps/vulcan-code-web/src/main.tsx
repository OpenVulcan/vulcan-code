import React from "react";
import ReactDOM from "react-dom/client";
import { WebuiApp } from "@/app/WebuiApp";
import { applyShellHostMarker } from "@/shell/runtime";
import "./boot-styles.css";

// The host marker must be published before the first paint so native drag regions are correct.
// 宿主标记必须在首帧之前发布，使原生拖动区域从一开始就正确。
applyShellHostMarker();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <WebuiApp />
  </React.StrictMode>,
);
