import type { ServiceClient } from "@/service/client";
import {
  resolveBrowserServiceAccessToken,
  resolveBrowserServiceBaseUrl,
} from "@/service/browser-service-discovery";
import { serviceClient } from "@/service/webui-service-runtime";
import {
  resourceId,
  type ResourceId,
} from "@/service/conversation-event-window-store";

/** Maximum resource references accepted by the Rust V1 list contract.
 * Rust V1 列表契约允许的最大资源引用数。 */
const SESSION_RESOURCE_LIST_V1_MAX_ITEMS = 256;

/** Browser-safe metadata for one available project resource.
 * 单个可用项目资源的浏览器安全元数据。 */
export interface AvailableSessionResourceV1 {
  availability: "available";
  resource_id: ResourceId;
  original_name: string;
  mime?: string;
  size: number;
}

/** Browser-safe fixed placeholder for one missing project resource.
 * 单个缺失项目资源的浏览器安全固定占位信息。 */
export interface MissingSessionResourceV1 {
  availability: "missing";
  resource_id: ResourceId;
  placeholder: string;
}

/** Exact tagged metadata projection for one project resource.
 * 单个项目资源的精确带标签元数据投影。 */
export type SessionResourceSummaryV1 =
  | AvailableSessionResourceV1
  | MissingSessionResourceV1;

/** Ordered metadata response for one V1 session owner.
 * 单个 V1 会话所有者的有序元数据响应。 */
export interface SessionResourceListV1Response {
  sessionId: string;
  resources: SessionResourceSummaryV1[];
}

/** Minimal transport required by the resource metadata client.
 * 资源元数据客户端所需的最小传输接口。 */
export interface SessionResourceRpcV1 {
  /** Resolves exact metadata for ordered project resources.
   * 解析有序项目资源的精确元数据。 */
  list(sessionId: string, resourceIds: readonly ResourceId[]): Promise<SessionResourceListV1Response>;
  /** Builds an authenticated opaque content URL for one available resource.
   * 为一个可用资源构建经过鉴权的不透明内容 URL。 */
  contentUrl(sessionId: string, currentResourceId: ResourceId): string;
}

/** Strongly typed V1 resource client over the existing service facade.
 * 基于既有服务门面的强类型 V1 资源客户端。 */
export class ServiceSessionResourceClientV1 implements SessionResourceRpcV1 {
  /** Creates one resource client over an exact RPC facade.
   * 基于一个精确 RPC 门面创建资源客户端。
   *
   * @param service transport-neutral service request facade.
   *                传输无关的服务请求门面。 */
  constructor(private readonly service: Pick<ServiceClient, "request">) {}

  /** Resolves and validates metadata in the exact request order.
   * 按精确请求顺序解析并校验元数据。
   *
   * @param sessionId exact V1 session identity.
   *                  精确 V1 会话身份。
   * @param resourceIds ordered project-scoped resource identities.
   *                    有序项目级资源身份。
   * @returns the strict browser-safe metadata response.
   *          严格的浏览器安全元数据响应。 */
  async list(
    sessionId: string,
    resourceIds: readonly ResourceId[],
  ): Promise<SessionResourceListV1Response> {
    if (sessionId.trim().length === 0 || sessionId.trim() !== sessionId) {
      throw new Error("Session resource metadata requires an exact non-empty session id.");
    }
    if (resourceIds.length > SESSION_RESOURCE_LIST_V1_MAX_ITEMS) {
      throw new Error(`Session resource metadata accepts at most ${SESSION_RESOURCE_LIST_V1_MAX_ITEMS} items.`);
    }
    const payload = await this.service.request<unknown>("service_session_resource_list_v1", {
      sessionId,
      resourceIds,
    });
    return parseSessionResourceListV1Response(payload, sessionId, resourceIds);
  }

  /** Builds one authenticated resource URL without exposing project or filesystem locators.
   * 构建一个不暴露项目或文件系统定位信息的鉴权资源 URL。
   *
   * @param sessionId exact V1 session identity.
   *                  精确 V1 会话身份。
   * @param currentResourceId exact project-scoped resource identity.
   *                          精确项目级资源身份。
   * @returns the local controlled content URL.
   *          本地受控内容 URL。 */
  contentUrl(sessionId: string, currentResourceId: ResourceId): string {
    const token = resolveBrowserServiceAccessToken();
    if (token === null) {
      throw new Error("Browser service token is unavailable for project resource content.");
    }
    const baseUrl = resolveBrowserServiceBaseUrl();
    const query = new URLSearchParams({ token });
    return `${baseUrl}/v1/session-resources/${encodeURIComponent(sessionId)}/${currentResourceId}?${query.toString()}`;
  }
}

/** Shared production resource client over the process-wide service facade.
 * 基于进程级服务门面的共享生产资源客户端。 */
export const sessionResourceClientV1 = new ServiceSessionResourceClientV1(serviceClient);

/** Parses one strict response and proves order and identity against the request.
 * 解析一份严格响应，并对照请求证明顺序与身份。
 *
 * @param payload unknown transport payload.
 *                未知传输载荷。
 * @param expectedSessionId requested session identity.
 *                          请求的会话身份。
 * @param expectedResourceIds requested ordered resource identities.
 *                            请求的有序资源身份。
 * @returns the validated response.
 *          已校验的响应。 */
export function parseSessionResourceListV1Response(
  payload: unknown,
  expectedSessionId: string,
  expectedResourceIds: readonly ResourceId[],
): SessionResourceListV1Response {
  const record = exactObject(payload, ["sessionId", "resources"], "session resource response");
  if (record.sessionId !== expectedSessionId) {
    throw new Error("Session resource response owner does not match the request.");
  }
  if (!Array.isArray(record.resources) || record.resources.length !== expectedResourceIds.length) {
    throw new Error("Session resource response length does not match the request.");
  }
  const resources = record.resources.map((entry, index) => {
    const parsed = parseSessionResourceSummaryV1(entry);
    if (parsed.resource_id !== expectedResourceIds[index]) {
      throw new Error("Session resource identity is out of request order.");
    }
    return parsed;
  });
  return { sessionId: expectedSessionId, resources };
}

/** Parses one exact tagged resource summary.
 * 解析一条精确的带标签资源摘要。
 *
 * @param value unknown summary payload.
 *              未知摘要载荷。
 * @param expectedResourceId resource identity required at this ordered position.
 *                           当前有序位置要求的资源身份。
 * @returns the validated tagged summary.
 *          已校验的带标签摘要。 */
export function parseSessionResourceSummaryV1(
  value: unknown,
): SessionResourceSummaryV1 {
  if (!isPlainObject(value) || typeof value.availability !== "string") {
    throw new Error("Session resource summary must be a tagged object.");
  }
  if (value.availability === "available") {
    const record = exactObject(
      value,
      ["availability", "resource_id", "original_name", "mime", "size"],
      "available session resource",
      ["mime"],
    );
    const parsedResourceId = resourceIdFromUnknown(record.resource_id);
    if (typeof record.original_name !== "string" || record.original_name.length === 0) {
      throw new Error("Available session resource requires an original name.");
    }
    if (record.mime !== undefined && typeof record.mime !== "string") {
      throw new Error("Available session resource MIME must be a string when present.");
    }
    if (!Number.isSafeInteger(record.size) || (record.size as number) < 0) {
      throw new Error("Available session resource size must be a non-negative safe integer.");
    }
    return {
      availability: "available",
      resource_id: parsedResourceId,
      original_name: record.original_name,
      ...(record.mime === undefined ? {} : { mime: record.mime }),
      size: record.size,
    } as AvailableSessionResourceV1;
  }
  if (value.availability === "missing") {
    const record = exactObject(
      value,
      ["availability", "resource_id", "placeholder"],
      "missing session resource",
    );
    const parsedResourceId = resourceIdFromUnknown(record.resource_id);
    if (typeof record.placeholder !== "string" || record.placeholder.length === 0) {
      throw new Error("Missing session resource requires a fixed placeholder.");
    }
    return {
      availability: "missing",
      resource_id: parsedResourceId,
      placeholder: record.placeholder,
    };
  }
  throw new Error(`Unknown session resource availability '${value.availability}'.`);
}

/** Validates one unknown number as a project ResourceId.
 * 将一个未知数字校验为项目 ResourceId。
 *
 * @param value unknown resource identity.
 *              未知资源身份。
 * @returns one branded ResourceId.
 *          一个带名义标记的 ResourceId。 */
function resourceIdFromUnknown(value: unknown): ResourceId {
  if (typeof value !== "number") {
    throw new Error("Session resource identity must be a number.");
  }
  return resourceId(value);
}

/** Returns whether one unknown payload is a non-array object.
 * 返回一个未知载荷是否为非数组对象。
 *
 * @param value unknown transport value.
 *              未知传输值。
 * @returns true only for plain object-shaped JSON values.
 *          仅对普通对象形态 JSON 值返回 true。 */
function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** Validates exact required and optional object keys without accepting compatibility fields.
 * 校验精确的必填与可选对象键，不接受兼容字段。
 *
 * @param value unknown transport value.
 *              未知传输值。
 * @param allowedKeys complete allowed key set.
 *                    完整允许键集合。
 * @param label diagnostic contract label.
 *              诊断契约标签。
 * @param optionalKeys allowed keys that may be absent.
 *                     可以缺失的允许键。
 * @returns the validated object record.
 *          已校验的对象记录。 */
function exactObject(
  value: unknown,
  allowedKeys: readonly string[],
  label: string,
  optionalKeys: readonly string[] = [],
): Record<string, unknown> {
  if (!isPlainObject(value)) {
    throw new Error(`${label} must be an object.`);
  }
  const allowed = new Set(allowedKeys);
  const optional = new Set(optionalKeys);
  const keys = Object.keys(value);
  const unknown = keys.find((key) => !allowed.has(key));
  if (unknown !== undefined) {
    throw new Error(`${label} contains unknown field '${unknown}'.`);
  }
  const missing = allowedKeys.find((key) => !optional.has(key) && !(key in value));
  if (missing !== undefined) {
    throw new Error(`${label} is missing field '${missing}'.`);
  }
  return value;
}
