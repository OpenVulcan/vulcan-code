import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import { MarkdownMessage } from "./conversation-markdown";
import { domainObject, resolveV1ToolDomainPresentation, v1ModelResult } from "./tool-presentations/v1-tool-domain-presentation";
import "@/styles/tool-domain.css";

// These exact extension ids can return resources staged by stage_provider_resources.
// 这些精确扩展标识可以返回 stage_provider_resources 暂存的资源。
const MEDIA_GENERATION_TOOLS = new Set(["image_generate", "image_edit", "video_generate", "video_edit", "video_extend", "speech_synthesize", "music_generate", "music_cover"]);

/** Identifies resource-producing extensions without guessing from model or provider names.
 * 识别资源生成型扩展，不根据模型或供应商名称猜测。
 * Parameter name is the canonical tool id; returns whether its result uses the eager media view.
 * 参数 name 为规范工具标识；返回其结果是否使用同步媒体视图。
 */
export function isMediaGenerationTool(name: string): boolean {
  return MEDIA_GENERATION_TOOLS.has(name);
}

/** Presents generated resources immediately from the persisted public and private result envelope.
 * 从持久化的公开与私有结果信封立即展示生成资源。
 * Parameter event owns model resources and generated_attachments host outputs; returns resource cards or the exact non-resource result.
 * 参数 event 持有模型资源与 generated_attachments 宿主输出；返回资源卡片或精确的非资源结果。
 */
export function ConversationMediaGenerationResult(props: { event: ToolCallEvent }) {
  // result and composite follow the two explicit producer branches, not alternate candidate paths.
  // result 与 composite 遵循产出方的两个显式分支，不轮询候选路径。
  const result = v1ModelResult(props.event.output);
  const object = domainObject(result);
  const composite = object?.kind === "composite" ? domainObject(object.composite) : null;
  const resources = composite === null ? object?.resources : composite.resources;
  // Only the generated_attachments host contract is authorized to supply local paths.
  // 只有 generated_attachments 宿主契约可以提供本地路径。
  const host = domainObject(props.event.output?.host_result);
  const payload = host?.kind === "generated_attachments" ? domainObject(host.payload) : null;
  const outputs = Array.isArray(payload?.outputs) ? payload.outputs : [];
  // diagnostic preserves execution failures even when a response contains partial resources.
  // diagnostic 保留执行失败，即使响应还包含部分资源。
  const diagnostic = resolveV1ToolDomainPresentation(props.event).diagnostic;
  // supplemental retains every non-resource composite field without duplicating the resource listing.
  // supplemental 保留复合结果的所有非资源字段，不重复铺开资源列表。
  const supplemental = composite === null ? null : Object.fromEntries(Object.entries(composite).filter(([key, value]) => key !== "resources" && value !== null && (!Array.isArray(value) || value.length > 0)));
  return <div className="tool-media-result" data-media-generation-result>
    {diagnostic ? <div className="v1-tool-domain-error" role="alert">{diagnostic}</div> : null}
    {Array.isArray(resources) ? resources.length === 0 ? <div className="v1-tool-domain-muted">未返回生成资源</div> : resources.map((value, index) => {
      // resource is one canonical public resource; malformed entries remain visibly invalid.
      // resource 为一条规范公开资源；无效条目保持明确可见。
      const resource = domainObject(value);
      if (!resource) return <div role="alert" key={index}>生成资源返回格式无效</div>;
      // Match by the producer's asset id, never by output position or file name.
      // 按产出方的资产标识匹配，绝不按输出位置或文件名关联。
      const attachment = typeof resource.asset_id === "string" ? outputs.map(domainObject).find(output => output?.asset_id === resource.asset_id) : undefined;
      const localPath = typeof attachment?.local_path === "string" && attachment.local_path.trim() ? attachment.local_path : null;
      return <article className="tool-media-resource" key={index}>
        <header><strong>{typeof resource.file_name === "string" ? resource.file_name : "生成资源"}</strong><span>{typeof resource.mime_type === "string" ? resource.mime_type : ""}{typeof resource.byte_len === "number" ? ` · ${resource.byte_len.toLocaleString()} 字节` : ""}</span></header>
        <div className="tool-media-local-path"><span>本地路径</span>{localPath ? <code tabIndex={0}>{localPath}</code> : <span className="v1-tool-domain-muted">未提供本地路径</span>}</div>
        {typeof resource.resource_path === "string" ? <details className="tool-media-reference"><summary>资源引用</summary><code>{resource.resource_path}</code></details> : null}
      </article>;
    }) : result === undefined ? <div className="v1-tool-domain-muted">尚无生成结果</div>
      : object?.kind === "text" && typeof object.text === "string" ? <MarkdownMessage content={object.text} eager />
      : typeof result === "string" ? <div className="v1-tool-domain-text">{result}</div> : <pre className="tool-model-schema">{JSON.stringify(result, null, 2)}</pre>}
    {supplemental && Object.keys(supplemental).length > 0 ? <pre className="tool-model-schema">{JSON.stringify(supplemental, null, 2)}</pre> : null}
  </div>;
}
