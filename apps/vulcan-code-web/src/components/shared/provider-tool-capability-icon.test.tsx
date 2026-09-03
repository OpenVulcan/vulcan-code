import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { PublicAiProviderToolCapabilityKind } from "@/types";
import {
  ProviderToolCapabilityIcon,
  type ProviderToolCapabilityIconFamily,
} from "./provider-tool-capability-icon";

describe("ProviderToolCapabilityIcon", () => {
  // iconCases lock every provider capability to the shared settings-and-conversation icon family.
  // iconCases 将每项供应商能力锁定到设置页与会话卡片共享的图标族。
  const iconCases: ReadonlyArray<[
    PublicAiProviderToolCapabilityKind,
    ProviderToolCapabilityIconFamily,
    string,
  ]> = [
    ["web_search", "search", "lucide-search"],
    ["web_extract", "extract", "lucide-file-search"],
    ["image_understand", "image", "lucide-image"],
    ["video_understand", "video", "lucide-video"],
    ["audio_understand", "audio", "lucide-volume-2"],
    ["image_generate", "image", "lucide-image"],
    ["image_edit", "image", "lucide-image"],
    ["video_generate", "video", "lucide-video"],
    ["video_edit", "video", "lucide-video"],
    ["video_extend", "video", "lucide-video"],
    ["speech_synthesize", "audio", "lucide-volume-2"],
    ["speech_transcribe", "audio", "lucide-volume-2"],
    ["music_generate", "music", "lucide-music"],
    ["music_cover", "music", "lucide-music"],
  ];

  it.each(iconCases)("renders %s with the canonical %s family", (capability, family, iconClass) => {
    // markup is the exact SVG emitted for one closed capability value.
    // markup 是一个封闭能力值发出的精确 SVG。
    const markup = renderToStaticMarkup(
      <ProviderToolCapabilityIcon capability={capability} size={16} />,
    );
    expect(markup).toContain(`data-provider-tool-icon-family="${family}"`);
    expect(markup).toContain(iconClass);
  });
});
