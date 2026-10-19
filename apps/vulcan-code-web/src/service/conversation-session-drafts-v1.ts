import type { ServiceClient } from "@/service/client";
import { parseComposerPromptSource } from "@/service/composer-attachment-reference";
import {
  parseSessionResourceSummaryV1,
  sessionResourceClientV1,
  type SessionResourceRpcV1,
  type SessionResourceSummaryV1,
} from "@/service/conversation-session-resources-v1";
import { hasServiceRuntime, serviceClient } from "@/service/webui-service-runtime";
import type { ComposerAttachmentSummary } from "@/types";

/** One authoritative materialized-session draft returned by the V1 service.
 * V1 服务返回的一份权威已物化会话草稿。
 */
export interface SessionDraftV1 {
  sessionId: string;
  text: string;
  attachments: ComposerAttachmentSummary[];
}

/** Minimal transport required by the authoritative V1 draft client.
 * 权威 V1 草稿客户端所需的最小传输接口。
 */
export interface SessionDraftRpcV1 {
  /** Reads the current durable draft for one exact session.
   * 读取一个精确会话的当前可靠草稿。
   */
  read(sessionId: string): Promise<SessionDraftV1>;
  /** Replaces and returns the current durable draft for one exact session.
   * 替换并返回一个精确会话的当前可靠草稿。
   */
  write(
    sessionId: string,
    text: string,
    attachments: readonly ComposerAttachmentSummary[],
  ): Promise<SessionDraftV1>;
}

/** V1 draft client over the existing strict service facade.
 * 基于既有严格服务门面的 V1 草稿客户端。
 */
export class ServiceSessionDraftClientV1 implements SessionDraftRpcV1 {
  /** Creates one draft client with an exact resource-content owner.
   * 使用精确资源内容 owner 创建一个草稿客户端。
   */
  constructor(
    private readonly service: Pick<ServiceClient, "request">,
    private readonly resources: SessionResourceRpcV1,
  ) {}

  /** Reads and validates one authoritative draft response.
   * 读取并校验一份权威草稿响应。
   */
  async read(sessionId: string): Promise<SessionDraftV1> {
    validateSessionId(sessionId);
    const payload = await this.service.request<unknown>("service_session_draft_v1", {
      sessionId,
    });
    return parseSessionDraftV1(payload, sessionId, this.resources);
  }

  /** Replaces one authoritative draft and returns the committed canonical projection.
   * 替换一份权威草稿，并返回已提交的规范投影。
   */
  async write(
    sessionId: string,
    text: string,
    attachments: readonly ComposerAttachmentSummary[],
  ): Promise<SessionDraftV1> {
    validateSessionId(sessionId);
    const payload = await this.service.request<unknown>("service_set_session_draft_v1", {
      sessionId,
      text,
      attachments,
    });
    return parseSessionDraftV1(payload, sessionId, this.resources);
  }
}

/** Page-local draft client for the browser preview, which has no service to own drafts.
 * 浏览器预览使用的页面内草稿客户端；预览没有持有草稿的服务。
 */
export class PreviewSessionDraftClientV1 implements SessionDraftRpcV1 {
  private readonly drafts = new Map<string, SessionDraftV1>();

  /** Returns the page-local draft, or an empty one before the first write.
   * 返回页面内草稿；首次写入前返回空草稿。
   */
  async read(sessionId: string): Promise<SessionDraftV1> {
    validateSessionId(sessionId);
    const draft = this.drafts.get(sessionId);
    return draft ? { ...draft, attachments: [...draft.attachments] } : { sessionId, text: "", attachments: [] };
  }

  /** Keeps the draft for the lifetime of the page.
   * 在页面生命周期内保留草稿。
   */
  async write(
    sessionId: string,
    text: string,
    attachments: readonly ComposerAttachmentSummary[],
  ): Promise<SessionDraftV1> {
    validateSessionId(sessionId);
    const draft = { sessionId, text, attachments: [...attachments] };
    this.drafts.set(sessionId, draft);
    return { ...draft, attachments: [...attachments] };
  }
}

/** Shared draft client: the process-wide service facade, or page-local drafts in the browser preview.
 * 共享草稿客户端：使用进程级服务门面；浏览器预览中使用页面内草稿。
 */
export const sessionDraftClientV1: SessionDraftRpcV1 = (() => {
  const service = new ServiceSessionDraftClientV1(serviceClient, sessionResourceClientV1);
  const preview = new PreviewSessionDraftClientV1();
  // The runtime is checked per call so importing this module needs no browser globals.
  // 每次调用时检查运行时，使导入本模块无需浏览器全局对象。
  const pick = (): SessionDraftRpcV1 => (hasServiceRuntime() ? service : preview);
  return {
    read: (sessionId) => pick().read(sessionId),
    write: (sessionId, text, attachments) => pick().write(sessionId, text, attachments),
  };
})();

/** Parses one strict response and rebuilds browser attachments from logical project resources.
 * 解析一份严格响应，并从逻辑项目资源重建浏览器附件。
 */
export function parseSessionDraftV1(
  payload: unknown,
  expectedSessionId: string,
  resources: SessionResourceRpcV1,
): SessionDraftV1 {
  const record = exactObject(payload, ["sessionId", "text", "resources", "stagedAttachmentIds", "resourceSources"], "session draft response");
  if (record.sessionId !== expectedSessionId) {
    throw new Error("Session draft response owner does not match the request.");
  }
  if (typeof record.text !== "string" || !Array.isArray(record.resources)) {
    throw new Error("Session draft response has an invalid text or resource list.");
  }
  // The service owns the exact occurrence mapping; a file hash or last-upload cache cannot replace it.
  // 服务持有精确出现映射；文件摘要或最近上传缓存不能替代它。
  const stagedAttachmentIds = record.stagedAttachmentIds;
  if (!Array.isArray(stagedAttachmentIds) || stagedAttachmentIds.length !== record.resources.length
    || stagedAttachmentIds.some((id: unknown) => id !== null && (typeof id !== "string" || id.length === 0 || id.trim() !== id || id.length > 1024 || /[\u0000-\u001f\u007f-\u009f/\\:]/u.test(id)))) {
    throw new Error("Session draft response has invalid staged attachment identities.");
  }
  // Original input positions are explicit service facts, never derived from repeated ResourceIds.
  // 原输入位置是显式服务事实，绝不从重复ResourceId推导。
  if (!Array.isArray(record.resourceSources) || record.resourceSources.length !== record.resources.length) {
    throw new Error("Session draft response has invalid resource sources.");
  }
  // Validate every source before publishing any partially parsed attachment array.
  // 发布任何部分解析附件数组前，校验全部来源。
  const resourceSources = record.resourceSources.map(parseComposerPromptSource);
  return {
    sessionId: expectedSessionId,
    text: record.text,
    attachments: record.resources.map((value, index) => {
      // Parse resource metadata before assigning its explicit staged occurrence identity.
      // 分配其显式暂存出现身份之前，解析资源元数据。
      const resource = parseSessionResourceSummaryV1(value);
      // Null deliberately denotes a project-only reference, including legacy drafts.
      // 空值明确表示纯项目引用，包括旧草稿。
      const stagedId = stagedAttachmentIds[index] as string | null;
      // Staged assets and project-input occurrences are disjoint ownership forms.
      // 暂存资产与项目输入出现是互斥所有权形式。
      const promptSource = resourceSources[index];
      if (stagedId !== null && promptSource !== null) {
        throw new Error("Session draft attachment has conflicting staged and prompt source identities.");
      }
      if (stagedId !== null && resource.availability === "missing") {
        throw new Error("Session draft staged attachment has no authoritative resource metadata.");
      }
      // Preview URLs remain project-scoped; future mutations resolve the exact managed asset id at the host.
      // 预览URL保持项目作用域；后续修改在宿主解析精确托管资产ID。
      const attachment = attachmentFromResourceSummary(
        expectedSessionId,
        resource,
        resources,
      );
      if (stagedId !== null) {
        delete attachment.projectResourceId;
        attachment.assetId = stagedId;
      } else if (promptSource !== null) {
        attachment.promptSource = promptSource;
      }
      return attachment;
    }),
  };
}

/** Rebuilds one displayable attachment from browser-safe project-resource metadata.
 * 从浏览器安全项目资源元数据重建一条可显示附件。
 */
function attachmentFromResourceSummary(
  sessionId: string,
  resource: SessionResourceSummaryV1,
  resources: SessionResourceRpcV1,
): ComposerAttachmentSummary {
  if (resource.availability === "missing") {
    return {
      kind: "file",
      fileName: resource.placeholder,
      localPath: resources.contentUrl(sessionId, resource.resource_id),
      projectResourceId: resource.resource_id,
      byteLen: 0,
      mimeType: "application/octet-stream",
    };
  }
  const mimeType = resource.mime ?? "application/octet-stream";
  return {
    kind: attachmentKindFromMime(mimeType),
    fileName: resource.original_name,
    localPath: resources.contentUrl(sessionId, resource.resource_id),
    projectResourceId: resource.resource_id,
    byteLen: resource.size,
    mimeType,
  };
}

/** Classifies one authoritative MIME type into the stable composer attachment kind.
 * 把一个权威 MIME 类型分类为稳定输入区附件类型。
 */
function attachmentKindFromMime(mimeType: string): string {
  if (mimeType === "application/pdf") {
    return "pdf";
  }
  if (mimeType.startsWith("image/")) {
    return "image";
  }
  if (mimeType.startsWith("audio/")) {
    return "audio";
  }
  if (mimeType.startsWith("video/")) {
    return "video";
  }
  return "file";
}

/** Validates one exact materialized-session identity.
 * 校验一个精确已物化会话身份。
 */
function validateSessionId(sessionId: string): void {
  if (sessionId.length === 0 || sessionId.trim() !== sessionId) {
    throw new Error("Session draft requires an exact non-empty session id.");
  }
}

/** Parses an exact object shape without compatibility fields.
 * 解析一份不含兼容字段的精确对象结构。
 */
function exactObject(
  value: unknown,
  requiredKeys: readonly string[],
  label: string,
): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object.`);
  }
  const record = value as Record<string, unknown>;
  const required = new Set(requiredKeys);
  for (const key of Object.keys(record)) {
    if (!required.has(key)) {
      throw new Error(`${label} contains unknown field '${key}'.`);
    }
  }
  for (const key of requiredKeys) {
    if (!(key in record)) {
      throw new Error(`${label} is missing field '${key}'.`);
    }
  }
  return record;
}
