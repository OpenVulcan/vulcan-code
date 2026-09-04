import * as React from "react"

import type { VmmConfigDraftSummary, VmmMemorySnapshot } from "@/types"

import {
  useMemoryAreaController,
  type MemoryActionProps,
} from "./use-memory-area-controller"
import { VmmInferenceSettings } from "./vmm-inference-settings"
import { VmmContentPage } from "./vmm-content-page"
import { VmmDataPage } from "./vmm-data-page"
import { VmmAdvancedPage } from "./vmm-advanced-page"
import { VmmIdentityProjectPage } from "./vmm-identity-project-page"
import { VmmNativeProjectPage } from "./vmm-native-project-page"
import { VmmNativeMaintenancePage } from "./vmm-native-maintenance-page"
import { VmmNativeResourcesPage } from "./vmm-native-resources-page"
import { VmmOverviewPage } from "./vmm-overview-page"
import { VmmProfilePage } from "./vmm-profile-page"
import { VmmRulesPage } from "./vmm-rules-page"
import {
  VmmSetupDialog,
  type VmmQuickSetupCatalog,
  type VmmQuickSetupConnectionInput,
  type VmmQuickSetupInput,
} from "./vmm-setup-dialog"

export { vmmSectionIcon } from "./vmm-memory-sections"
export type { MemoryActionProps } from "./use-memory-area-controller"

// MemorySubAreaId identifies the selected VMM memory management page.
// MemorySubAreaId 标识当前选中的 VMM 记忆管理页面。
export type MemorySubAreaId = string

// vmmUsesNativeMemoryMaintenance selects the process-local content workbench without management HTTP.
// vmmUsesNativeMemoryMaintenance 选择不经过管理 HTTP 的进程内记忆内容工作台。
export function vmmUsesNativeMemoryMaintenance(
  deployment: VmmMemorySnapshot["experienceState"]["deployment"],
): boolean {
  return deployment === "native"
}

// shouldRequestVmmManagementCatalog decides whether the live VMM directory can be loaded safely.
// shouldRequestVmmManagementCatalog 判断当前是否可以安全加载实时 VMM 管理目录。
export function shouldRequestVmmManagementCatalog(input: {
  // Current visible memory section.
  // 当前可见的记忆分区。
  activeSection: MemorySubAreaId
  // Authoritative configured backend deployment.
  // 权威的已配置后端部署方式。
  deployment: VmmMemorySnapshot["experienceState"]["deployment"]
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
    input.deployment === "external"
    && (
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

// externalSetupActionConnection projects only gRPC transport fields into one transient action payload.
// externalSetupActionConnection 只把 gRPC 传输字段投影到一次临时动作载荷。
export function externalSetupActionConnection(input: VmmQuickSetupConnectionInput) {
  return {
    connection: {
      grpcTarget: input.externalEndpoint.trim(),
      grpcApiKey: input.externalApiKey,
    },
  }
}

// materializeExternalFixedBindingPlan replaces only pending default rows with the confirmed remote fixed project ID.
// materializeExternalFixedBindingPlan 只把待定的默认行替换为已确认的远端固定项目 ID。
// Parameters: plan contains user-visible row decisions and fixedProjectId is the authoritative remote ID.
// 参数：plan 包含用户可见的逐行决定，fixedProjectId 是权威远端 ID。
// Returns: a service-ready plan that preserves explicit create and alternate-existing choices.
// 返回值：保留明确创建及其他已有项目选择、可直接提交给服务端的计划。
export function materializeExternalFixedBindingPlan(
  plan: VmmQuickSetupInput["bindingPlan"],
  fixedProjectId: number,
): VmmQuickSetupInput["bindingPlan"] {
  if (!Number.isInteger(fixedProjectId) || fixedProjectId <= 0) {
    throw new Error("External VMM fixed project binding requires a positive project id.")
  }
  return plan.map((entry) => (
    entry.action === "bind_existing" && entry.targetProjectId == null
      ? { ...entry, targetProjectId: fixedProjectId }
      : entry
  ))
}

// shouldRouteAdvancedSaveToSetup protects backend switches with the full project reconciliation flow.
// shouldRouteAdvancedSaveToSetup 通过完整项目对账流程保护后端切换。
// Parameters: draftBackend is the edited backend and savedBackend is the currently active backend.
// 参数：draftBackend 是编辑后的后端，savedBackend 是当前活动后端。
// Returns: true only when a direct advanced save would bypass required binding decisions.
// 返回值：仅当直接高级保存会绕过必需绑定决定时返回 true。
export function shouldRouteAdvancedSaveToSetup(
  draftBackend: VmmConfigDraftSummary["backendMode"],
  savedBackend: VmmConfigDraftSummary["backendMode"],
): boolean {
  return draftBackend !== savedBackend
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
  // setupDialogOpen controls the first-time native or external VMM setup workflow.
  // setupDialogOpen 控制首次原生或外置 VMM 快速配置流程。
  const [setupDialogOpen, setSetupDialogOpen] = React.useState(false)
  // nativeDeployment is the saved service authority; an unsaved settings draft must not reroute pages.
  // nativeDeployment 是已保存的服务端权威；未保存的设置草稿不得改写页面路由。
  const nativeDeployment = vmmUsesNativeMemoryMaintenance(
    props.snapshot.experienceState.deployment,
  )
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
      deployment: props.snapshot.experienceState.deployment,
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
    props.snapshot.experienceState.deployment,
    props.snapshot.managementCatalog,
    props.snapshot.runtime.processReady,
    runAction,
  ])

  // handleQuickSetupPrepare verifies one disabled backend and returns its authoritative project catalog.
  // handleQuickSetupPrepare 校验一个禁用状态的后端并返回其权威项目目录。
  const handleQuickSetupPrepare = React.useCallback(async (
    input: VmmQuickSetupConnectionInput,
  ): Promise<VmmQuickSetupCatalog | null> => {
    if (input.mode === "native") {
      const catalogResult = await runAction("native_project_catalog")
      return catalogResult?.data.kind === "native_project_catalog"
        ? { native: catalogResult.data.value }
        : null
    }
    // connection remains request-scoped until the user confirms the complete setup.
    // connection 在用户确认完整配置前只存在于当前请求中。
    const connection = externalSetupActionConnection(input)
    if (!await runAction("healthz", connection)) return null
    const catalogResult = await runAction("load_users_projects", connection)
    return catalogResult?.data.kind === "users_projects"
      ? {
          externalUsers: catalogResult.data.value.users,
          externalProjects: catalogResult.data.value.projects,
        }
      : null
  }, [runAction])

  // handleQuickSetup resolves external identities when needed and enables the selected backend with explicit bindings.
  // handleQuickSetup 在需要时解析外置身份，并使用显式绑定启用所选后端。
  const handleQuickSetup = React.useCallback(async (input: VmmQuickSetupInput): Promise<boolean> => {
    let userId = ""
    let projectId = ""
    if (input.mode === "external") {
      // connection is sent explicitly so resolving remote identities still precedes persistence.
      // connection 被明确随请求发送，使远端身份解析仍然发生在持久化之前。
      const connection = externalSetupActionConnection(input)
      const userResult = await runAction("resolve_user", {
        ...connection,
        userRef: input.userName,
        confirmCreate: true,
      })
      userId = userResult?.data.kind === "resolve_user"
        ? String(userResult.data.value.user?.user_id ?? "")
        : ""
      if (!userId) return false
      if (input.bindingMode === "fixed") {
        if (input.externalFixedAction === "bind_existing") {
          projectId = String(input.externalFixedTargetProjectId ?? "")
        } else {
          const projectResult = await runAction("ensure_project", {
            ...connection,
            projectPath: `${input.externalProjectPrefix}/${input.externalFixedProjectName}`,
            confirmCreate: true,
          })
          projectId = projectResult?.data.kind === "ensure_project"
            ? String(projectResult.data.value.project?.project_id ?? "")
            : ""
        }
        if (!projectId) return false
      }
    }
    const enabledDraft = {
      ...globalDraft,
      backendMode: input.mode === "native" ? "native" as const : "external_grpc" as const,
      nativeProjectBindingMode: input.mode === "native" ? input.bindingMode : globalDraft.nativeProjectBindingMode,
      externalProjectBindingMode: input.mode === "external" ? input.bindingMode : globalDraft.externalProjectBindingMode,
      nativeFixedProjectId: input.mode === "native" ? String(input.nativeFixedProjectId) : globalDraft.nativeFixedProjectId,
      externalProjectPrefix: input.mode === "external" ? input.externalProjectPrefix : globalDraft.externalProjectPrefix,
      enabled: true,
      grpcTarget: input.mode === "external" ? input.externalEndpoint : globalDraft.grpcTarget,
      grpcApiKey: input.mode === "external" ? input.externalApiKey : globalDraft.grpcApiKey,
      managementBaseUrl: input.mode === "external" ? input.externalManagementBaseUrl : globalDraft.managementBaseUrl,
      managementBearerToken: input.mode === "external" ? input.externalManagementToken : globalDraft.managementBearerToken,
      userId: input.mode === "external" ? userId : globalDraft.userId,
      projectId: input.mode === "external" ? projectId : globalDraft.projectId,
    }
    // fixedPlan materializes only rows that retained the global fixed default; explicit row edits survive.
    // fixedPlan 只物化仍采用全局固定默认值的行；逐行明确修改会被保留。
    const fixedPlan = input.mode === "external" && input.bindingMode === "fixed"
      ? materializeExternalFixedBindingPlan(input.bindingPlan, Number(projectId))
      : input.bindingPlan
    if (!await applyDraft("global", null, enabledDraft, fixedPlan)) return false
    if (input.mode === "external") {
      await runAction("healthz")
      await runAction("load_users_projects")
    } else {
      await runAction("native_project_catalog")
    }
    return true
  }, [applyDraft, globalDraft, runAction])

  // saveAdvancedDraft routes backend switches through setup so every unbound project has a decision.
  // saveAdvancedDraft 将后端切换转入快速配置，使每个未绑定项目都有明确决定。
  const saveAdvancedDraft = React.useCallback(async (): Promise<boolean> => {
    if (shouldRouteAdvancedSaveToSetup(
      globalDraft.backendMode,
      props.snapshot.globalDraft.backendMode,
    )) {
      setSetupDialogOpen(true)
      props.onSelectSection("overview")
      return false
    }
    return saveDraft("global")
  }, [globalDraft.backendMode, props.onSelectSection, props.snapshot.globalDraft.backendMode, saveDraft])

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
            notice={notice}
            onOpenCredentialManagement={props.onOpenCredentialManagement}
            onOpenChange={setSetupDialogOpen}
            onPrepare={handleQuickSetupPrepare}
            onSubmit={handleQuickSetup}
            open={setupDialogOpen}
            projects={props.snapshot.projectScopes}
          />
        </main>
      </div>
    </section>
  )

  if (activeSection === "overview") {
    return overviewPage
  }

  if (activeSection === "inference") {
    if (!nativeDeployment) return overviewPage
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
    if (nativeDeployment) {
      return (
        <section className="vmm-memory-page">
          <div className="vmm-memory-shell status-only">
            <main className="vmm-memory-main vmm-status-page">
              <VmmNativeProjectPage
                busy={operationBusy}
                notice={notice}
                onRunAction={runAction}
                projects={props.snapshot.projectScopes}
              />
            </main>
          </div>
        </section>
      )
    }
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

  if (activeSection === "resource_rules") {
    if (!nativeDeployment) return overviewPage
    return (
      <section className="vmm-memory-page">
        <div className="vmm-memory-shell status-only">
          <main className="vmm-memory-main vmm-status-page">
            <VmmNativeResourcesPage busy={operationBusy} onRunAction={runAction} />
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
            {nativeDeployment ? (
              <VmmContentPage
                busy={operationBusy}
                onRunAction={runAction}
                result={result}
                snapshot={props.snapshot}
              />
            ) : (
              <VmmDataPage
                busy={operationBusy}
                onRunAction={runAction}
                result={result}
                snapshot={props.snapshot}
              />
            )}
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

  if (activeSection === "maintenance") {
    if (!nativeDeployment) return overviewPage
    return (
      <section className="vmm-memory-page">
        <div className="vmm-memory-shell status-only">
          <main className="vmm-memory-main vmm-status-page">
            <VmmNativeMaintenancePage busy={operationBusy} onRunAction={runAction} />
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
              onSave={saveAdvancedDraft}
              onRunAction={runAction}
              onTestManagement={async () => {
                if (!await saveAdvancedDraft()) return false
                return Boolean(await runAction("management_capabilities"))
              }}
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
    "resource_rules",
    "content",
    "profile",
    "maintenance",
    "advanced",
  ])
  return visibleSections.has(section) ? section : "overview"
}

