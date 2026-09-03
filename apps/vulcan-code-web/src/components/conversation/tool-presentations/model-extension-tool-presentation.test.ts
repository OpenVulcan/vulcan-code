import { describe, expect, it } from "vitest";

import {
  modelExtensionToolCapability,
  modelExtensionToolMarkdown,
  modelExtensionToolTitleKey,
  modelExtensionToolValueMarkdown,
} from "./model-extension-tool-presentation";

describe("model extension tool presentation", () => {
  // toolCases lock the complete runtime extension-tool set to stable user-facing titles.
  // toolCases 将完整运行时扩展工具集合锁定到稳定的用户可见标题。
  const toolCases = [
    ["get_model_extension_parameters", "toolPresentation.modelExtension.parameters", null],
    ["web_search", "settings.providerTools.webSearch", "web_search"],
    ["web_extract", "settings.providerTools.webExtract", "web_extract"],
    ["image_understand", "settings.aiUnderstandImage", "image_understand"],
    ["video_understand", "settings.aiUnderstandVideo", "video_understand"],
    ["audio_understand", "settings.aiUnderstandAudio", "audio_understand"],
    ["image_generate", "settings.providerTools.imageGenerate", "image_generate"],
    ["image_edit", "settings.providerTools.imageEdit", "image_edit"],
    ["video_generate", "settings.providerTools.videoGenerate", "video_generate"],
    ["video_edit", "settings.providerTools.videoEdit", "video_edit"],
    ["video_extend", "settings.providerTools.videoExtend", "video_extend"],
    ["speech_synthesize", "settings.providerTools.speechSynthesize", "speech_synthesize"],
    ["speech_transcribe", "settings.providerTools.speechTranscribe", "speech_transcribe"],
    ["music_generate", "settings.providerTools.musicGenerate", "music_generate"],
    ["music_cover", "settings.providerTools.musicCover", "music_cover"],
  ] as const;

  it.each(toolCases)("maps %s to its localized title and canonical capability icon", (
    toolName,
    titleKey,
    capability,
  ) => {
    expect(modelExtensionToolTitleKey(toolName)).toBe(titleKey);
    expect(modelExtensionToolCapability(toolName)).toBe(capability);
  });

  it("does not classify an adjacent unknown tool as a model extension", () => {
    expect(modelExtensionToolTitleKey("video_generate_preview")).toBeNull();
    expect(modelExtensionToolCapability("video_generate_preview")).toBeNull();
  });

  it("formats extension JSON as an indented Markdown code block", () => {
    expect(modelExtensionToolMarkdown(
      "speech_synthesize",
      '{"resources":[{"file_name":"audio-1.mp3","byte_len":58740}]}',
    )).toBe([
      "```json",
      "{",
      '  "resources": [',
      "    {",
      '      "file_name": "audio-1.mp3",',
      '      "byte_len": 58740',
      "    }",
      "  ]",
      "}",
      "```",
    ].join("\n"));
  });

  it("formats persisted extension input values without requiring a result string", () => {
    expect(modelExtensionToolValueMarkdown("video_generate", {
      duration_seconds: 6,
      prompt: "A blue flame",
    })).toBe([
      "```json",
      "{",
      '  "duration_seconds": 6,',
      '  "prompt": "A blue flame"',
      "}",
      "```",
    ].join("\n"));
    expect(modelExtensionToolValueMarkdown("video_generate", null)).toBe("");
  });

  it("preserves non-JSON failures and unknown-tool content", () => {
    expect(modelExtensionToolMarkdown("video_generate", "Tool execution failed.")).toBe("Tool execution failed.");
    expect(modelExtensionToolMarkdown("custom_tool", '{"ok":true}')).toBe('{"ok":true}');
  });
});
