import React from "react";
import { createRoot } from "react-dom/client";
import { WebuiApp } from "../../src/app/WebuiApp";
import { calls } from "./runtime";
import { AuditGitSurface } from "./git-surface";
import { AuditMediaSurface } from "./media-surface";
import { AuditStatsSurface } from "./stats-surface";
import { AuditDecorativeSurface } from "./decorative-surface";
import { AuditStructureSurface } from "./structure-surface";
import { AuditComposerSurface } from "./composer-surface";
import { AuditSharedSurface } from "./shared-surface";
import { AuditSelectionSurface } from "./selection-surface";
import { AuditOverlaySurface } from "./overlay-surface";
import { AuditPickerSurface } from "./picker-surface";
import { AuditMenuSurface } from "./menu-surface";
import { AuditModelSurface } from "./model-surface";
import { AuditStartupSurface } from "./startup-surface";
import { AuditMemoryControlsSurface } from "./memory-controls-surface";
import { AuditMemoryPolicySurface } from "./memory-policy-surface";
import { AuditMemoryLifecycleSurface } from "./memory-lifecycle-surface";
import { AuditMemoryMaintenanceSurface } from "./memory-maintenance-surface";
import "../../src/boot-styles.css";
import "./app.css";

// theme initializes the existing production theme contract within this isolated origin.
// theme 在此隔离源中初始化既有生产主题契约。
const theme = new URLSearchParams(window.location.search).get("theme");
if (theme === "light" || theme === "dark") localStorage.setItem("vulcan-webui-theme", theme);
// publish forwards bounded transport metadata to the optional outer toolbar only.
// publish 仅将有界传输元数据转发给可选的外层工具栏。
function publish() {
  if (window.parent !== window) window.parent.postMessage({ type: "ui-audit:rpc", calls }, window.location.origin);
}
window.addEventListener("ui-audit:rpc", publish);
// root mounts the unchanged production boot gate in its own real viewport.
// root 在自身真实视口中挂载未改动的生产启动门。
const root = document.getElementById("root");
if (!root) throw new Error("UI_AUDIT_APP_ROOT_MISSING");
// surface selects an isolated production component only when explicitly requested by the audit URL.
// surface 仅在验收网址明确请求时选择隔离生产组件。
const surface = new URLSearchParams(window.location.search).get("surface");
if (surface === "memory-maintenance") document.documentElement.classList.toggle("dark", theme === "dark");
if (surface === "picker" || surface === "menu" || surface === "model" || surface === "startup" || surface === "memory-controls" || surface === "memory-policy" || surface === "memory-lifecycle") document.documentElement.classList.toggle("dark", theme === "dark");
if (surface === "git" || surface === "media" || surface === "stats" || surface === "decorative" || surface === "structure" || surface === "composer" || surface === "shared" || surface === "selection" || surface === "overlay") document.documentElement.classList.toggle("dark", theme === "dark");
createRoot(root).render(surface === "memory-maintenance" ? <AuditMemoryMaintenanceSurface /> : surface === "git" ? <AuditGitSurface /> : surface === "media" ? <AuditMediaSurface /> : surface === "stats" ? <AuditStatsSurface /> : surface === "decorative" ? <AuditDecorativeSurface /> : surface === "structure" ? <AuditStructureSurface /> : surface === "composer" ? <AuditComposerSurface /> : surface === "shared" ? <AuditSharedSurface /> : surface === "selection" ? <AuditSelectionSurface /> : surface === "overlay" ? <AuditOverlaySurface /> : surface === "picker" ? <AuditPickerSurface /> : surface === "menu" ? <AuditMenuSurface /> : surface === "model" ? <AuditModelSurface /> : surface === "startup" ? <AuditStartupSurface /> : surface === "memory-controls" ? <AuditMemoryControlsSurface /> : surface === "memory-policy" ? <AuditMemoryPolicySurface /> : surface === "memory-lifecycle" ? <AuditMemoryLifecycleSurface /> : <WebuiApp />);
