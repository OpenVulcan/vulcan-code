import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { DraftSubmissionTranscript } from "@/components/conversation/conversation-chrome";
import { I18nProvider } from "@/i18n";
import { appendComposerFileReferences } from "@/lib/composer-file-references";

describe("DraftSubmissionTranscript", () => {
  it("hides raw paths while a reference-only first prompt is being created", () => {
    // Reproduce the pending-to-durable transition for an attachment-only send.
    // 复现仅附件发送从待处理到持久化的阶段。
    const path = "D:/Manuals/guide.chm";
    const html = renderToStaticMarkup(<I18nProvider initialLocale="en-US"><DraftSubmissionTranscript
      scrollRef={null}
      submission={{
        clientSubmissionId: "pending-file",
        prompt: appendComposerFileReferences("", [path]),
        attachments: [], startedAt: 100, stage: "creating",
      }}
      surfaceId="draft-1"
    /></I18nProvider>);
    expect(html).not.toContain('class="vk-prompt-text"');
    expect(html).not.toContain(path);
    expect(html).toContain('aria-expanded="false"');
  });

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
