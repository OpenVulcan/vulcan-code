import { describe, expect, it } from "vitest";

import { composerAttachmentIdentity, parseComposerPromptSource } from "@/service/composer-attachment-reference";
import type { ComposerAttachmentSummary } from "@/types";

// The metadata fixture is not a staged asset and does not claim any original model.
// 元数据夹具不是暂存资产，也不声明任何原始模型。
const PROJECT_ATTACHMENT: ComposerAttachmentSummary = {
  kind: "image", fileName: "image.png", localPath: "", mimeType: "image/png", projectResourceId: 7,
};

// Verifies view identity remains separate from exact per-prompt provenance.
// 验证视图身份与精确逐提示词来源保持分离。
describe("composerAttachmentIdentity", () => {
  // Distinguishes repeated ResourceIds while preserving stable original source anchors across reordering.
  // 区分重复ResourceId，同时跨重排保留稳定的原始来源锚点。
  it("distinguishes repeated project occurrences without rewriting provenance", () => {
    // The original position belongs to the historical prompt, not the current draft.
    // 原始位置属于历史提示词，而不是当前草稿。
    const attachment = { ...PROJECT_ATTACHMENT, promptSource: { client_action_id: "original-action", resource_position: 2 } };
    expect(composerAttachmentIdentity(attachment, 0)).toBe("project-resource:7:0");
    expect(composerAttachmentIdentity(attachment, 1)).toBe("project-resource:7:1");
    expect(attachment.promptSource.resource_position).toBe(2);
  });

  // Rejects dual ownership, unsafe project ids, and staged assets that claim prompt provenance.
  // 拒绝双重所有权、不安全项目ID，以及声明提示词来源的暂存资产。
  it("rejects ambiguous owners and malformed view identities", () => {
    expect(composerAttachmentIdentity({ ...PROJECT_ATTACHMENT, assetId: "asset-a" }, 0)).toBeNull();
    for (const id of [0, -1, 1.5, Number.MAX_SAFE_INTEGER + 1]) {
      expect(composerAttachmentIdentity({ ...PROJECT_ATTACHMENT, projectResourceId: id }, 0)).toBeNull();
    }
    for (const position of [-1, 0.5, Number.MAX_SAFE_INTEGER + 1]) {
      expect(composerAttachmentIdentity(PROJECT_ATTACHMENT, position)).toBeNull();
    }
    expect(composerAttachmentIdentity({ ...PROJECT_ATTACHMENT, projectResourceId: undefined, assetId: "asset-a" }, 0)).toBe("asset-a");
    expect(composerAttachmentIdentity({ ...PROJECT_ATTACHMENT, projectResourceId: undefined, assetId: "asset-a", promptSource: { client_action_id: "old", resource_position: 0 } }, 0)).toBeNull();
    expect(composerAttachmentIdentity({ ...PROJECT_ATTACHMENT, projectResourceId: undefined }, 0)).toBeNull();
  });
});

// Verifies the frontend accepts exactly the declared Core action/position shape.
// 验证前端只接受声明的Core动作/位置形状。
describe("parseComposerPromptSource", () => {
  // Unknown provenance stays null; UTF-8 length follows Core rather than JavaScript string length.
  // 未知来源保持空值；UTF-8长度遵循Core，而非JavaScript字符串长度。
  it("accepts unknown and exact boundary sources", () => {
    expect(parseComposerPromptSource(null)).toBeNull();
    // Eighty-five Chinese characters occupy 255 UTF-8 bytes.
    // 八十五个汉字占255个UTF-8字节。
    const source = { client_action_id: "中".repeat(85), resource_position: 0 };
    expect(parseComposerPromptSource(source)).toEqual(source);
    expect(parseComposerPromptSource({ client_action_id: "a\nb", resource_position: Number.MAX_SAFE_INTEGER })).toEqual({ client_action_id: "a\nb", resource_position: Number.MAX_SAFE_INTEGER });
  });

  // Explicit malformed claims fail instead of becoming unknown provenance.
  // 显式格式错误声明失败，不转成未知来源。
  it("rejects invalid source claims", () => {
    for (const value of [undefined, [], "action", {},
      { client_action_id: "中".repeat(86), resource_position: 0 },
      { client_action_id: " action", resource_position: 0 },
      { client_action_id: "action", resource_position: -1 },
      { client_action_id: "action", resource_position: 0, model: "guessed" },
    ]) expect(() => parseComposerPromptSource(value)).toThrow();
  });
});
