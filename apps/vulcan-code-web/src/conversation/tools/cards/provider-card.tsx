// Provider card for model-extension tools other than web search and extraction (see web-cards.tsx): media
// understanding shows the answer; generation names the prompt and model while the produced files follow the card as
// media.
// 网页搜索与提取以外的模型扩展工具的供应商卡（网页卡见 web-cards.tsx）：媒体理解展示回答；生成类说明提示词与模型，
// 产出的文件以媒体形式紧随卡片之后。
import type * as React from "react";
import {
  AudioLines, Clapperboard, Film, ImagePlus, Images, Mic, Music, ScanEye, SlidersHorizontal, Sparkles, Volume2,
  type LucideIcon,
} from "lucide-react";
import { useI18n, type TranslationKey } from "@/i18n";
import { ToolFrame, type ToolCardProps, type ToolFamily } from "../tool-card";
import { listOf, objectOf, textOf, type ToolCall } from "../tool-call";
import { MediaResultView } from "../views/media-result-view";
import { ModelParametersView } from "../views/model-parameters-view";
import { ErrorNote, MarkdownResult } from "../views/result-views";

// ProviderToolSpec fixes how one model-extension tool is named and which arguments describe its request.
// ProviderToolSpec 固定一个模型扩展工具的命名方式以及描述其请求的参数。
interface ProviderToolSpec {
  // Purpose family.
  // 用途家族。
  family: ToolFamily;
  // Head icon.
  // 标题图标。
  icon: LucideIcon;
  // Action verb.
  // 动作词。
  verb: TranslationKey;
  // Arguments that carry the request text, in priority order.
  // 承载请求文本的参数，按优先级排列。
  request: readonly string[];
}

// PROMPT_ARGUMENTS are the request-text arguments shared by generation tools.
// PROMPT_ARGUMENTS 是生成类工具共有的请求文本参数。
const PROMPT_ARGUMENTS = ["prompt", "instruction", "text", "lyrics"] as const;

// UNDERSTAND_ARGUMENTS are the request-text arguments shared by understanding tools.
// UNDERSTAND_ARGUMENTS 是理解类工具共有的请求文本参数。
const UNDERSTAND_ARGUMENTS = ["prompt", "instruction", "task"] as const;

// PROVIDER_TOOLS is keyed by canonical tool name; only these tools use the provider card.
// PROVIDER_TOOLS 以规范工具名为键；只有这些工具使用供应商卡。
const PROVIDER_TOOLS: Readonly<Record<string, ProviderToolSpec>> = {
  image_understand: { family: "read", icon: ScanEye, request: UNDERSTAND_ARGUMENTS, verb: "toolCard.provider.understandImage" },
  video_understand: { family: "read", icon: Film, request: UNDERSTAND_ARGUMENTS, verb: "toolCard.provider.understandVideo" },
  audio_understand: { family: "read", icon: AudioLines, request: UNDERSTAND_ARGUMENTS, verb: "toolCard.provider.understandAudio" },
  speech_transcribe: { family: "read", icon: Mic, request: [], verb: "toolCard.provider.transcribe" },
  image_generate: { family: "media", icon: ImagePlus, request: PROMPT_ARGUMENTS, verb: "toolCard.provider.imageGenerate" },
  image_edit: { family: "media", icon: Images, request: PROMPT_ARGUMENTS, verb: "toolCard.provider.imageEdit" },
  video_generate: { family: "media", icon: Clapperboard, request: PROMPT_ARGUMENTS, verb: "toolCard.provider.videoGenerate" },
  video_edit: { family: "media", icon: Film, request: PROMPT_ARGUMENTS, verb: "toolCard.provider.videoEdit" },
  video_extend: { family: "media", icon: Film, request: PROMPT_ARGUMENTS, verb: "toolCard.provider.videoExtend" },
  speech_synthesize: { family: "media", icon: Volume2, request: PROMPT_ARGUMENTS, verb: "toolCard.provider.speechSynthesize" },
  music_generate: { family: "media", icon: Music, request: PROMPT_ARGUMENTS, verb: "toolCard.provider.musicGenerate" },
  music_cover: { family: "media", icon: Music, request: PROMPT_ARGUMENTS, verb: "toolCard.provider.musicCover" },
  get_model_extension_parameters: { family: "read", icon: SlidersHorizontal, request: [], verb: "toolCard.provider.parameters" },
};

// HOST_TARGET_PREFIX marks the host provider targets (e.g. vulcan_image_generate) that do what the unprefixed
// model-extension tool does.
// HOST_TARGET_PREFIX 标记与无前缀模型扩展工具作用相同的宿主供应商目标（如 vulcan_image_generate）。
const HOST_TARGET_PREFIX = "vulcan_";

// providerToolSpec returns the spec of one model-extension tool, or null for any other tool.
// providerToolSpec 返回模型扩展工具的规格；其他工具返回 null。
export function providerToolSpec(call: ToolCall): ProviderToolSpec | null {
  const name = call.name.startsWith(HOST_TARGET_PREFIX) ? call.name.slice(HOST_TARGET_PREFIX.length) : call.name;
  return Object.hasOwn(PROVIDER_TOOLS, name) ? PROVIDER_TOOLS[name] : null;
}

// providerArguments reads the model-native arguments: executors wrap them as {model, args}, direct tools do not.
// providerArguments 读取模型原生参数：执行工具以 {model, args} 包装，直连工具不包装。
function providerArguments(call: ToolCall): Record<string, unknown> {
  return objectOf(call.input.args) ?? call.input;
}

// stripReportHeading removes the "# image understanding" or "# Result" heading the card already names.
// stripReportHeading 移除卡片标题已表达的 "# image understanding" 或 "# Result" 标题。
export function stripReportHeading(text: string): string {
  return text.replace(/^# (?:\w+ understanding|Result)\n+/, "");
}

/** Renders one media understanding, generation or parameter lookup call.
 * 渲染一次媒体理解、生成或参数查询调用。 */
export function ProviderCard(props: ToolCardProps) {
  const { t } = useI18n();
  const { call } = props;
  const spec = providerToolSpec(call);
  const args = providerArguments(call);
  const output = call.isError ? "" : call.modelOutput ?? "";
  const parameters = spec?.verb === "toolCard.provider.parameters";
  const request = (spec?.request ?? []).map((key) => textOf(args[key])?.trim()).find(Boolean) ?? "";
  const model = textOf(call.input.model) ?? (parameters ? null : textOf(args.model));
  const outputs = call.host?.kind === "generated_attachments" ? listOf(call.host.payload.outputs).length : 0;
  let target = request;
  if (parameters) target = [textOf(args.capability), textOf(args.model)].filter(Boolean).join(" · ");
  if (!target) target = mediaSource(args);
  const meta: React.ReactNode[] = [];
  if (outputs > 0) meta.push(<span key="files">{t("toolCard.provider.files").replace("{count}", String(outputs))}</span>);
  if (model) meta.push(<span className="vk-truncate" key="model" title={model}>{model}</span>);
  return (
    <ToolFrame
      {...props}
      icon={spec?.icon ?? Sparkles}
      meta={meta.length > 0 ? meta : null}
      state={call.state}
      target={target}
      targetTitle={target}
      verb={spec ? t(spec.verb) : call.event.tool_name}
    >
      {call.isError ? <ErrorNote text={call.modelOutput} />
        : spec?.family === "media" ? (
          <>
            {request ? <p className="vk-tool-note">{request}</p> : null}
            <MediaResultView event={call.event} />
          </>
        )
          : parameters ? <ModelParametersView event={call.event} />
            : output.trim() ? <MarkdownResult text={stripReportHeading(output)} /> : null}
    </ToolFrame>
  );
}

// MEDIA_SOURCE_ARGUMENTS name the media a request reads, used as the target when it has no request text.
// MEDIA_SOURCE_ARGUMENTS 指明请求读取的媒体；请求没有文本时用作目标。
const MEDIA_SOURCE_ARGUMENTS = ["audio_path", "image_path", "video_path", "source_video_path", "media_paths", "source_paths"] as const;

// mediaSource reads the media a request reads.
// mediaSource 读取请求所读取的媒体。
function mediaSource(args: Record<string, unknown>): string {
  for (const key of MEDIA_SOURCE_ARGUMENTS) {
    const value = args[key];
    const text = typeof value === "string" ? value : listOf(value).filter((item) => typeof item === "string").join(" · ");
    if (text.trim()) return text.trim();
  }
  return "";
}
