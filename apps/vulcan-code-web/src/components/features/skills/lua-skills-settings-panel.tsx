import * as React from "react"

import { useI18n } from "@/i18n"
import type {
  LuaSkillsCatalogSummary,
  LuaSkillsManagedRuntimeComponentSummary,
  LuaSkillsManagedRuntimeBootstrapStatus,
  LuaSkillsManagedRuntimeProgressSummary,
  LuaSkillsManagedRuntimeTarget,
  LuaSkillsManagedRuntimeSettings,
  LuaSkillsRuntimeSettings,
  LuaSkillsToolCacheSettings,
  LuaVmPoolSettings,
} from "@/types"

import type { SkillActionProps } from "./skill-area-types"

// LuaVmPoolKey identifies one exact VM pool in the package settings document.
// LuaVmPoolKey 标识包设置文档中的一个精确 VM 池。
type LuaVmPoolKey = "vmPool" | "runLuaPool"

// LuaVmPoolNumberKey identifies one numeric VM pool setting.
// LuaVmPoolNumberKey 标识一个数值型 VM 池设置。
type LuaVmPoolNumberKey = keyof LuaVmPoolSettings

// LuaSkillsToolCacheNumberKey identifies one numeric tool-cache setting.
// LuaSkillsToolCacheNumberKey 标识一个数值型工具缓存设置。
type LuaSkillsToolCacheNumberKey = keyof LuaSkillsToolCacheSettings

// LuaSkillsNetworkBooleanKey identifies one Boolean network setting.
// LuaSkillsNetworkBooleanKey 标识一个布尔型网络设置。
type LuaSkillsNetworkBooleanKey = "allowDownloads" | "enablePrivateUrlSkillInstall"

// LuaSkillsNetworkTextKey identifies one optional network endpoint setting.
// LuaSkillsNetworkTextKey 标识一个可选网络端点设置。
type LuaSkillsNetworkTextKey = "githubBaseUrl" | "githubApiBaseUrl" | "officialSkillHubBaseUrl"

// LuaSkillsCapabilityBooleanKey identifies one Boolean host capability setting.
// LuaSkillsCapabilityBooleanKey 标识一个布尔型宿主能力设置。
type LuaSkillsCapabilityBooleanKey = "enableSkillManagementBridge" | "enableManagedIoCompat"

// LuaSkillsManagedRuntimeNumberKey identifies one required numeric managed-runtime setting.
// LuaSkillsManagedRuntimeNumberKey 标识一个必填的数值型受管运行时设置。
type LuaSkillsManagedRuntimeNumberKey = Exclude<
  keyof LuaSkillsManagedRuntimeSettings,
  "invokeDefaultTimeoutMs"
>

// managedRuntimeStatusLabel resolves one runtime validation state into the active locale.
// managedRuntimeStatusLabel 将一个运行时校验状态解析为当前界面语言。
function managedRuntimeStatusLabel(
  status: LuaSkillsManagedRuntimeBootstrapStatus,
  t: ReturnType<typeof useI18n>["t"],
): string {
  switch (status) {
    case "ready":
      return t("skills.luaSettings.managedRuntimeStatusReady")
    case "installed":
      return t("skills.luaSettings.managedRuntimeStatusInstalled")
    case "unsupported":
      return t("skills.luaSettings.managedRuntimeStatusUnsupported")
    case "unavailable":
      return t("skills.luaSettings.managedRuntimeStatusUnavailable")
  }
}

// managedRuntimeProgressBelongsTo reports whether one installer event belongs to a runtime row.
// managedRuntimeProgressBelongsTo 判断一个安装器事件是否属于指定运行时行。
function managedRuntimeProgressBelongsTo(
  progress: LuaSkillsManagedRuntimeProgressSummary,
  target: LuaSkillsManagedRuntimeTarget,
): boolean {
  if (target === "python") {
    return progress.component === "python" || progress.component === "uv"
  }
  return progress.component === "node" || progress.component === "pnpm"
}

// ManagedRuntimeRow renders one independent runtime status and repair control without a card frame.
// ManagedRuntimeRow 以无卡片边框形式渲染一个独立运行时状态与修复控件。
function ManagedRuntimeRow(props: {
  // Human-facing runtime name.
  // 面向用户的运行时名称。
  label: string
  // Exact backend validation result.
  // 精确的后端校验结果。
  runtime: LuaSkillsManagedRuntimeComponentSummary
  // Live progress belonging to this runtime when active.
  // 当前运行时处于活动状态时的实时进度。
  progress: LuaSkillsManagedRuntimeProgressSummary | null
  // Whether this runtime is currently being reinstalled.
  // 当前运行时是否正在重新安装。
  retrying: boolean
  // Whether another managed runtime mutation prevents this action.
  // 是否因另一项受管运行时变更而禁用当前操作。
  actionDisabled: boolean
  // Starts an independent reinstall for this runtime.
  // 为当前运行时启动独立重装。
  onRetry: () => void
}) {
  // t resolves localized row status and action labels.
  // t 解析本地化的行状态与操作文案。
  const { t } = useI18n()
  // progressValue uses exact byte progress when available and otherwise leaves the bar indeterminate.
  // progressValue 在可用时使用精确字节进度，否则让进度条保持不确定状态。
  const progressValue = props.progress?.completedBytes != null
    && props.progress.totalBytes != null
    && props.progress.totalBytes > 0
    ? Math.min(100, (props.progress.completedBytes / props.progress.totalBytes) * 100)
    : null
  // installed treats both pre-existing and newly installed validated states as ready.
  // installed 将原有有效状态与新安装后的有效状态都视为已安装。
  const installed = props.runtime.status === "ready" || props.runtime.status === "installed"

  return (
    <div className={`lua-managed-runtime-row ${props.runtime.status}`}>
      <div className="lua-managed-runtime-row-main">
        <span className="lua-managed-runtime-row-name">{props.label}</span>
        <span className="lua-managed-runtime-row-version">{props.runtime.version}</span>
        <div className="lua-managed-runtime-row-action">
          <span>{installed ? t("skills.luaSettings.managedRuntimeInstalled") : managedRuntimeStatusLabel(props.runtime.status, t)}</span>
          {props.runtime.status === "unavailable" ? (
            <button
              className="skill-action-button"
              type="button"
              disabled={props.actionDisabled}
              title={props.runtime.diagnostic}
              onClick={props.onRetry}
            >
              {props.retrying ? t("skills.luaSettings.managedRuntimeRetrying") : t("skills.luaSettings.managedRuntimeReinstall")}
            </button>
          ) : null}
        </div>
      </div>
      {props.retrying ? (
        <div className="lua-managed-runtime-row-progress">
          <span>{props.progress?.message ?? t("skills.luaSettings.managedRuntimePreparing")}</span>
          {progressValue == null ? <progress /> : <progress max={100} value={progressValue} />}
          {props.progress?.completedBytes != null && props.progress.totalBytes != null ? (
            <small>
              {(props.progress.completedBytes / 1_048_576).toFixed(1)} MB / {(props.progress.totalBytes / 1_048_576).toFixed(1)} MB
            </small>
          ) : null}
        </div>
      ) : null}
    </div>
  )
}

// CombinedManagedRuntimeRow renders a compatibility state when independent evidence is unavailable.
// CombinedManagedRuntimeRow 在缺少独立判断依据时渲染兼容状态。
function CombinedManagedRuntimeRow(props: {
  // Aggregate backend status retained by older snapshots.
  // 旧快照保留的后端聚合状态。
  status: LuaSkillsManagedRuntimeBootstrapStatus
}) {
  // t resolves the localized compatibility row.
  // t 解析本地化兼容状态行。
  const { t } = useI18n()
  // normal identifies a complete aggregate runtime layout without attributing component state.
  // normal 标识完整的聚合运行时布局，但不推断具体组件状态。
  const normal = props.status === "ready" || props.status === "installed"
  return (
    <div className={`lua-managed-runtime-row ${props.status}`}>
      <div className="lua-managed-runtime-row-main">
        <span className="lua-managed-runtime-row-name">{t("skills.luaSettings.combinedRuntime")}</span>
        <div className="lua-managed-runtime-row-action">
          <span>{normal ? t("skills.luaSettings.managedRuntimeNormal") : managedRuntimeStatusLabel(props.status, t)}</span>
        </div>
      </div>
    </div>
  )
}

// cloneLuaSkillsRuntimeSettings creates an editable copy without sharing nested arrays.
// cloneLuaSkillsRuntimeSettings 创建不共享嵌套数组的可编辑副本。
//
// Parameters: settings is the exact Vulcan-owned settings document returned by the service.
// 参数：settings 是服务返回的精确 Vulcan 自有设置文档。
// Returns: a structurally independent settings document suitable for local editing.
// 返回值：适合本地编辑且结构独立的设置文档。
function cloneLuaSkillsRuntimeSettings(settings: LuaSkillsRuntimeSettings): LuaSkillsRuntimeSettings {
  return {
    ...settings,
    vmPool: { ...settings.vmPool },
    runLuaPool: { ...settings.runLuaPool },
    toolCache: { ...settings.toolCache },
    network: {
      ...settings.network,
      privateSkillSourceAllowlist: [...settings.network.privateSkillSourceAllowlist],
    },
    capabilities: { ...settings.capabilities },
    managedRuntime: { ...settings.managedRuntime },
  }
}

// runtimeSettingsAreEqual compares two normalized settings documents for dirty-state display.
// runtimeSettingsAreEqual 比较两份规范设置文档以展示脏状态。
//
// Parameters: left and right are complete schema-versioned settings documents.
// 参数：left 与 right 是完整且带 schema 版本的设置文档。
// Returns: true only when every persisted field has the same value and order.
// 返回值：仅当所有持久化字段的值与顺序都一致时返回 true。
function runtimeSettingsAreEqual(left: LuaSkillsRuntimeSettings, right: LuaSkillsRuntimeSettings): boolean {
  return JSON.stringify(left) === JSON.stringify(right)
}

// runtimeSettingsValidationError validates the constraints already enforced by the Rust package.
// runtimeSettingsValidationError 校验 Rust 包已经强制执行的约束。
//
// Parameters: settings is the complete local draft and t resolves localized validation messages.
// 参数：settings 是完整本地草稿，t 用于解析本地化校验信息。
// Returns: the first actionable validation error, or null when the draft can be submitted.
// 返回值：首个可处理的校验错误；草稿可提交时返回 null。
function runtimeSettingsValidationError(
  settings: LuaSkillsRuntimeSettings,
  t: ReturnType<typeof useI18n>["t"],
): string | null {
  const positiveIntegers = [
    settings.vmPool.minSize,
    settings.vmPool.maxSize,
    settings.vmPool.idleTtlSecs,
    settings.runLuaPool.minSize,
    settings.runLuaPool.maxSize,
    settings.runLuaPool.idleTtlSecs,
    settings.toolCache.maxEntries,
    settings.toolCache.defaultTtlSecs,
    settings.toolCache.maxTtlSecs,
    settings.managedRuntime.workerPoolMaxSizePerEnvironment,
    settings.managedRuntime.workerIdleTtlSecs,
    settings.managedRuntime.persistentSessionLimitPerEngine,
    settings.managedRuntime.persistentSessionDefaultBufferLimitBytesPerStream,
  ]
  if (positiveIntegers.some((value) => !Number.isInteger(value) || value <= 0)) {
    return t("skills.luaSettings.validationPositive")
  }
  if (
    settings.vmPool.maxSize < settings.vmPool.minSize
    || settings.runLuaPool.maxSize < settings.runLuaPool.minSize
  ) {
    return t("skills.luaSettings.validationPoolOrder")
  }
  if (settings.toolCache.maxTtlSecs < settings.toolCache.defaultTtlSecs) {
    return t("skills.luaSettings.validationCacheOrder")
  }
  if (
    settings.managedRuntime.invokeDefaultTimeoutMs != null
    && (
      !Number.isInteger(settings.managedRuntime.invokeDefaultTimeoutMs)
      || settings.managedRuntime.invokeDefaultTimeoutMs <= 0
    )
  ) {
    return t("skills.luaSettings.validationPositive")
  }
  return null
}

// LuaSkillsSettingsPanel renders and saves Vulcan-owned LuaSkills engine startup settings.
// LuaSkillsSettingsPanel 渲染并保存 Vulcan 自有 LuaSkills 引擎启动设置。
export function LuaSkillsSettingsPanel(props: {
  // Current LuaSkills catalog including persisted settings and the running-engine comparison.
  // 当前 LuaSkills 目录，包含持久化设置与运行中引擎比较结果。
  catalog: LuaSkillsCatalogSummary
  // Service-backed LuaSkills page actions.
  // 服务端支持的 LuaSkills 页面动作。
  actions: SkillActionProps
}) {
  // t resolves localized settings labels for the current interface locale.
  // t 为当前界面语言解析本地化设置标签。
  const { t } = useI18n()
  // draft stores the editable complete package settings document.
  // draft 保存可编辑的完整包设置文档。
  const [draft, setDraft] = React.useState<LuaSkillsRuntimeSettings>(() => (
    cloneLuaSkillsRuntimeSettings(props.catalog.runtimeSettings)
  ))
  // saving prevents concurrent package settings writes.
  // saving 防止并发写入包设置。
  const [saving, setSaving] = React.useState(false)
  // retryingTarget prevents concurrent installs and identifies the row that owns transient progress.
  // retryingTarget 防止并发安装并标识承载临时进度的运行时行。
  const [retryingTarget, setRetryingTarget] = React.useState<LuaSkillsManagedRuntimeTarget | null>(null)
  // feedback stores the latest successful save message.
  // feedback 保存最近一次成功保存信息。
  const [feedback, setFeedback] = React.useState<string | null>(null)
  // error stores client validation or service persistence failures.
  // error 保存客户端校验或服务持久化失败信息。
  const [error, setError] = React.useState<string | null>(null)

  // committedSettingsKey changes only when persisted values change, not when snapshots recreate objects.
  // committedSettingsKey 仅在持久化值变化时改变，不会因快照重建对象而变化。
  const committedSettingsKey = JSON.stringify(props.catalog.runtimeSettings)
  // committedSettings restores the typed immutable comparison value from its own typed serialization.
  // committedSettings 从自身的类型化序列化中恢复用于比较的不可变类型值。
  const committedSettings = React.useMemo<LuaSkillsRuntimeSettings>(
    () => JSON.parse(committedSettingsKey) as LuaSkillsRuntimeSettings,
    [committedSettingsKey],
  )

  React.useEffect(() => {
    setDraft(cloneLuaSkillsRuntimeSettings(committedSettings))
  }, [committedSettings])

  // dirty records whether the local editor differs from the latest committed package settings.
  // dirty 记录本地编辑器是否不同于最近提交的包设置。
  const dirty = !runtimeSettingsAreEqual(draft, committedSettings)
  // pythonRuntimeProgress is the exact live installer event assigned to the Python row.
  // pythonRuntimeProgress 是分配给 Python 行的精确实时安装器事件。
  const pythonRuntimeProgress = props.catalog.managedRuntimeProgress
    && managedRuntimeProgressBelongsTo(props.catalog.managedRuntimeProgress, "python")
    ? props.catalog.managedRuntimeProgress
    : null
  // nodeRuntimeProgress is the exact live installer event assigned to the Node.js row.
  // nodeRuntimeProgress 是分配给 Node.js 行的精确实时安装器事件。
  const nodeRuntimeProgress = props.catalog.managedRuntimeProgress
    && managedRuntimeProgressBelongsTo(props.catalog.managedRuntimeProgress, "node")
    ? props.catalog.managedRuntimeProgress
    : null
  // managedRuntimeActionDisabled serializes settings-page runtime repairs.
  // managedRuntimeActionDisabled 串行化设置页中的运行时修复操作。
  const managedRuntimeActionDisabled = retryingTarget != null || props.catalog.managedRuntimeProgress != null

  // updatePoolSetting updates one exact numeric field on one exact VM pool.
  // updatePoolSetting 更新一个精确 VM 池上的一个精确数值字段。
  const updatePoolSetting = React.useCallback((
    poolKey: LuaVmPoolKey,
    field: LuaVmPoolNumberKey,
    value: number,
  ) => {
    setDraft((current) => ({
      ...current,
      [poolKey]: { ...current[poolKey], [field]: value },
    }))
    setFeedback(null)
  }, [])

  // updateToolCacheSetting updates one exact numeric tool-cache field.
  // updateToolCacheSetting 更新一个精确的数值型工具缓存字段。
  const updateToolCacheSetting = React.useCallback((field: LuaSkillsToolCacheNumberKey, value: number) => {
    setDraft((current) => ({
      ...current,
      toolCache: { ...current.toolCache, [field]: value },
    }))
    setFeedback(null)
  }, [])

  // updateNetworkBoolean updates one exact Boolean network setting.
  // updateNetworkBoolean 更新一个精确的布尔型网络设置。
  const updateNetworkBoolean = React.useCallback((field: LuaSkillsNetworkBooleanKey, value: boolean) => {
    setDraft((current) => ({
      ...current,
      network: { ...current.network, [field]: value },
    }))
    setFeedback(null)
  }, [])

  // updateNetworkText updates one exact optional network endpoint.
  // updateNetworkText 更新一个精确的可选网络端点。
  const updateNetworkText = React.useCallback((field: LuaSkillsNetworkTextKey, value: string) => {
    setDraft((current) => ({
      ...current,
      network: { ...current.network, [field]: value },
    }))
    setFeedback(null)
  }, [])

  // updateNetworkAllowlist updates the exact private source prefix list from one line per entry.
  // updateNetworkAllowlist 按每行一个条目更新精确的私有来源前缀列表。
  const updateNetworkAllowlist = React.useCallback((value: string) => {
    setDraft((current) => ({
      ...current,
      network: {
        ...current.network,
        privateSkillSourceAllowlist: value.split(/\r?\n/),
      },
    }))
    setFeedback(null)
  }, [])

  // updateCapabilityBoolean updates one exact Boolean host capability.
  // updateCapabilityBoolean 更新一个精确的布尔型宿主能力。
  const updateCapabilityBoolean = React.useCallback((field: LuaSkillsCapabilityBooleanKey, value: boolean) => {
    setDraft((current) => ({
      ...current,
      capabilities: { ...current.capabilities, [field]: value },
    }))
    setFeedback(null)
  }, [])

  // updateDefaultEncoding updates the optional managed-IO text encoding.
  // updateDefaultEncoding 更新可选的托管 IO 文本编码。
  const updateDefaultEncoding = React.useCallback((value: string) => {
    setDraft((current) => ({
      ...current,
      capabilities: {
        ...current.capabilities,
        defaultTextEncoding: value || null,
      },
    }))
    setFeedback(null)
  }, [])

  // updateManagedRuntimeSetting updates one required managed-runtime numeric field.
  // updateManagedRuntimeSetting 更新一个必填的受管运行时数值字段。
  const updateManagedRuntimeSetting = React.useCallback((
    field: LuaSkillsManagedRuntimeNumberKey,
    value: number,
  ) => {
    setDraft((current) => ({
      ...current,
      managedRuntime: { ...current.managedRuntime, [field]: value },
    }))
    setFeedback(null)
  }, [])

  // updateManagedRuntimeInvokeTimeout updates the optional default invoke timeout.
  // updateManagedRuntimeInvokeTimeout 更新可选的默认调用超时。
  const updateManagedRuntimeInvokeTimeout = React.useCallback((value: number | null) => {
    setDraft((current) => ({
      ...current,
      managedRuntime: { ...current.managedRuntime, invokeDefaultTimeoutMs: value },
    }))
    setFeedback(null)
  }, [])

  // saveSettings validates and commits the complete Vulcan-owned settings document.
  // saveSettings 校验并提交完整的 Vulcan 自有设置文档。
  const saveSettings = React.useCallback(async () => {
    const validationError = runtimeSettingsValidationError(draft, t)
    if (validationError) {
      setError(validationError)
      return
    }
    setSaving(true)
    setError(null)
    setFeedback(null)
    try {
      await props.actions.onSaveLuaSkillsRuntimeSettings(draft)
      setFeedback(t("skills.luaSettings.saved"))
    } catch (caughtError) {
      setError(caughtError instanceof Error ? caughtError.message : String(caughtError))
    } finally {
      setSaving(false)
    }
  }, [draft, props.actions, t])

  // retryManagedRuntimes executes the service-owned bootstrap and lets the returned catalog carry failures.
  // retryManagedRuntimes 执行服务持有的初始化，并由返回目录承载失败结果。
  const retryManagedRuntimes = React.useCallback(async (target: LuaSkillsManagedRuntimeTarget) => {
    setRetryingTarget(target)
    setError(null)
    setFeedback(null)
    try {
      await props.actions.onRetryLuaSkillsManagedRuntimes(target)
      setFeedback(t("skills.luaSettings.managedRuntimeRetryCompleted"))
    } catch (caughtError) {
      setError(caughtError instanceof Error ? caughtError.message : String(caughtError))
    } finally {
      setRetryingTarget(null)
    }
  }, [props.actions, t])

  return (
    <section className="skill-page-body lua-settings-page-body">
      <header className="agent-catalog-toolbar lua-settings-toolbar">
        <div className="agent-catalog-toolbar-heading">
          <div>
            <h3>{t("skills.navigation.luaSettings")}</h3>
            <p>{t("skills.luaSettings.description")}</p>
          </div>
          {props.catalog.restartRequired ? (
            <span className="lua-settings-restart-badge">{t("skills.luaSettings.restartRequired")}</span>
          ) : null}
        </div>
      </header>

      <div className="lua-settings-content">
        <div className="lua-managed-runtime-list">
          {props.catalog.managedPythonRuntime && props.catalog.managedNodeRuntime ? (
            <>
              <ManagedRuntimeRow
                label={t("skills.luaSettings.pythonRuntime")}
                runtime={props.catalog.managedPythonRuntime}
                progress={pythonRuntimeProgress}
                retrying={retryingTarget === "python" || pythonRuntimeProgress != null}
                actionDisabled={managedRuntimeActionDisabled}
                onRetry={() => void retryManagedRuntimes("python")}
              />
              <ManagedRuntimeRow
                label={t("skills.luaSettings.nodeRuntime")}
                runtime={props.catalog.managedNodeRuntime}
                progress={nodeRuntimeProgress}
                retrying={retryingTarget === "node" || nodeRuntimeProgress != null}
                actionDisabled={managedRuntimeActionDisabled}
                onRetry={() => void retryManagedRuntimes("node")}
              />
            </>
          ) : (
            <CombinedManagedRuntimeRow status={props.catalog.managedRuntimeBootstrap.status} />
          )}
        </div>

        <PoolSettingsSection
          title={t("skills.luaSettings.mainPool")}
          settings={draft.vmPool}
          onChange={(field, value) => updatePoolSetting("vmPool", field, value)}
        />
        <PoolSettingsSection
          title={t("skills.luaSettings.runLuaPool")}
          settings={draft.runLuaPool}
          onChange={(field, value) => updatePoolSetting("runLuaPool", field, value)}
        />

        <section className="lua-settings-section">
          <h4>{t("skills.luaSettings.toolCache")}</h4>
          <div className="lua-settings-grid">
            <NumberSettingField
              label={t("skills.luaSettings.maxEntries")}
              value={draft.toolCache.maxEntries}
              onChange={(value) => updateToolCacheSetting("maxEntries", value)}
            />
            <NumberSettingField
              label={t("skills.luaSettings.defaultTtl")}
              value={draft.toolCache.defaultTtlSecs}
              onChange={(value) => updateToolCacheSetting("defaultTtlSecs", value)}
            />
            <NumberSettingField
              label={t("skills.luaSettings.maxTtl")}
              value={draft.toolCache.maxTtlSecs}
              onChange={(value) => updateToolCacheSetting("maxTtlSecs", value)}
            />
          </div>
        </section>

        <section className="lua-settings-section">
          <h4>{t("skills.luaSettings.network")}</h4>
          <ToggleSettingField
            label={t("skills.luaSettings.allowDownloads")}
            checked={draft.network.allowDownloads}
            onChange={(checked) => updateNetworkBoolean("allowDownloads", checked)}
          />
          <div className="lua-settings-grid">
            <TextSettingField
              label={t("skills.luaSettings.githubBaseUrl")}
              value={draft.network.githubBaseUrl ?? ""}
              onChange={(value) => updateNetworkText("githubBaseUrl", value)}
            />
            <TextSettingField
              label={t("skills.luaSettings.githubApiBaseUrl")}
              value={draft.network.githubApiBaseUrl ?? ""}
              onChange={(value) => updateNetworkText("githubApiBaseUrl", value)}
            />
            <TextSettingField
              label={t("skills.luaSettings.skillHubBaseUrl")}
              value={draft.network.officialSkillHubBaseUrl ?? ""}
              onChange={(value) => updateNetworkText("officialSkillHubBaseUrl", value)}
            />
          </div>
          <ToggleSettingField
            label={t("skills.luaSettings.privateUrlInstall")}
            checked={draft.network.enablePrivateUrlSkillInstall}
            onChange={(checked) => updateNetworkBoolean("enablePrivateUrlSkillInstall", checked)}
          />
          <label className="lua-settings-field wide">
            <span>{t("skills.luaSettings.privateAllowlist")}</span>
            <textarea
              value={draft.network.privateSkillSourceAllowlist.join("\n")}
              onChange={(event) => updateNetworkAllowlist(event.currentTarget.value)}
            />
            <small>{t("skills.luaSettings.privateAllowlistHint")}</small>
          </label>
        </section>

        <section className="lua-settings-section">
          <h4>{t("skills.luaSettings.managedRuntime")}</h4>
          <p>{t("skills.luaSettings.managedRuntimeHint")}</p>
          <div className="lua-settings-grid">
            <NumberSettingField
              label={t("skills.luaSettings.managedWorkerPoolMax")}
              value={draft.managedRuntime.workerPoolMaxSizePerEnvironment}
              onChange={(value) => updateManagedRuntimeSetting("workerPoolMaxSizePerEnvironment", value)}
            />
            <NumberSettingField
              label={t("skills.luaSettings.managedWorkerIdleTtl")}
              value={draft.managedRuntime.workerIdleTtlSecs}
              onChange={(value) => updateManagedRuntimeSetting("workerIdleTtlSecs", value)}
            />
            <NumberSettingField
              label={t("skills.luaSettings.managedSessionLimit")}
              value={draft.managedRuntime.persistentSessionLimitPerEngine}
              onChange={(value) => updateManagedRuntimeSetting("persistentSessionLimitPerEngine", value)}
            />
            <NumberSettingField
              label={t("skills.luaSettings.managedSessionBuffer")}
              value={draft.managedRuntime.persistentSessionDefaultBufferLimitBytesPerStream}
              onChange={(value) => updateManagedRuntimeSetting(
                "persistentSessionDefaultBufferLimitBytesPerStream",
                value,
              )}
            />
            <OptionalNumberSettingField
              label={t("skills.luaSettings.managedInvokeTimeout")}
              value={draft.managedRuntime.invokeDefaultTimeoutMs ?? null}
              onChange={updateManagedRuntimeInvokeTimeout}
              placeholder={t("skills.luaSettings.unlimited")}
            />
          </div>
        </section>

        <section className="lua-settings-section">
          <h4>{t("skills.luaSettings.capabilities")}</h4>
          <label className="lua-settings-field">
            <span>{t("skills.luaSettings.defaultEncoding")}</span>
            <select
              value={draft.capabilities.defaultTextEncoding ?? ""}
              onChange={(event) => updateDefaultEncoding(event.currentTarget.value)}
            >
              <option value="">{t("skills.luaSettings.autoEncoding")}</option>
              <option value="utf-8">UTF-8</option>
              <option value="system">System</option>
              <option value="oem">OEM</option>
              <option value="gbk">GBK</option>
              <option value="gb18030">GB18030</option>
              <option value="latin1">Latin-1</option>
              <option value="base64">Base64</option>
            </select>
          </label>
          <ToggleSettingField
            label={t("skills.luaSettings.skillManagementBridge")}
            checked={draft.capabilities.enableSkillManagementBridge}
            onChange={(checked) => updateCapabilityBoolean("enableSkillManagementBridge", checked)}
          />
          <ToggleSettingField
            label={t("skills.luaSettings.managedIoCompat")}
            checked={draft.capabilities.enableManagedIoCompat}
            onChange={(checked) => updateCapabilityBoolean("enableManagedIoCompat", checked)}
          />
        </section>

        <aside className="lua-settings-restart-note">{t("skills.luaSettings.restartHint")}</aside>
        {error ? <div className="lua-settings-feedback error">{error}</div> : null}
        {feedback ? <div className="lua-settings-feedback success">{feedback}</div> : null}
        <div className="lua-settings-actions">
          <button
            className="skill-action-button"
            type="button"
            disabled={saving || !dirty}
            onClick={() => void saveSettings()}
          >
            {saving ? t("skills.luaSettings.saving") : t("skills.luaSettings.save")}
          </button>
        </div>
      </div>
    </section>
  )
}

// PoolSettingsSection renders one exact Lua VM pool settings group.
// PoolSettingsSection 渲染一个精确的 Lua VM 池设置组。
function PoolSettingsSection(props: {
  // Localized section title.
  // 本地化分区标题。
  title: string
  // Current editable pool settings.
  // 当前可编辑池设置。
  settings: LuaVmPoolSettings
  // Updates one exact numeric pool field.
  // 更新一个精确的池数值字段。
  onChange: (field: LuaVmPoolNumberKey, value: number) => void
}) {
  // t resolves the shared pool field labels.
  // t 解析共享池字段标签。
  const { t } = useI18n()
  return (
    <section className="lua-settings-section">
      <h4>{props.title}</h4>
      <p>{t("skills.luaSettings.poolHint")}</p>
      <div className="lua-settings-grid">
        <NumberSettingField
          label={t("skills.luaSettings.minSize")}
          value={props.settings.minSize}
          onChange={(value) => props.onChange("minSize", value)}
        />
        <NumberSettingField
          label={t("skills.luaSettings.maxSize")}
          value={props.settings.maxSize}
          onChange={(value) => props.onChange("maxSize", value)}
        />
        <NumberSettingField
          label={t("skills.luaSettings.idleTtl")}
          value={props.settings.idleTtlSecs}
          onChange={(value) => props.onChange("idleTtlSecs", value)}
        />
      </div>
    </section>
  )
}

// NumberSettingField renders one positive integer startup setting.
// NumberSettingField 渲染一个正整数启动设置。
function NumberSettingField(props: {
  // Localized field label.
  // 本地化字段标签。
  label: string
  // Current numeric value.
  // 当前数值。
  value: number
  // Commits the next numeric draft value.
  // 提交下一份数值草稿。
  onChange: (value: number) => void
}) {
  return (
    <label className="lua-settings-field">
      <span>{props.label}</span>
      <input
        type="number"
        min={1}
        step={1}
        value={props.value}
        onChange={(event) => props.onChange(Number(event.currentTarget.value))}
      />
    </label>
  )
}

// OptionalNumberSettingField renders one optional positive integer startup setting.
// OptionalNumberSettingField 渲染一个可选的正整数启动设置。
function OptionalNumberSettingField(props: {
  // Localized field label.
  // 本地化字段标签。
  label: string
  // Current numeric value or null when the runtime default is unlimited.
  // 当前数值；运行时默认无限制时为 null。
  value: number | null
  // Localized placeholder displayed for the unlimited state.
  // 无限状态下显示的本地化占位文本。
  placeholder: string
  // Commits the next optional numeric draft value.
  // 提交下一份可选数值草稿。
  onChange: (value: number | null) => void
}) {
  return (
    <label className="lua-settings-field">
      <span>{props.label}</span>
      <input
        type="number"
        min={1}
        step={1}
        placeholder={props.placeholder}
        value={props.value ?? ""}
        onChange={(event) => {
          const nextValue = event.currentTarget.value
          props.onChange(nextValue === "" ? null : Number(nextValue))
        }}
      />
    </label>
  )
}

// TextSettingField renders one optional package startup text value.
// TextSettingField 渲染一个可选的包启动文本值。
function TextSettingField(props: {
  // Localized field label.
  // 本地化字段标签。
  label: string
  // Current text value.
  // 当前文本值。
  value: string
  // Commits the next text draft value.
  // 提交下一份文本草稿。
  onChange: (value: string) => void
}) {
  return (
    <label className="lua-settings-field">
      <span>{props.label}</span>
      <input
        type="text"
        value={props.value}
        onChange={(event) => props.onChange(event.currentTarget.value)}
      />
    </label>
  )
}

// ToggleSettingField renders one package startup Boolean setting.
// ToggleSettingField 渲染一个包启动布尔设置。
function ToggleSettingField(props: {
  // Localized toggle label.
  // 本地化开关标签。
  label: string
  // Current Boolean value.
  // 当前布尔值。
  checked: boolean
  // Commits the next Boolean draft value.
  // 提交下一份布尔草稿。
  onChange: (checked: boolean) => void
}) {
  return (
    <label className="lua-settings-toggle">
      <input
        type="checkbox"
        checked={props.checked}
        onChange={(event) => props.onChange(event.currentTarget.checked)}
      />
      <span>{props.label}</span>
    </label>
  )
}
