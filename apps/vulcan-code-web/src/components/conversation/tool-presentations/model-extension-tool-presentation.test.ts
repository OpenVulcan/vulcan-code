import { describe, expect, it } from "vitest";

import {
  modelExtensionToolMarkdown,
  modelExtensionToolTitleKey,
  modelExtensionToolValueMarkdown,
} from "./model-extension-tool-presentation";

describe("model extension tool presentation", () => {
  // toolCases lock the complete runtime extension-tool set to stable user-facing titles.
  // toolCases 将完整运行时扩展工具集合锁定到稳定的用户可见标题。
  const toolCases = [
    ["get_model_extension_parameters", "toolPresentation.modelExtension.parameters"],
    ["image_generate", "settings.providerTools.imageGenerate"],
    ["image_edit", "settings.providerTools.imageEdit"],
    ["video_generate", "settings.providerTools.videoGenerate"],
    ["video_edit", "settings.providerTools.videoEdit"],
    ["video_extend", "settings.providerTools.videoExtend"],
    ["speech_synthesize", "settings.providerTools.speechSynthesize"],
    ["speech_transcribe", "settings.providerTools.speechTranscribe"],
    ["music_generate", "settings.providerTools.musicGenerate"],
    ["music_cover", "settings.providerTools.musicCover"],
  ] as const;

  it.each(toolCases)("maps %s to its localized title", (toolName, titleKey) => {
    expect(modelExtensionToolTitleKey(toolName)).toBe(titleKey);
  });

  it("does not classify an adjacent unknown tool as a model extension", () => {
    expect(modelExtensionToolTitleKey("video_generate_preview")).toBeNull();
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
