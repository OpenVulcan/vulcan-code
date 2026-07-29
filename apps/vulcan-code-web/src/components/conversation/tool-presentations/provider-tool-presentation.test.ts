import { describe, expect, it } from "vitest";
import type { ToolExecutionConversationBlock } from "@/components/conversation/conversation-tool-execution-types";
import { resolveProviderToolStructuredPresentation } from "./provider-tool-presentation";

// providerToolBlock builds one exact visible host-result fixture.
// providerToolBlock 构建一个精确可见宿主结果夹具。
function providerToolBlock(payload: Record<string, unknown>): ToolExecutionConversationBlock[] {
  return [{
    id: "tool-row",
    toolCallId: "tool-call",
    rawMetadata: {
      host_result: {
        kind: "provider_tool_output",
        payload,
      },
    },
  }] as ToolExecutionConversationBlock[];
}

describe("provider structured tool presentation", () => {
  it("resolves embedding vectors into a dedicated structured card", () => {
    const presentation = resolveProviderToolStructuredPresentation(
      providerToolBlock({
        kind: "embedding",
        embedding: {
          model: "embedding-model",
          vectors: [{ index: 0, values: [0.25, -0.5] }],
        },
      }),
      "completed",
    );

    expect(presentation).toMatchObject({
      kind: "provider_tool_structured",
      outputKind: "embedding",
      itemCount: 1,
      payload: {
        vector_count: 1,
        vectors: [{ index: 0, dimensions: 2 }],
      },
    });
  });

  it("rejects malformed composite payloads instead of guessing missing collections", () => {
    const presentation = resolveProviderToolStructuredPresentation(
      providerToolBlock({
        kind: "composite",
        composite: {
          text: [],
          resources: [],
        },
      }),
      "completed",
    );

    expect(presentation).toBeNull();
  });
});
