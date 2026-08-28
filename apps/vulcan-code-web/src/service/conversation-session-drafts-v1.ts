import type { ServiceClient } from "@/service/client";
import {
  parseSessionResourceSummaryV1,
  sessionResourceClientV1,
  type SessionResourceRpcV1,
  type SessionResourceSummaryV1,
} from "@/service/conversation-session-resources-v1";
import { serviceClient } from "@/service/webui-service-runtime";
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

/** Shared production draft client over the process-wide service facade.
 * 基于进程级服务门面的共享生产草稿客户端。
 */
export const sessionDraftClientV1 = new ServiceSessionDraftClientV1(
  serviceClient,
  sessionResourceClientV1,
);

/** Parses one strict response and rebuilds browser attachments from logical project resources.
 * 解析一份严格响应，并从逻辑项目资源重建浏览器附件。
 */
export function parseSessionDraftV1(
  payload: unknown,
  expectedSessionId: string,
  resources: SessionResourceRpcV1,
): SessionDraftV1 {
  const record = exactObject(payload, ["sessionId", "text", "resources"], "session draft response");
  if (record.sessionId !== expectedSessionId) {
    throw new Error("Session draft response owner does not match the request.");
  }
  if (typeof record.text !== "string" || !Array.isArray(record.resources)) {
    throw new Error("Session draft response has an invalid text or resource list.");
  }
  return {
    sessionId: expectedSessionId,
    text: record.text,
    attachments: record.resources.map((value) =>
      attachmentFromResourceSummary(
        expectedSessionId,
        parseSessionResourceSummaryV1(value),
        resources,
      )),
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
