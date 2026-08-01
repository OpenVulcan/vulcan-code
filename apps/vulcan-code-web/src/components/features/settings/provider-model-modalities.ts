import type {
  PublicAiModel,
  PublicAiOperation,
  PublicAiResourceKind,
} from "@/types"

// ProviderModelModality identifies one user-visible input or output modality.
// ProviderModelModality 标识一种面向用户展示的输入或输出模态。
export type ProviderModelModality = PublicAiResourceKind | "text" | "embedding" | "rerank"

// ProviderModelModalities carries the exact effective input and output modality sets for one route.
// ProviderModelModalities 承载一个精确路由的有效输入与输出模态集合。
export interface ProviderModelModalities {
  // Ordered input modalities accepted by the route.
  // 路由接受的有序输入模态。
  input: ProviderModelModality[]
  // Ordered output modalities emitted by the route.
  // 路由输出的有序模态。
  output: ProviderModelModality[]
}

const MODALITY_ORDER: ProviderModelModality[] = [
  "text",
  "image",
  "audio",
  "video",
  "file",
  "embedding",
  "rerank",
]

// providerModelModalities derives display data only from the route's declared capability contract.
// providerModelModalities 仅从路由声明的能力契约派生展示数据。
export function providerModelModalities(model: PublicAiModel): ProviderModelModalities {
  const operations = new Set<PublicAiOperation>(model.capabilities.operations ?? [])
  const input = new Set<ProviderModelModality>()
  const output = new Set<ProviderModelModality>()
  if (operations.size > 0) {
    input.add("text")
  }
  for (const resource of model.capabilities.input_resources ?? []) {
    input.add(resource)
  }
  if (operations.has("conversation_respond")) {
    output.add("text")
  }
  if (operations.has("embedding_create") || model.capabilities.embedding) {
    output.add("embedding")
  }
  if (operations.has("rerank_documents") || model.capabilities.rerank) {
    output.add("rerank")
  }
  for (const resource of model.capabilities.output_resources ?? []) {
    output.add(resource)
  }
  return {
    input: MODALITY_ORDER.filter((modality) => input.has(modality)),
    output: MODALITY_ORDER.filter((modality) => output.has(modality)),
  }
}
