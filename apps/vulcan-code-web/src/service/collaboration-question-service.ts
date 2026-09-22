import {
  hasServiceRuntime,
  serviceClient,
} from "@/service/webui-service-runtime";

// CollaborationQuestionCancelResponse is the host-owned settlement receipt returned to the WebUI.
// CollaborationQuestionCancelResponse 是宿主返回给 WebUI 的问题结算回执。
export interface CollaborationQuestionCancelResponse {
  // status identifies a successfully accepted or idempotently replayed request.
  // status 标识一次成功接纳或幂等重放的请求。
  status: string;
  // question_id identifies the exact collaboration question whose authoritative state was read.
  // question_id 标识读取了权威状态的精确协作问题。
  question_id: string;
  // question_state is the durable state after the user action.
  // question_state 是用户动作后的可靠问题状态。
  question_state: "pending" | "answered" | "cancelled";
  // operation_id is the stable user request identity used for retry and audit correlation.
  // operation_id 是用于重试和审计关联的稳定用户请求身份。
  operation_id: string;
  // revision is the source document revision observed at the durable boundary.
  // revision 是可靠边界处观察到的来源文档修订。
  revision: number;
  // delivery and answer retain the same stable DTO shape used by collaboration tools.
  // delivery 与 answer 保留协作工具使用的同一稳定 DTO 形状。
  delivery?: unknown;
  answer?: unknown;
}

// CollaborationQuestionCancelOperationPrefix namespaces user cancellation from model operations.
// CollaborationQuestionCancelOperationPrefix 将用户撤销与模型操作命名空间隔离。
export const CollaborationQuestionCancelOperationPrefix = "user-question-cancel:v1:";

// pendingQuestionCancelOperationIds retains uncertain user requests until the host confirms them.
// pendingQuestionCancelOperationIds 在宿主确认前保留响应不确定的用户请求。
const pendingQuestionCancelOperationIds = new Map<string, string>();

// createCollaborationQuestionCancelOperationId creates one identity for the first click only.
// createCollaborationQuestionCancelOperationId 只在首次点击时创建一个身份。
//
// The caller must retain this value and reuse it when the same network request is retried.
// 调用方必须保留此值，并在同一次网络请求重试时复用它。
export function createCollaborationQuestionCancelOperationId(): string {
  return `${CollaborationQuestionCancelOperationPrefix}${crypto.randomUUID()}`;
}

// cancelCollaborationQuestion asks the host to cancel one exact pending collaboration question.
// cancelCollaborationQuestion 请求宿主撤销一条精确的待处理协作问题。
//
// The service owns authorization and settlement. This wrapper only carries the user operation
// identity and never mutates a local question projection optimistically.
// 服务负责授权和结算。本封装只携带用户操作身份，不乐观修改本地问题投影。
export async function cancelCollaborationQuestion(
  questionId: string,
  sourceRootSessionId: string,
  operationId?: string,
  reason = "user_cancelled",
): Promise<CollaborationQuestionCancelResponse> {
  if (!hasServiceRuntime()) {
    throw new Error("Collaboration question cancellation requires an available service runtime.");
  }
  if (!sourceRootSessionId.trim()) {
    throw new Error("Collaboration question cancellation requires the source root session identity.");
  }
  // An explicit id is used by callers that persist the request outside this module; otherwise the
  // module retains one id per question so a UI retry cannot create a second cancellation intent.
  // 调用方可传入自己持久化的 ID；否则模块按问题保留一个 ID，避免界面重试产生第二个撤销意图。
  const requestOperationId = operationId
    ?? pendingQuestionCancelOperationIds.get(questionId)
    ?? createCollaborationQuestionCancelOperationId();
  pendingQuestionCancelOperationIds.set(questionId, requestOperationId);
  try {
    const response = await serviceClient.request<CollaborationQuestionCancelResponse>(
      "service_cancel_collaboration_question",
      {
        questionId,
        sourceRootSessionId,
        operationId: requestOperationId,
        reason,
      },
    );
    pendingQuestionCancelOperationIds.delete(questionId);
    return response;
  } catch (error) {
    // Keep the exact id after transport or service uncertainty so the next click is a true retry.
    // 传输或服务结果不确定时保留原 ID，确保下次点击是真正重试。
    throw error;
  }
}
