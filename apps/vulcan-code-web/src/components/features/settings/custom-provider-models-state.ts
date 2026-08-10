import type {
  PublicAiCatalogDiscoveryStatus,
  PublicAiConnection,
  PublicAiCustomModelDefinition,
  PublicAiCustomModelProtocolPreference,
  PublicAiCustomModelTemplateDecision,
  PublicAiModel,
  PublicAiModelCapabilityOverrides,
  PublicAiSettingsSnapshot,
} from "@/types"

// CustomProviderModelAccountState describes one exact account's evidence for a model.
// CustomProviderModelAccountState 描述一个精确账号对某个模型的证据。
export interface CustomProviderModelAccountState {
  connection: PublicAiConnection
  status?: PublicAiCatalogDiscoveryStatus
  discoveredModel?: PublicAiModel
  effectiveModel?: PublicAiModel
  // Provider-resolved model before this account's access policy is applied.
  // 应用此账号访问策略前由供应商解析出的模型。
  supportModel?: PublicAiModel
  availabilityOverride?: boolean
}

// CustomProviderModelRow merges model identity while retaining every credential-scoped fact.
// CustomProviderModelRow 合并模型身份，同时保留每项凭据作用域事实。
export interface CustomProviderModelRow {
  id: string
  displayName: string
  enabled: boolean
  defaultEnabled: boolean
  manualDefinition?: PublicAiCustomModelDefinition
  effectiveModel?: PublicAiModel
  // Strongest resolved capability baseline available even when every account hides the model.
  // 即使全部账号隐藏模型仍可使用的最强已解析能力基线。
  capabilityModel?: PublicAiModel
  capabilityOverrides?: PublicAiModelCapabilityOverrides
  templateDecision?: PublicAiCustomModelTemplateDecision
  accounts: CustomProviderModelAccountState[]
}

// buildCustomProviderModelRows creates one stable provider model union without losing account provenance.
// buildCustomProviderModelRows 创建一个稳定的供应商模型并集且不丢失账号来源。
//
// Parameters
// 参数
// snapshot is the immutable browser projection and providerId selects one exact provider.
// snapshot 是不可变浏览器投影，providerId 选择一个精确供应商。
//
// Returns
// 返回值
// Returns one row per model ID with credential-specific source and effective records attached.
// 返回每个模型 ID 一行，并附带凭据特定的源记录与有效记录。
export function buildCustomProviderModelRows(
  snapshot: PublicAiSettingsSnapshot,
  providerId: string,
): CustomProviderModelRow[] {
  const providerConnections = snapshot.connections
    .filter((connection) => connection.provider_id === providerId)
    .sort((left, right) => left.display_name.localeCompare(right.display_name))
  const manualDefinitions = snapshot.customModels?.[providerId] ?? {}
  const accessByModel = new Map(
    snapshot.modelAccessRecords
      .filter((record) => record.providerId === providerId)
      .map((record) => [record.modelId, record]),
  )
  const capabilityOverridesByModel = new Map(
    (snapshot.modelCapabilityOverrideRecords ?? [])
      .filter((record) => record.providerId === providerId)
      .map((record) => [record.modelId, record.overrides]),
  )
  const templateDecisionByModel = new Map(
    (snapshot.customModelTemplateDecisionRecords ?? [])
      .filter((record) => record.providerId === providerId)
      .map((record) => [record.modelId, record.decision]),
  )
  const availabilityByConnectionModel = new Map(
    (snapshot.customModelConnectionAvailabilityRecords ?? []).map((record) => [
      connectionModelKey(record.connectionId, record.modelId),
      record,
    ]),
  )
  const modelIds = new Set(Object.keys(manualDefinitions))
  for (const connection of providerConnections) {
    for (const model of snapshot.sourceCatalogs[connection.id]?.models ?? []) {
      modelIds.add(model.id)
    }
    for (const model of snapshot.catalogs[connection.id]?.models ?? []) {
      modelIds.add(model.id)
    }
    for (const model of snapshot.modelSupportCatalogs[connection.id]?.models ?? []) {
      modelIds.add(model.id)
    }
  }

  return [...modelIds]
    .map((modelId) => {
      const accounts = providerConnections.map((connection) => {
        const sourceModel = snapshot.sourceCatalogs[connection.id]?.models.find(
          (model) => model.id === modelId,
        )
        const effectiveModel = snapshot.catalogs[connection.id]?.models.find(
          (model) => model.id === modelId,
        )
        // supportModel retains resolved inherited capabilities when account access hides the model.
        // supportModel 在账号访问隐藏模型时保留已解析的继承能力。
        const supportModel = snapshot.modelSupportCatalogs[connection.id]?.models.find(
          (model) => model.id === modelId,
        )
        return {
          connection,
          status: snapshot.catalogStatuses[connection.id],
          discoveredModel: sourceModel,
          effectiveModel,
          supportModel,
          availabilityOverride: availabilityByConnectionModel.get(
            connectionModelKey(connection.id, modelId),
          )?.enabled,
        }
      })
      const firstDiscovered = accounts.find((account) => account.discoveredModel)?.discoveredModel
      const firstEffective = accounts.find((account) => account.effectiveModel)?.effectiveModel
      const firstSupport = accounts.find((account) => account.supportModel)?.supportModel
      const manualDefinition = manualDefinitions[modelId]
      const access = accessByModel.get(modelId)
      return {
        id: modelId,
        displayName:
          manualDefinition?.display_name
          ?? firstDiscovered?.display_name
          ?? firstSupport?.display_name
          ?? firstEffective?.display_name
          ?? modelId,
        enabled: access?.enabled ?? manualDefinition?.enabled ?? false,
        defaultEnabled: access?.defaultEnabled ?? manualDefinition?.enabled ?? false,
        manualDefinition,
        effectiveModel: firstEffective,
        capabilityModel: firstEffective ?? firstSupport,
        capabilityOverrides: capabilityOverridesByModel.get(modelId),
        templateDecision: templateDecisionByModel.get(modelId),
        accounts,
      }
    })
    .sort((left, right) => left.displayName.localeCompare(right.displayName))
}

// accountActuallyDiscoveredModel reports whether the remote catalog explicitly named the model.
// accountActuallyDiscoveredModel 报告远程目录是否明确列出了该模型。
export function accountActuallyDiscoveredModel(account: CustomProviderModelAccountState): boolean {
  return account.discoveredModel !== undefined
}

// accountExposesModel reports the final account-model availability after runtime resolution.
// accountExposesModel 报告运行时解析后的最终账号模型可用性。
export function accountExposesModel(account: CustomProviderModelAccountState): boolean {
  return account.effectiveModel !== undefined
}

// accountIncludesModel reports provider-resolved account membership before the account access switch.
// accountIncludesModel 报告应用账号访问开关前由供应商解析出的账号成员关系。
export function accountIncludesModel(account: CustomProviderModelAccountState): boolean {
  return account.supportModel !== undefined || account.effectiveModel !== undefined
}

// shouldReplaceManualModelDrafts protects local advanced edits from unrelated service snapshots.
// shouldReplaceManualModelDrafts 保护本地高级编辑不被无关服务快照覆盖。
//
// Parameters
// 参数
// manualModelsDirty reports unsaved local edits and manualCommitSucceeded marks the exact write
// that persisted those edits.
// manualModelsDirty 报告未保存的本地编辑，manualCommitSucceeded 标记精确持久化这些编辑的写入。
//
// Returns
// 返回值
// Returns true only when no local edits exist or the manual-definition commit itself succeeded.
// 仅当不存在本地编辑，或手工定义提交本身成功时返回 true。
export function shouldReplaceManualModelDrafts(
  manualModelsDirty: boolean,
  manualCommitSucceeded: boolean,
): boolean {
  return !manualModelsDirty || manualCommitSucceeded
}

// shouldBlockManualModelSubmission closes the debounce window before template review completes.
// shouldBlockManualModelSubmission 在模板审查完成前关闭防抖竞态窗口。
//
// Parameters
// 参数
// saving reports an active persistence request and suggestionLoading reports an unresolved template lookup.
// saving 报告正在进行的持久化请求，suggestionLoading 报告尚未完成的模板查询。
//
// Returns
// 返回值
// Returns true whenever accepting the dialog could skip one in-flight decision or duplicate a write.
// 当接受弹窗可能跳过进行中的决策或重复写入时返回 true。
export function shouldBlockManualModelSubmission(
  saving: boolean,
  suggestionLoading: boolean,
): boolean {
  return saving || suggestionLoading
}

// mergeCustomModelProtocolPreferences builds the complete sparse replacement payload expected by
// the Service API while preserving preferences owned by every untouched model.
// mergeCustomModelProtocolPreferences 构建 Service API 期望的完整稀疏替换载荷，同时保留
// 每个未编辑模型持有的偏好。
//
// Parameters
// 参数
// current is the exact provider snapshot and updates contains the deliberate model edits.
// current 是精确供应商快照，updates 包含用户明确执行的模型编辑。
//
// Returns
// 返回值
// Returns a deterministic full-provider list; Auto is represented by absence to keep persistence sparse.
// 返回确定性的供应商完整列表；Auto 以缺省表示，使持久化保持稀疏。
export function mergeCustomModelProtocolPreferences(
  current: ReadonlyMap<string, PublicAiCustomModelProtocolPreference>,
  updates: ReadonlyArray<{
    modelId: string
    preference: PublicAiCustomModelProtocolPreference
  }>,
): { modelId: string; preference: PublicAiCustomModelProtocolPreference }[] {
  // merged owns one exact preference per model before deterministic serialization.
  // merged 在确定性序列化前持有每个模型的一项精确偏好。
  const merged = new Map(
    [...current].filter(([, preference]) => preference.mode !== "auto"),
  )
  for (const update of updates) {
    if (update.preference.mode === "auto") {
      merged.delete(update.modelId)
    } else {
      merged.set(update.modelId, update.preference)
    }
  }
  return [...merged]
    .sort(([leftModelId], [rightModelId]) => leftModelId.localeCompare(rightModelId))
    .map(([modelId, preference]) => ({ modelId, preference }))
}

// connectionModelKey builds the browser-only exact key for one availability record lookup.
// connectionModelKey 构建用于查询一项可用性记录的浏览器端精确键。
function connectionModelKey(connectionId: string, modelId: string): string {
  return `${connectionId}\u0000${modelId}`
}
