import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { DraftSubmissionTranscript } from "@/components/conversation/conversation-chrome";

describe("DraftSubmissionTranscript", () => {
  // The first accepted prompt must occupy the normal user-bubble layout before any durable session exists.
  // 首条已接纳提示词必须在持久会话建立前占据正常的用户气泡布局。
  it("renders the accepted prompt and attachments immediately", () => {
    const html = renderToStaticMarkup(<DraftSubmissionTranscript
      scrollRef={null}
      submission={{
        clientSubmissionId: "client-1",
        prompt: "first prompt",
        attachments: [{ kind: "file", fileName: "notes.txt", localPath: "", byteLen: null, mimeType: "text/plain" }],
        startedAt: 100,
        stage: "creating",
      }}
      surfaceId="draft-1"
    />);
    expect(html).toContain('data-client-action-id="client-1"');
    expect(html).toContain('class="vk-prompt-row"');
    expect(html).toContain("first prompt");
    expect(html).toContain("notes.txt");
  });
});
