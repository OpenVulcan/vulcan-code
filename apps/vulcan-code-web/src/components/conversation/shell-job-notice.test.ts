import { describe, expect, it } from "vitest";
import { decodeShellJobNotice } from "./shell-job-notice";

// notice builds the text the Core renders for one finished job (format of ShellJobNoticeFacts::render).
// notice 构建 Core 为一个已结束任务渲染的文本（ShellJobNoticeFacts::render 的格式）。
const notice = (fields: string[]) => [
  "<vulcan_shell_job_notice version=\"1\" job_id=\"job-1\">",
  "Background shell job job-1 finished.",
  ...fields,
  "Read its output with shell_job (action `read` or `search`, job_id `job-1`) if you need it.",
  "</vulcan_shell_job_notice>",
].join("\n");

describe("decodeShellJobNotice", () => {
  it("keeps multi-line commands and reads the outcome from the end", () => {
    expect(decodeShellJobNotice(notice([
      "Purpose: Serve the docs",
      "Command: npm run dev\necho Status: fake",
      "Status: stopped",
      "Duration: 12 ms",
      "Reason: stopped by the user",
    ]))).toEqual({ jobId: "job-1", purpose: "Serve the docs", command: "npm run dev\necho Status: fake", status: "stopped", exitCode: null });
    expect(decodeShellJobNotice(notice([
      "Purpose: ", "Command: make", "Status: failed", "Duration: 3 ms", "Exit code: 2",
    ]))?.exitCode).toBe(2);
  });

  it("ignores other agent messages and truncated notices", () => {
    expect(decodeShellJobNotice("Please review the result.")).toBeNull();
    const text = notice(["Purpose: Build", "Command: make", "Status: completed", "Duration: 1 ms", "Exit code: 0"]);
    expect(decodeShellJobNotice(text.slice(0, -3))).toBeNull();
  });
});
