import type {
  PublicAiCustomModelDefinition,
  PublicAiCustomModelRequestProjection,
  PublicAiModelParameterDefinition,
} from "@/types"

// AdvancedModelTextDraft retains protocol JSON text independently for one edited model.
// AdvancedModelTextDraft 为一个被编辑模型独立保留协议 JSON 文本。
export interface AdvancedModelTextDraft {
  // parameterText is the exact unsaved model-parameter JSON array.
  // parameterText 是尚未保存的精确模型参数 JSON 数组。
  parameterText: string
  // projectionText is the exact unsaved request-projection JSON object.
  // projectionText 是尚未保存的精确请求投影 JSON 对象。
  projectionText: string
}

// AdvancedModelDraftValidationMessages carries localized controlled parse failures.
// AdvancedModelDraftValidationMessages 承载本地化且受控的解析失败文案。
export interface AdvancedModelDraftValidationMessages {
  // parametersInvalid is returned when the parameter JSON is not an array.
  // parametersInvalid 在参数 JSON 不是数组时返回。
  parametersInvalid: string
  // projectionInvalid is returned when the projection JSON is not an object.
  // projectionInvalid 在投影 JSON 不是对象时返回。
  projectionInvalid: string
}

// advancedModelTextDraftFromDefinition serializes one persisted definition for text editing.
// advancedModelTextDraftFromDefinition 将一项持久化定义序列化为文本编辑草稿。
export function advancedModelTextDraftFromDefinition(
  model: PublicAiCustomModelDefinition,
): AdvancedModelTextDraft {
  return {
    projectionText: JSON.stringify(model.request_projection, null, 2),
    parameterText: JSON.stringify(model.capabilities.model_parameters ?? [], null, 2),
  }
}

// applyAdvancedModelTextDrafts validates and applies every model-scoped draft atomically in memory.
// applyAdvancedModelTextDrafts 在内存中校验并原子应用每个模型作用域草稿。
export function applyAdvancedModelTextDrafts(
  models: PublicAiCustomModelDefinition[],
  drafts: Record<string, AdvancedModelTextDraft>,
  messages: AdvancedModelDraftValidationMessages,
): PublicAiCustomModelDefinition[] {
  return models.map((model) => {
    const draft = drafts[model.id]
    if (!draft) return model
    const projection = parseProjection(draft.projectionText, messages.projectionInvalid)
    const parameters = parseParameters(draft.parameterText, messages.parametersInvalid)
    return {
      ...model,
      request_projection: projection,
      capabilities: { ...model.capabilities, model_parameters: parameters },
    }
  })
}

// parseProjection validates one advanced request-projection JSON object.
// parseProjection 校验一项高级请求投影 JSON 对象。
function parseProjection(
  value: string,
  invalidMessage: string,
): PublicAiCustomModelRequestProjection {
  const parsed = JSON.parse(value) as unknown
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error(invalidMessage)
  }
  return parsed as PublicAiCustomModelRequestProjection
}

// parseParameters validates one advanced model-parameter JSON array.
// parseParameters 校验一项高级模型参数 JSON 数组。
function parseParameters(value: string, invalidMessage: string): PublicAiModelParameterDefinition[] {
  const parsed = JSON.parse(value) as unknown
  if (!Array.isArray(parsed)) throw new Error(invalidMessage)
  return parsed as PublicAiModelParameterDefinition[]
}
