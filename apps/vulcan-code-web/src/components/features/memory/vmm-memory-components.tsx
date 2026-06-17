import * as React from "react"
import { FileText, History, Trash2 } from "lucide-react"

import { StatusPill } from "@/components/shared/status"
import type { MemorySummary, VmmConfigDraftSummary, VmmMemoryActionResult, VmmMemorySnapshot } from "@/types"

// VmmEffectiveOverview renders compact effective-config cards.
// VmmEffectiveOverview 渲染紧凑的最终配置卡片。
export function VmmEffectiveOverview(props: {
  // Legacy summary entries.
  // 旧摘要条目。
  memories: MemorySummary[]
  // Detailed VMM snapshot.
  // 详细 VMM 快照。
  snapshot: VmmMemorySnapshot
}) {
  const effective = props.snapshot.effective;
  return (
    <div className="vmm-overview-grid">
      <article className="vmm-overview-card">
        <StatusPill status={effective.diagnostic === "ready" ? "ready" : "warning"} label={vmmDiagnosticLabel(effective.diagnostic)} />
        <strong>有效配置</strong>
        <span>gRPC：{effective.grpcTarget || "未配置"}</span>
        <span>user/project：{effective.userId ?? "-"} / {effective.projectId ?? "-"}</span>
      </article>
      <article className="vmm-overview-card">
        <StatusPill status={effective.precheckDiagnostic === "ready" ? "ready" : "warning"} label={vmmDiagnosticLabel(effective.precheckDiagnostic)} />
        <strong>召回策略</strong>
        <span>TTL：{effective.implicitMemoryTurns} 回合</span>
        <span>前置召回：{effective.precheckEnabled ? "启用" : "关闭"} · AI 工具：{effective.aiToolsEnabled ? "启用" : "关闭"}</span>
      </article>
      <article className="vmm-overview-card">
        <StatusPill status="ready" label="runtime" />
        <strong>运行态</strong>
        <span>outbox：{props.snapshot.runtime.outboxCount} · 画像节点：{props.snapshot.runtime.profileSyncCount}</span>
        <span>已完成回合：{props.snapshot.runtime.completedTurns}</span>
      </article>
      <article className="vmm-overview-card">
        <StatusPill status="warning" label="summary" />
        <strong>摘要入口</strong>
        <span>{props.memories.map((memory) => `${memory.title} ${memory.state}`).join("；") || "暂无摘要"}</span>
      </article>
    </div>
  );
}

// VmmRuntimePanel renders selected-session runtime details.
// VmmRuntimePanel 渲染当前选中会话运行态详情。
export function VmmRuntimePanel(props: {
  // Detailed VMM snapshot.
  // 详细 VMM 快照。
  snapshot: VmmMemorySnapshot
}) {
  return (
    <section className="vmm-side-card">
      <VmmPanelHeading icon={<History size={15} />} title="运行态" subtitle="当前会话的 VMM hook 与画像缓存。" />
      <dl className="vmm-stat-list">
        <div><dt>会话</dt><dd>{props.snapshot.selectedConversationId ?? "未选择"}</dd></div>
        <div><dt>outbox</dt><dd>{props.snapshot.runtime.outboxCount}</dd></div>
        <div><dt>画像节点</dt><dd>{props.snapshot.runtime.profileSyncCount}</dd></div>
        <div><dt>更新时间</dt><dd>{props.snapshot.runtime.updatedAt ?? "暂无"}</dd></div>
      </dl>
      <pre className="vmm-preview-text">{props.snapshot.runtime.profilePreview || "暂无画像基点。"}</pre>
    </section>
  );
}

// VmmResultPanel renders the latest VMM action JSON previews.
// VmmResultPanel 渲染最近一次 VMM 动作 JSON 预览。
export function VmmResultPanel(props: {
  // Latest action result.
  // 最近动作结果。
  result: VmmMemoryActionResult | null
}) {
  return (
    <section className="vmm-side-card">
      <VmmPanelHeading icon={<FileJson2Icon />} title="动作结果" subtitle={props.result?.summary ?? "尚未执行 VMM 动作。"} />
      {props.result ? (
        <>
          <strong className="vmm-result-kind">{props.result.kind}</strong>
          <VmmJsonPreview title="请求" value={props.result.request} />
          <VmmJsonPreview title="响应" value={props.result.response} />
        </>
      ) : (
        <span className="vmm-muted">点击左侧动作后会显示请求与响应。</span>
      )}
    </section>
  );
}

// FileJson2Icon renders the file-json icon without widening imports further.
// FileJson2Icon 渲染 file-json 图标并避免继续扩大导入列表。
function FileJson2Icon() {
  return <FileText size={15} />;
}

// VmmPanelHeading renders a small VMM panel heading.
// VmmPanelHeading 渲染小型 VMM 面板标题。
export function VmmPanelHeading(props: {
  // Heading icon.
  // 标题图标。
  icon: React.ReactNode
  // Heading title.
  // 标题文本。
  title: string
  // Heading subtitle.
  // 副标题文本。
  subtitle: string
}) {
  return (
    <header className="vmm-panel-heading">
      {props.icon}
      <div>
        <strong>{props.title}</strong>
        <span>{props.subtitle}</span>
      </div>
    </header>
  );
}

// VmmTextField renders one compact VMM text field.
// VmmTextField 渲染单个紧凑 VMM 文本字段。
export function VmmTextField(props: {
  // Visible field label.
  // 可见字段标签。
  label: string
  // Placeholder shown when the value is empty.
  // 值为空时显示的占位文本。
  placeholder: string
  // Current field value.
  // 当前字段值。
  value: string
  // Updates the field value.
  // 更新字段值。
  onChange: (value: string) => void
}) {
  return (
    <label className="vmm-field">
      <span>{props.label}</span>
      <input placeholder={props.placeholder} value={props.value} onChange={(event) => props.onChange(event.target.value)} />
    </label>
  );
}

// VmmActionButton renders one VMM action button.
// VmmActionButton 渲染单个 VMM 动作按钮。
export function VmmActionButton(props: {
  // Whether the action is currently running.
  // 动作是否正在运行。
  busy: boolean
  // Whether the action is destructive.
  // 动作是否具有破坏性。
  danger?: boolean
  // Button icon.
  // 按钮图标。
  icon: React.ReactNode
  // Button label shown when idle.
  // 空闲时显示的按钮文本。
  label: string
  // Runs the action.
  // 执行动作。
  onClick: () => void
}) {
  return (
    <button className={`vmm-icon-button${props.danger ? " danger" : ""}`} disabled={props.busy} onClick={props.onClick} type="button">
      {props.icon}{props.busy ? "执行中" : props.label}
    </button>
  );
}

// VmmSettingsForm renders editable VMM settings.
// VmmSettingsForm 渲染可编辑 VMM 设置。
export function VmmSettingsForm(props: {
  // Editable configuration draft.
  // 可编辑配置草稿。
  draft: VmmConfigDraftSummary
  // Form mode that selects memory or transport fields.
  // 决定展示记忆或传输字段的表单模式。
  mode: "memory" | "transport"
  // Applies one partial draft update.
  // 应用单次局部草稿更新。
  onChange: (patch: Partial<VmmConfigDraftSummary>) => void
}) {
  if (props.mode === "transport") {
    return (
      <div className="vmm-form-grid">
        <VmmTextField label="gRPC 地址" placeholder="127.0.0.1:50051" value={props.draft.grpcTarget} onChange={(grpcTarget) => props.onChange({ grpcTarget })} />
        <VmmTextField label="API Key" placeholder="可选" value={props.draft.grpcApiKey} onChange={(grpcApiKey) => props.onChange({ grpcApiKey })} />
        <VmmTextField label="请求超时" placeholder="30000" value={props.draft.receiveTimeoutMs} onChange={(receiveTimeoutMs) => props.onChange({ receiveTimeoutMs })} />
        <VmmTextField label="握手超时" placeholder="8000" value={props.draft.handshakeTimeoutMs} onChange={(handshakeTimeoutMs) => props.onChange({ handshakeTimeoutMs })} />
        <VmmTextField label="最大接收字节" placeholder="可选" value={props.draft.maxReceiveMessageBytes} onChange={(maxReceiveMessageBytes) => props.onChange({ maxReceiveMessageBytes })} />
        <VmmTextField label="keepalive 间隔" placeholder="30000" value={props.draft.grpcKeepaliveTimeMs} onChange={(grpcKeepaliveTimeMs) => props.onChange({ grpcKeepaliveTimeMs })} />
        <VmmTextField label="keepalive 超时" placeholder="10000" value={props.draft.grpcKeepaliveTimeoutMs} onChange={(grpcKeepaliveTimeoutMs) => props.onChange({ grpcKeepaliveTimeoutMs })} />
        <label className="vmm-check-row"><input checked={props.draft.grpcKeepalivePermitWithoutCalls} onChange={(event) => props.onChange({ grpcKeepalivePermitWithoutCalls: event.target.checked })} type="checkbox" />空闲 keepalive</label>
      </div>
    );
  }
  return (
    <div className="vmm-form-grid">
      <label className="vmm-check-row"><input checked={props.draft.enabled} onChange={(event) => props.onChange({ enabled: event.target.checked })} type="checkbox" />启用记忆连接</label>
      <label className="vmm-check-row"><input checked={props.draft.aiToolsEnabled} onChange={(event) => props.onChange({ aiToolsEnabled: event.target.checked })} type="checkbox" />暴露 AI 专属工具</label>
      <label className="vmm-check-row"><input checked={props.draft.precheckEnabled} onChange={(event) => props.onChange({ precheckEnabled: event.target.checked })} type="checkbox" />自动前置召回</label>
      <VmmTextField label="用户 ID" placeholder="数字" value={props.draft.userId} onChange={(userId) => props.onChange({ userId })} />
      <VmmTextField label="项目 ID" placeholder="数字" value={props.draft.projectId} onChange={(projectId) => props.onChange({ projectId })} />
      <VmmTextField label="临时记忆 TTL" placeholder="5" value={props.draft.implicitMemoryTurns} onChange={(implicitMemoryTurns) => props.onChange({ implicitMemoryTurns })} />
      <VmmTextField label="画像刷新间隔" placeholder="3" value={props.draft.profileRefreshTurns} onChange={(profileRefreshTurns) => props.onChange({ profileRefreshTurns })} />
      <VmmTextField label="画像节点上限" placeholder="24" value={props.draft.profileNodeLimit} onChange={(profileNodeLimit) => props.onChange({ profileNodeLimit })} />
      <VmmTextField label="注入标签" placeholder="vmm_inject" value={props.draft.injectionTag} onChange={(injectionTag) => props.onChange({ injectionTag })} />
      <VmmTextField label="注入说明" placeholder="VMM recalled memory" value={props.draft.injectionDescription} onChange={(injectionDescription) => props.onChange({ injectionDescription })} />
    </div>
  );
}

// VmmUserRows renders user rows from a VMM result.
// VmmUserRows 从 VMM 结果渲染用户行。
export function VmmUserRows(props: { result: VmmMemoryActionResult | null }) {
  const users = vmmResultArray(props.result, "users");
  if (users.length === 0) {
    return <p className="vmm-muted">执行“刷新用户/项目”后会显示用户列表。</p>;
  }
  return <div className="vmm-row-list">{users.map((user, index) => <VmmDataRow key={index} title={String(user.userName ?? user.user_name ?? "未命名用户")} detail={`ID：${String(user.userId ?? user.user_id ?? "-")}`} />)}</div>;
}

// VmmProjectRows renders project rows from a VMM result.
// VmmProjectRows 从 VMM 结果渲染项目行。
export function VmmProjectRows(props: { result: VmmMemoryActionResult | null }) {
  const projects = vmmResultArray(props.result, "projects");
  if (projects.length === 0) {
    return <p className="vmm-muted">执行“刷新用户/项目”后会显示项目列表。</p>;
  }
  return <div className="vmm-row-list">{projects.map((project, index) => <VmmDataRow key={index} title={String(project.displayPath ?? project.display_path ?? "未命名项目")} detail={`ID：${String(project.projectId ?? project.project_id ?? "-")}`} />)}</div>;
}

// VmmProfileRows renders active profile nodes from a VMM result.
// VmmProfileRows 从 VMM 结果渲染 active 画像节点。
export function VmmProfileRows(props: { result: VmmMemoryActionResult | null }) {
  const nodes = vmmResultArray(props.result, "nodes");
  if (nodes.length === 0) {
    return <p className="vmm-muted">刷新画像节点后会在这里显示 active profile nodes。</p>;
  }
  return <div className="vmm-row-list">{nodes.map((node, index) => <VmmDataRow key={index} title={`#${String(node.profileNodeId ?? node.profile_node_id ?? index + 1)}`} detail={String(node.content ?? "")} meta={`${String(node.priority ?? "-")} · ${String(node.level ?? "-")} · bind=${String(node.bindId ?? node.bind_id ?? "-")}`} />)}</div>;
}

// VmmBundlePreview renders profile bundle text from a VMM result.
// VmmBundlePreview 从 VMM 结果渲染画像 bundle 文本。
export function VmmBundlePreview(props: { result: VmmMemoryActionResult | null }) {
  const response = vmmObject(props.result?.response);
  const parts = [
    ["完整画像", response.combinedText ?? response.combined_text],
    ["团队画像", response.teamProfile ?? response.team_profile],
    ["空间画像", response.spaceProfile ?? response.space_profile],
    ["项目画像", response.projectProfile ?? response.project_profile],
    ["用户画像", response.userProfile ?? response.user_profile],
  ]
    .filter((entry): entry is [string, string] => typeof entry[1] === "string" && entry[1].trim().length > 0);
  if (parts.length === 0) {
    return <p className="vmm-muted">读取完整画像后会显示 bundle 内容。</p>;
  }
  return <div className="vmm-bundle-preview">{parts.map(([label, value]) => <section key={label}><strong>{label}</strong><pre>{String(value)}</pre></section>)}</div>;
}

// VmmToolResult renders compact tool-debug results.
// VmmToolResult 渲染紧凑的工具调试结果。
export function VmmToolResult(props: {
  // Latest action result.
  // 最近动作结果。
  result: VmmMemoryActionResult | null
  // Optional memory deletion callback used by search rows.
  // 检索行使用的可选记忆删除回调。
  onDeleteMemory?: (memoryId: number) => void
}) {
  if (!props.result) {
    return <p className="vmm-muted">工具调试结果会按动作显示。</p>;
  }
  if (props.result.kind === "search_memory_events") {
    const groups = vmmResultArray(props.result, "results");
    return (
      <div className="vmm-row-list">
        {groups.flatMap((group, groupIndex) => vmmArray(group.hits).map((hit, hitIndex) => {
          const memoryId = vmmNumber(hit.memoryId ?? hit.memory_id);
          return (
            <VmmDataRow
              key={`${groupIndex}-${hitIndex}`}
              title={String(group.query ?? "query")}
              detail={String(hit.abstractText ?? hit.abstract_text ?? "")}
              meta={`memory=${String(memoryId || "-")} · turn=${String(hit.sourceTurnId ?? hit.source_turn_id ?? "-")}`}
              action={memoryId > 0 && props.onDeleteMemory ? {
                label: "删除记忆",
                onClick: () => props.onDeleteMemory?.(memoryId),
              } : undefined}
            />
          );
        }))}
      </div>
    );
  }
  if (props.result.kind === "get_turn_details") {
    const turns = vmmResultArray(props.result, "turns");
    return <div className="vmm-row-list">{turns.map((turn, index) => <VmmDataRow key={index} title={`turn=${String(turn.turnId ?? turn.turn_id ?? "-")}`} detail={String(turn.userQuestion ?? turn.user_question ?? "")} meta={String(turn.assistantAnswer ?? turn.assistant_answer ?? "")} />)}</div>;
  }
  if (props.result.kind === "write_memories") {
    const items = vmmResultArray(props.result, "items");
    return <div className="vmm-row-list">{items.map((item, index) => <VmmDataRow key={index} title={`memory=${String(item.memoryId ?? item.memory_id ?? "-")}`} detail={`deduped=${String(item.deduped ?? false)}`} />)}</div>;
  }
  if (props.result.kind === "delete_memories") {
    const response = vmmObject(props.result.response);
    return (
      <div className="vmm-row-list">
        <VmmDataRow title="已删除" detail={vmmIdListText(response.deletedMemoryIds ?? response.deleted_memory_ids)} meta={`向量行 ${String(response.deletedVectorRows ?? response.deleted_vector_rows ?? 0)}`} />
        <VmmDataRow title="未找到" detail={vmmIdListText(response.notFoundMemoryIds ?? response.not_found_memory_ids)} meta={`trace=${String(response.traceId ?? response.trace_id ?? "-")}`} />
      </div>
    );
  }
  return <VmmJsonPreview title="结果" value={props.result.response} />;
}

// VmmDataRow renders one compact result row.
// VmmDataRow 渲染单条紧凑结果行。
function VmmDataRow(props: {
  title: string
  detail: string
  meta?: string
  action?: {
    label: string
    onClick: () => void
  }
}) {
  return (
    <article className="vmm-data-row">
      <div className="vmm-data-row-header">
        <strong>{props.title}</strong>
        {props.action ? (
          <button aria-label={props.action.label} className="vmm-row-action-button" onClick={props.action.onClick} title={props.action.label} type="button">
            <Trash2 size={14} />
          </button>
        ) : null}
      </div>
      <span>{props.detail}</span>
      {props.meta ? <small>{props.meta}</small> : null}
    </article>
  );
}

// VmmJsonPreview renders one JSON preview block.
// VmmJsonPreview 渲染单个 JSON 预览块。
export function VmmJsonPreview(props: {
  // Preview title.
  // 预览标题。
  title: string
  // Value serialized as formatted JSON.
  // 格式化为 JSON 的值。
  value: unknown
}) {
  return (
    <div className="vmm-json-preview">
      <strong>{props.title}</strong>
      <pre>{JSON.stringify(props.value, null, 2)}</pre>
    </div>
  );
}

// vmmDiagnosticLabel returns a user-facing diagnostic label.
// vmmDiagnosticLabel 返回面向用户的诊断标签。
function vmmDiagnosticLabel(value: string): string {
  const labels: Record<string, string> = {
    ready: "ready",
    disabled_by_config: "已关闭",
    missing_grpc_target: "缺少 gRPC",
    grpc_offline: "gRPC 离线",
    missing_user_id: "缺少用户",
    missing_project_id: "缺少项目",
  };
  return labels[value] ?? value;
}

// vmmResultArray reads an array from the latest VMM result response.
// vmmResultArray 从最近 VMM 结果响应中读取数组。
function vmmResultArray(result: VmmMemoryActionResult | null, key: string): Record<string, unknown>[] {
  return vmmArray(vmmObject(result?.response)[key]);
}

// vmmArray normalizes unknown values into object arrays.
// vmmArray 将未知值规范化为对象数组。
function vmmArray(value: unknown): Record<string, unknown>[] {
  return Array.isArray(value) ? value.filter((item): item is Record<string, unknown> => typeof item === "object" && item !== null) : [];
}

// vmmObject normalizes unknown values into an object.
// vmmObject 将未知值规范化为对象。
function vmmObject(value: unknown): Record<string, unknown> {
  return typeof value === "object" && value !== null ? value as Record<string, unknown> : {};
}

// vmmNumber normalizes unknown values into a finite number.
// vmmNumber 将未知值规范化为有限数字。
function vmmNumber(value: unknown): number {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === "string") {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : 0;
  }
  return 0;
}

// vmmIdListText renders one unknown id list as compact text.
// vmmIdListText 将未知 ID 列表渲染为紧凑文本。
function vmmIdListText(value: unknown): string {
  if (!Array.isArray(value) || value.length === 0) {
    return "无";
  }
  return value.map(String).join("、");
}
