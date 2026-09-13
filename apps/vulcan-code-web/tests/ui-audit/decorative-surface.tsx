import React from "react";
import { I18nProvider } from "../../src/i18n";
import { ProviderIcon } from "../../src/components/features/settings/provider-icon";
import { ProjectIcon } from "../../src/components/project/project-icons";
import { ConversationLoadingCenter, ConversationSubmissionStatus, EmptyConversationPrompt } from "../../src/components/conversation/conversation-chrome";
import type { ProjectSummary } from "../../src/types";

// providerIds is the explicit provider artwork map from the production component.
// providerIds 是生产组件中明确的供应商图标映射。
const providerIds = ["alibaba", "apikeyfun", "anthropic", "agentspace", "claude", "claudeapi", "code0", "commandcode", "deepseek", "fennoai", "google", "kimi", "minimax", "openai", "opencode-go", "openrouter", "qwen", "qiniu", "tavily", "tokenrhythm", "vertex", "xai", "zhipu", "custom:audit"];
// projects are in-memory records with known badge and catalog formats; their paths are never opened.
// projects 是使用已知徽标与目录格式的内存记录；绝不打开其中路径。
const projects: ProjectSummary[] = [
  { id: "default", name: "默认文件夹", path: "D:/audit" },
  { id: "lucide", name: "延迟 Lucide 图标", path: "D:/audit", iconPack: "lucide", iconKey: "FolderPlus" },
  { id: "missing", name: "不存在的图标键", path: "D:/audit", iconPack: "lucide", iconKey: "AUDIT_MISSING_ICON" },
  { id: "badge", name: "自定义徽标", path: "D:/audit", iconPack: "badge", iconKey: JSON.stringify({ shape: "circle", text: "测", background: "#2563eb", border: "#111827", foreground: "#ffffff" }) },
  { id: "broken", name: "无效徽标", path: "D:/audit", iconPack: "badge", iconKey: "invalid-json" },
];

/** Mounts production decorative components or one declared conversation placeholder state.
 * 挂载生产装饰组件，或一个明确的会话占位状态。
 * No props; returns isolated local visuals with no RPC, permissions, or stored changes.
 * 无属性；返回隔离本地视觉界面，不调用 RPC、权限或持久化变更。
 */
export function AuditDecorativeSurface() {
  // query selects a supported display state without mutating the actual workbench.
  // query 选择受支持的展示状态，不修改真实工作台。
  const query = new URLSearchParams(window.location.search);
  // locale keeps the production language contract explicit.
  // locale 明确遵循生产语言契约。
  const locale = query.get("locale") === "en-US" ? "en-US" : "zh-CN";
  // chrome names a closed placeholder state, or the default decorative gallery.
  // chrome 指定封闭的占位状态，未指定时展示装饰图集。
  const chrome = query.get("chrome") ?? "icons";
  if (!["icons", "empty", "leaving", "loading", "creating", "preparing"].includes(chrome)) throw new Error(`Unknown decorative audit state: ${chrome}`);
  return <I18nProvider initialLocale={locale}>
    {chrome === "icons" ? <main style={{ padding: 16 }}>
      <p>合成装饰组件验收 · 无模型调用与配置写入</p>
      <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(140px, 1fr))", gap: 16 }}>
        {providerIds.map((providerId) => <div key={providerId} style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <ProviderIcon providerId={providerId} category={providerId === "commandcode" ? "relay" : undefined} />{providerId}
        </div>)}
        <div><ProviderIcon providerId="google" serviceEntryId="antigravity" />Antigravity</div>
        <div><ProviderIcon providerId="google" serviceEntryId="vertex" />Vertex</div>
        <div><ProviderIcon providerId="openai" serviceEntryId="codex-subscription" />Codex</div>
      </div>
      <div style={{ display: "flex", gap: 16, flexWrap: "wrap", marginTop: 32 }}>
        {projects.map((project) => <div key={project.id} style={{ display: "grid", gap: 8 }}>
          <span>{project.name}</span><ProjectIcon project={project} size={32} /><ProjectIcon project={project} isOpen size={32} />
        </div>)}
      </div>
    </main> : <main style={{ position: "relative", height: "100dvh", minWidth: 0 }}>
      {chrome === "empty" || chrome === "leaving" ? <EmptyConversationPrompt leaving={chrome === "leaving"} projectName={"LongProjectName".repeat(12)} /> : null}
      {chrome === "loading" ? <ConversationLoadingCenter /> : null}
      {chrome === "creating" || chrome === "preparing" ? <ConversationSubmissionStatus stage={chrome} /> : null}
    </main>}
  </I18nProvider>;
}
