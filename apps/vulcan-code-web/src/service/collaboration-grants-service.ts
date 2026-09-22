import {
  hasServiceRuntime,
  serviceClient,
} from "@/service/webui-service-runtime";

// CollaborationGrantAction is the exact user-authorized capability set persisted by the V1 host.
// CollaborationGrantAction 是 V1 宿主持久化的精确用户授权能力集合。
export type CollaborationGrantAction = "read" | "ask" | "steer" | "start" | "reply" | "notify";

// CollaborationGrantRecord is the source-owned durable grant returned by the service.
// CollaborationGrantRecord 是服务返回的来源会话拥有的可靠授权记录。
export interface CollaborationGrantRecord {
  // grant_id identifies the durable authorization and its revocation history.
  // grant_id 标识可靠授权及其撤销历史。
  grant_id: string;
  // source_session_id is the only session whose metadata owns this grant.
  // source_session_id 是唯一持有此授权 metadata 的会话。
  source_session_id: string;
  // target_session_id identifies the exact session allowed by this grant.
  // target_session_id 标识该授权允许访问的精确目标会话。
  target_session_id: string;
  // actions contains only the capabilities explicitly selected by the user.
  // actions 只包含用户明确选择的能力。
  actions: CollaborationGrantAction[];
  // authority identifies the host-owned user authority that created the record.
  // authority 标识创建记录的宿主持有用户权威。
  authority: "local_user";
  // created_revision identifies the source metadata revision that created the grant.
  // created_revision 标识创建授权的来源 metadata 修订。
  created_revision: number;
  // created_at_ms is the source-owned creation timestamp.
  // created_at_ms 是来源拥有的创建时间戳。
  created_at_ms: number;
  // revoked_at_ms is present only after explicit user revocation.
  // revoked_at_ms 仅在用户明确撤销后存在。
  revoked_at_ms?: number;
  // revoked_revision identifies the source metadata revision that revoked the grant.
  // revoked_revision 标识撤销授权的来源 metadata 修订。
  revoked_revision?: number;
}

// CollaborationGrantListResponse is the bounded user-facing grant query result.
// CollaborationGrantListResponse 是有界的用户授权查询结果。
export interface CollaborationGrantListResponse {
  // status is the stable service result marker.
  // status 是稳定的服务结果标记。
  status: "ok";
  // sourceSessionId identifies the sole durable grant owner.
  // sourceSessionId 标识唯一可靠授权 owner。
  sourceSessionId: string;
  // grants contains currently effective grants for this source session.
  // grants 包含该来源会话当前有效的授权。
  grants: CollaborationGrantRecord[];
}

// CollaborationGrantMutationResponse describes one committed or idempotent grant mutation.
// CollaborationGrantMutationResponse 描述一次已提交或幂等的授权修改。
export interface CollaborationGrantMutationResponse {
  // status is the stable service result marker.
  // status 是稳定的服务结果标记。
  status: "ok";
  // changed distinguishes a new durable mutation from an exact retry.
  // changed 区分新的可靠修改与完全重试。
  changed: boolean;
  // grant is the complete source-owned after-state.
  // grant 是完整的来源拥有后状态。
  grant: CollaborationGrantRecord;
  // sourceSessionId identifies the grant owner.
  // sourceSessionId 标识授权 owner。
  sourceSessionId: string;
  // eventSeq is the collaboration event sequence when a mutation was published.
  // eventSeq 是协作变更发布时的事件序号。
  eventSeq?: number;
  // revision is the source document revision observed at the durable boundary.
  // revision 是可靠边界处观察到的来源文档修订。
  revision?: number;
  // targetSessionId is returned for grant creation and remains optional for revocation.
  // targetSessionId 在创建授权时返回，撤销授权时可省略。
  targetSessionId?: string;
  // sourceProjectId is the host-verified source project identity.
  // sourceProjectId 是宿主校验过的来源项目身份。
  sourceProjectId?: string;
  // targetProjectId is the host-verified target project identity.
  // targetProjectId 是宿主校验过的目标项目身份。
  targetProjectId?: string;
  // sourceSegmentGeneration is the source generation checked at creation.
  // sourceSegmentGeneration 是创建时校验的来源代际。
  sourceSegmentGeneration?: number;
  // targetSegmentGeneration is the target generation checked at creation.
  // targetSegmentGeneration 是创建时校验的目标代际。
  targetSegmentGeneration?: number;
}

// CollaborationGrantMutationKind selects the host-validated user operation namespace.
// CollaborationGrantMutationKind 选择经过宿主校验的用户操作命名空间。
export type CollaborationGrantMutationKind = "add" | "remove";

// createCollaborationGrantOperationId creates one request identity at the first user click.
// createCollaborationGrantOperationId 在用户首次点击时生成一个请求标识。
export function createCollaborationGrantOperationId(kind: CollaborationGrantMutationKind): string {
  const prefix = kind === "add" ? "user-grant-add:v1:" : "user-grant-remove:v1:";
  return `${prefix}${crypto.randomUUID()}`;
}

// listCollaborationGrants reads current user grants from the exact source session.
// listCollaborationGrants 从精确来源会话读取当前用户授权。
export async function listCollaborationGrants(
  sourceSessionId: string,
): Promise<CollaborationGrantListResponse> {
  if (!hasServiceRuntime()) {
    throw new Error("Service runtime is unavailable; collaboration grants cannot be read.");
  }
  return serviceClient.request<CollaborationGrantListResponse>(
    "service_list_collaboration_grants",
    { sourceSessionId },
  );
}

// addCollaborationGrant creates one source-owned cross-session authorization through a user RPC.
// addCollaborationGrant 通过用户 RPC 创建一条来源拥有的跨会话授权。
export async function addCollaborationGrant(
  sourceSessionId: string,
  targetSessionId: string,
  actions: CollaborationGrantAction[],
  operationId: string,
): Promise<CollaborationGrantMutationResponse> {
  if (!hasServiceRuntime()) {
    throw new Error("Service runtime is unavailable; collaboration grant was not created.");
  }
  return serviceClient.request<CollaborationGrantMutationResponse>(
    "service_add_collaboration_grant",
    { sourceSessionId, targetSessionId, actions, operationId },
  );
}

// removeCollaborationGrant revokes one source-owned authorization without erasing its audit fact.
// removeCollaborationGrant 撤销一条来源拥有的授权，但不删除其审计事实。
export async function removeCollaborationGrant(
  sourceSessionId: string,
  grantId: string,
  operationId: string,
): Promise<CollaborationGrantMutationResponse> {
  if (!hasServiceRuntime()) {
    throw new Error("Service runtime is unavailable; collaboration grant was not revoked.");
  }
  return serviceClient.request<CollaborationGrantMutationResponse>(
    "service_remove_collaboration_grant",
    { sourceSessionId, grantId, operationId },
  );
}
