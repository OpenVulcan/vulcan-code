import {
  FileSearch,
  Image,
  Music,
  Search,
  Video,
  Volume2,
} from "lucide-react";

import type { PublicAiProviderToolCapabilityKind } from "@/types";

// ProviderToolCapabilityIconFamily names the visual family shared by related provider capabilities.
// ProviderToolCapabilityIconFamily 命名相关供应商能力共享的视觉族。
export type ProviderToolCapabilityIconFamily = "search" | "extract" | "image" | "video" | "audio" | "music";

// providerToolCapabilityIconFamily resolves one exact capability to its canonical visual family.
// providerToolCapabilityIconFamily 将一项精确能力解析到其规范视觉族。
// Parameters: capability is one member of the closed provider capability contract.
// 参数：capability 是封闭供应商能力契约中的一项。
// Returns: the stable icon family used by every provider-capability surface.
// 返回值：每个供应商能力界面使用的稳定图标族。
export function providerToolCapabilityIconFamily(
  capability: PublicAiProviderToolCapabilityKind,
): ProviderToolCapabilityIconFamily {
  switch (capability) {
    case "web_search":
      return "search";
    case "web_extract":
      return "extract";
    case "image_understand":
    case "image_generate":
    case "image_edit":
      return "image";
    case "video_understand":
    case "video_generate":
    case "video_edit":
    case "video_extend":
      return "video";
    case "audio_understand":
    case "speech_synthesize":
    case "speech_transcribe":
      return "audio";
    case "music_generate":
    case "music_cover":
      return "music";
  }
  return unreachableProviderToolCapability(capability);
}

// unreachableProviderToolCapability makes future capability additions fail compilation until their icon contract is defined.
// unreachableProviderToolCapability 使未来新增能力在定义图标契约前无法通过编译。
// Parameters: capability can only be never after an exhaustive switch.
// 参数：capability 仅能在穷尽 switch 后成为 never。
// Returns: this function never returns and exists solely as an exhaustiveness guard.
// 返回值：此函数永不返回，仅作为穷尽性守卫存在。
function unreachableProviderToolCapability(capability: never): never {
  throw new Error(`Unhandled provider tool capability: ${String(capability)}`);
}

// ProviderToolCapabilityIcon renders the canonical capability-family icon shared by settings and conversation cards.
// ProviderToolCapabilityIcon 渲染由设置页与会话卡片共享的规范能力族图标。
// Parameters: capability selects the exact provider feature and size controls the Lucide glyph dimensions.
// 参数：capability 选择精确供应商能力，size 控制 Lucide 图标尺寸。
// Returns: one icon from the closed provider-capability visual contract.
// 返回值：封闭供应商能力视觉契约中的一个图标。
export function ProviderToolCapabilityIcon(props: {
  // Exact provider capability represented by the icon.
  // 图标表示的精确供应商能力。
  capability: PublicAiProviderToolCapabilityKind;
  // Square Lucide glyph size in pixels.
  // Lucide 方形图标的像素尺寸。
  size: number;
}) {
  // family is the single visual authority shared across all capability surfaces.
  // family 是所有能力界面共享的唯一视觉权威。
  const family = providerToolCapabilityIconFamily(props.capability);
  switch (family) {
    case "search":
      return <Search data-provider-tool-icon-family={family} size={props.size} />;
    case "extract":
      return <FileSearch data-provider-tool-icon-family={family} size={props.size} />;
    case "image":
      return <Image data-provider-tool-icon-family={family} size={props.size} />;
    case "video":
      return <Video data-provider-tool-icon-family={family} size={props.size} />;
    case "audio":
      return <Volume2 data-provider-tool-icon-family={family} size={props.size} />;
    case "music":
      return <Music data-provider-tool-icon-family={family} size={props.size} />;
  }
  return unreachableProviderToolIconFamily(family);
}

// unreachableProviderToolIconFamily makes the glyph renderer exhaustive over every declared visual family.
// unreachableProviderToolIconFamily 使图标渲染器穷尽每个已声明视觉族。
// Parameters: family can only be never after an exhaustive switch.
// 参数：family 仅能在穷尽 switch 后成为 never。
// Returns: this function never returns and exists solely as an exhaustiveness guard.
// 返回值：此函数永不返回，仅作为穷尽性守卫存在。
function unreachableProviderToolIconFamily(family: never): never {
  throw new Error(`Unhandled provider tool icon family: ${String(family)}`);
}
