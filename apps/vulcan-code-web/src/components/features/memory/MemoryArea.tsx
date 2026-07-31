import * as React from "react"

import type { VmmMemorySnapshot } from "@/types"

import {
  useMemoryAreaController,
  type MemoryActionProps,
} from "./use-memory-area-controller"
import { VmmInferenceSettings } from "./vmm-inference-settings"
import { VmmDataPage } from "./vmm-data-page"
import { VmmAdvancedPage } from "./vmm-advanced-page"
import { VmmIdentityProjectPage } from "./vmm-identity-project-page"
import { VmmOverviewPage } from "./vmm-overview-page"
import { VmmProfilePage } from "./vmm-profile-page"
import { VmmRulesPage } from "./vmm-rules-page"
import {
  VmmSetupDialog,
  type VmmQuickSetupInput,
} from "./vmm-setup-dialog"

export { vmmSectionIcon } from "./vmm-memory-sections"
export type { MemoryActionProps } from "./use-memory-area-controller"

// MemorySubAreaId identifies the selected VMM memory management page.
// MemorySubAreaId 标识当前选中的 VMM 记忆管理页面。
export type MemorySubAreaId = string

// shouldRequestVmmManagementCatalog decides whether the live VMM directory can be loaded safely.
// shouldRequestVmmManagementCatalog 判断当前是否可以安全加载实时 VMM 管理目录。
export function shouldRequestVmmManagementCatalog(input: {
  // Current visible memory section.
  // 当前可见的记忆分区。
  activeSection: MemorySubAreaId
  // Whether another memory operation owns the exclusive request channel.
  // 是否已有其他记忆操作占用独占请求通道。
  operationBusy: boolean
  // Whether the configured VMM transport has completed its readiness handshake.
  // 已配置 VMM 传输是否已完成就绪握手。
  processReady: boolean
  // Whether this page generation already issued the directory request.
  // 当前页面代次是否已经发起过目录请求。
  requestIssued: boolean
  // Current retained management catalog.
  // 当前保留的管理目录。
  catalog: VmmMemorySnapshot["managementCatalog"]
}): boolean {
  return (
    (
      input.activeSection === "overview"
      || input.activeSection === "identity_projects"
      || input.activeSection === "profile"
    )
    && input.processReady
    && !input.operationBusy
    && !input.requestIssued
    && !input.catalog.loading
    && input.catalog.users.length === 0
    && input.catalog.projects.length === 0
  )
}

// MemoryArea renders the VMM memory management workbench.
// MemoryArea 渲染 VMM 记忆管理工作台。
export function MemoryArea(props: {
  // Active VMM section selected from the shared feature sidebar.
  // 从共享功能侧栏选中的当前 VMM 分区。
  activeSection: MemorySubAreaId
  // Service-backed VMM actions.
  // 服务端支持的 VMM 动作。
  actions: MemoryActionProps
  // Opens model credential management when required inference routes cannot be configured.
  // 在无法配置必需推理路由时打开模型凭据管理。
  onOpenCredentialManagement: () => void
  // Detailed VMM snapshot.
  // 详细 VMM 快照。
  snapshot: VmmMemorySnapshot
  // Selects another VMM section from actions rendered inside the page.
  // 从页面内部操作选择另一个 VMM 分区。
  onSelectSection: (section: MemorySubAreaId) => void
}) {
  // activeSection falls back to overview when stale navigation references an unavailable page.
  // activeSection 在旧导航引用不可用页面时回退到总览。
  const activeSection = vmmVisibleSection(props.activeSection)
  // controller owns exact draft routes and the exclusive asynchronous operation channel.
  // controller 持有准确草稿路由与独占异步操作通道。
  const controller = useMemoryAreaController({
    actions: props.actions,
    snapshot: props.snapshot,
  });
  // Controller fields expose the rendered scope, exact drafts, and guarded action callbacks.
  // 控制器字段暴露渲染作用域、准确草稿与受保护动作回调。
  const {
    applyDraft,
    clearFeedback,
    globalDraft,
    notice,
    operationBusy,
    result,
    runAction,
    saveDraft,
    setGlobalDraft,
  } = controller;
  // setupDialogOpen controls the first-time managed VMM setup workflow.
  // setupDialogOpen 控制首次托管 VMM 快速配置流程。
  const [setupDialogOpen, setSetupDialogOpen] = React.useState(false)
  // catalogLoadRequested prevents repeated automatic directory requests during snapshot refreshes.
  // catalogLoadRequested 防止快照刷新期间重复发起自动目录请求。
  const catalogLoadRequested = React.useRef(false)

  // Page navigation starts a new feedback generation so an earlier page cannot leak notices.
  // 页面导航会开启新的反馈代次，避免先前页面泄漏提示。
  React.useEffect(() => {
    clearFeedback()
  }, [activeSection, clearFeedback])

  React.useEffect(() => {
    const catalog = props.snapshot.managementCatalog
    if (!props.snapshot.runtime.processReady) {
      catalogLoadRequested.current = false
    }
    if (!shouldRequestVmmManagementCatalog({
      activeSection,
      operationBusy,
      processReady: props.snapshot.runtime.processReady,
      requestIssued: catalogLoadRequested.current,
      catalog,
    })) {
      return
    }
    catalogLoadRequested.current = true
    // One automatic request is enough for this ready process generation. A failed request remains
    // visible and user-retryable instead of creating an unbounded background retry loop.
    // 当前就绪进程代次只自动请求一次；失败会保持可见并允许用户重试，避免产生无界后台重试循环。
    void runAction("load_users_projects")
  }, [
    activeSection,
    operationBusy,
    props.snapshot.managementCatalog,
    props.snapshot.runtime.processReady,
    runAction,
  ])

  // handleQuickSetup persists the selected service mode, verifies it, creates identity bindings, and applies one explicit scope.
  // handleQuickSetup 持久化选中的服务模式、完成验证、创建身份绑定并应用到一个明确范围。
  const handleQuickSetup = React.useCallback(async (input: VmmQuickSetupInput): Promise<boolean> => {
    // connectionDraft is saved before any RPC so every following action uses the exact visible connection choice.
    // connectionDraft 在任何 RPC 前保存，确保后续动作使用用户可见的准确连接选择。
    const connectionDraft = {
      ...globalDraft,
      enabled: false,
      grpcTarget: input.mode === "external" ? input.externalEndpoint : "",
      grpcApiKey: input.mode === "external" ? input.externalApiKey : "",
      managementBaseUrl: input.mode === "external" ? input.externalManagementBaseUrl : "",
      managementBearerToken: input.mode === "external" ? input.externalManagementToken : "",
      managedInstallEnabled: input.mode === "managed",
      autoStartPolicy: input.mode === "managed" ? "on_start" : "manual",
    }
    if (!await applyDraft("global", null, connectionDraft)) {
      return false
    }
    // Saving the managed on-start draft performs the single authoritative startup reconciliation.
    // 保存托管自动启动草稿会执行唯一权威的启动协调，此处仅验证连接，避免重复启动。
    if (!await runAction("healthz")) {
      return false
    }
    if (!await runAction("management_capabilities")) {
      return false
    }
    const userResult = await runAction("resolve_user", {
      userRef: input.userName,
      confirmCreate: true,
    })
    const userId = userResult?.data.kind === "resolve_user"
      ? String(userResult.data.value.user?.user_id ?? "")
      : ""
    if (!userId) {
      return false
    }
    const projectResult = await runAction("ensure_project", {
      projectPath: vmmSetupProjectPath(input.projectName),
      confirmCreate: true,
    })
    const projectId = projectResult?.data.kind === "ensure_project"
      ? String(projectResult.data.value.project?.project_id ?? "")
      : ""
    if (!projectId) {
      return false
    }
    // identityDraft stores durable VMM ids while keeping internal ids out of the setup UI.
    // identityDraft 保存持久化 VMM 标识，同时不在配置界面暴露内部标识。
    const identityDraft = {
      ...connectionDraft,
      enabled: input.scope === "all_projects",
      userId,
      projectId,
    }
    if (!await applyDraft("global", null, identityDraft)) {
      return false
    }
    if (input.scope === "current_project") {
      const currentProjectId = props.snapshot.selectedProjectId
        ?? props.snapshot.projectScopes[0]?.projectId
        ?? null
      if (!currentProjectId) {
        return false
      }
      const projectDraft = {
        ...identityDraft,
        enabled: true,
        hasOverride: true,
      }
      if (!await applyDraft("project", currentProjectId, projectDraft)) {
        return false
      }
    }
    await runAction("healthz")
    await runAction("load_users_projects")
    return true
  }, [
    applyDraft,
    globalDraft,
    props.snapshot.projectScopes,
    props.snapshot.selectedProjectId,
    runAction,
  ])

  // overviewPage is the safe product fallback for stale navigation and first-time setup.
  // overviewPage 是旧导航与首次配置共用的安全产品回退页。
  const overviewPage = (
    <section className="vmm-memory-page">
      <div className="vmm-memory-shell status-only">
        <main className="vmm-memory-main vmm-status-page">
          <VmmOverviewPage
            busy={operationBusy}
            onControl={async (action) => {
              await runAction(action)
            }}
            onOpenAdvanced={() => props.onSelectSection("advanced")}
            onOpenSetup={() => setSetupDialogOpen(true)}
            snapshot={props.snapshot}
          />
          <VmmSetupDialog
            busy={operationBusy}
            currentProjectAvailable={Boolean(
              props.snapshot.selectedProjectId
              ?? props.snapshot.projectScopes[0]?.projectId,
            )}
            defaultProjectName={props.snapshot.projectScopes.find(
              (project) => project.projectId === props.snapshot.selectedProjectId,
            )?.name
              ?? props.snapshot.projectScopes[0]?.name
              ?? "Vulcan Code"}
            defaultUserName={props.snapshot.displayIdentity.userName ?? "Vulcan Code"}
            notice={notice}
            onOpenCredentialManagement={props.onOpenCredentialManagement}
            onOpenChange={setSetupDialogOpen}
            onSubmit={handleQuickSetup}
            open={setupDialogOpen}
          />
        </main>
      </div>
    </section>
  )

  if (activeSection === "overview") {
    return overviewPage
  }

  if (activeSection === "inference") {
    return (
      <section className="vmm-memory-page">
        <div className="vmm-memory-shell status-only">
          <main className="vmm-memory-main vmm-status-page">
            <VmmInferenceSettings />
          </main>
        </div>
      </section>
    )
  }

  if (activeSection === "identity_projects") {
    return (
      <section className="vmm-memory-page">
        <div className="vmm-memory-shell status-only">
          <main className="vmm-memory-main vmm-status-page">
            <VmmIdentityProjectPage
              busy={operationBusy}
              defaultProjectId={globalDraft.projectId}
              defaultUserId={globalDraft.userId}
              notice={notice}
              onRunAction={runAction}
              onSetDefaultProject={async (projectId) => applyDraft("global", null, {
                ...globalDraft,
                projectId,
              })}
              onSetDefaultUser={async (userId) => applyDraft("global", null, {
                ...globalDraft,
                userId,
              })}
              result={result}
              snapshot={props.snapshot}
            />
          </main>
        </div>
      </section>
    )
  }

  if (activeSection === "rules") {
    return (
      <section className="vmm-memory-page">
        <div className="vmm-memory-shell status-only">
          <main className="vmm-memory-main vmm-status-page">
            <VmmRulesPage
              busy={operationBusy}
              draft={globalDraft}
              notice={notice}
              onApply={applyDraft}
              onChange={(patch) => setGlobalDraft((draft) => ({ ...draft, ...patch }))}
              projects={props.snapshot.projectScopes}
            />
          </main>
        </div>
      </section>
    )
  }

  if (activeSection === "content") {
    return (
      <section className="vmm-memory-page">
        <div className="vmm-memory-shell status-only">
          <main className="vmm-memory-main vmm-status-page">
            <VmmDataPage
              busy={operationBusy}
              onRunAction={runAction}
              result={result}
              snapshot={props.snapshot}
            />
          </main>
        </div>
      </section>
    )
  }

  if (activeSection === "profile") {
    return (
      <section className="vmm-memory-page">
        <div className="vmm-memory-shell status-only">
          <main className="vmm-memory-main vmm-status-page">
            <VmmProfilePage
              busy={operationBusy}
              onRunAction={runAction}
              result={result}
              snapshot={props.snapshot}
            />
          </main>
        </div>
      </section>
    )
  }

  if (activeSection === "advanced") {
    return (
      <section className="vmm-memory-page">
        <div className="vmm-memory-shell status-only">
          <main className="vmm-memory-main vmm-status-page">
            <VmmAdvancedPage
              busy={operationBusy}
              draft={globalDraft}
              notice={notice}
              onChange={(patch) => setGlobalDraft((draft) => ({ ...draft, ...patch }))}
              onMaintenance={async (action) => {
                const maintenanceResult = await runAction(action)
                return maintenanceResult?.data.kind === "maintenance"
                  ? maintenanceResult.data.value
                  : null
              }}
              onSave={() => saveDraft("global")}
              onTestManagement={async () => {
                if (!await saveDraft("global")) return false
                return Boolean(await runAction("management_capabilities"))
              }}
              onStartRuntime={async () => Boolean(await runAction("managed_runtime_start"))}
              snapshot={props.snapshot}
            />
          </main>
        </div>
      </section>
    )
  }

  return overviewPage
}

// vmmVisibleSection returns one supported product page and falls back to the overview.
// vmmVisibleSection 返回一个受支持的产品页面，并在未知输入时回退到总览。
function vmmVisibleSection(section: MemorySubAreaId): MemorySubAreaId {
  const visibleSections = new Set([
    "overview",
    "inference",
    "identity_projects",
    "rules",
    "content",
    "profile",
    "advanced",
  ])
  return visibleSections.has(section) ? section : "overview"
}

// vmmSetupProjectPath converts one visible project name into the canonical VMM hierarchy.
// vmmSetupProjectPath 将一个可见项目名称转换为标准 VMM 层级路径。
function vmmSetupProjectPath(projectName: string): string {
  const normalizedName = projectName.trim().replace(/[\\/]+/g, "-")
  return `default/default/${normalizedName}`
}


