import { describe, expect, it } from "vitest";
import { findLinkLiterals } from "./conversation-link-literals";

// linked returns the text each link literal covers, paired with its target.
// linked 返回每个链接字面量覆盖的文本及其目标。
function linked(text: string): Array<[string, string]> {
  return findLinkLiterals(text).map((literal) => [text.slice(literal.start, literal.end), literal.href]);
}

describe("findLinkLiterals", () => {
  it.each([
    ["访问 https://example.com/a。然后继续", "https://example.com/a"],
    ["链接https://a.com/x）结束", "https://a.com/x"],
    ["see www.example.com/docs，再看", "www.example.com/docs"],
    ["(see https://a.com/wiki/A_(b)).", "https://a.com/wiki/A_(b)"],
    ["[https://a.com/x]", "https://a.com/x"],
    ["Go to https://a.com/path?q=1&r=2#top.", "https://a.com/path?q=1&r=2#top"],
    ["\"https://a.com/x\"", "https://a.com/x"],
    ["at http://localhost:5173/app, then", "http://localhost:5173/app"],
    ["at http://127.0.0.1/x!", "http://127.0.0.1/x"],
    ["at http://devbox:8080/x", "http://devbox:8080/x"],
    ["at http://[::1]:3000/x", "http://[::1]:3000/x"],
  ])("ends the web address in %j where the address ends", (text, address) => {
    expect(linked(text).map(([covered]) => covered)).toEqual([address]);
  });

  it("gives www. addresses an https scheme", () => {
    expect(linked("www.example.com")).toEqual([["www.example.com", "https://www.example.com"]]);
  });

  it.each([
    "https://exa",
    "https://example.c",
    "www.x",
    "http://",
    "abchttps://a.com",
    "foo.www.example.com",
  ])("leaves %j as text", (text) => {
    expect(linked(text)).toEqual([]);
  });

  it("finds several addresses separated by spaces or full-width punctuation", () => {
    expect(linked("https://a.com、https://b.org/x；www.c.net").map(([covered]) => covered))
      .toEqual(["https://a.com", "https://b.org/x", "www.c.net"]);
  });

  it.each([
    ["打开 D:\\项目\\报告.docx 查看", "D:\\项目\\报告.docx", "file:///D:/项目/报告.docx"],
    ["文件在D:\\docs\\报告.docx里面", "D:\\docs\\报告.docx", "file:///D:/docs/报告.docx"],
    ["see C:/work/a.tar.gz.", "C:/work/a.tar.gz", "file:///C:/work/a.tar.gz"],
    ["at C:\\src\\app.ts:12-20, then", "C:\\src\\app.ts:12-20", "file:///C:/src/app.ts:12-20"],
    ["log /var/log/app.log。", "/var/log/app.log", "file:///var/log/app.log"],
    ["(C:\\a\\b(1).txt)", "C:\\a\\b(1).txt", "file:///C:/a/b(1).txt"],
    ["C:\\a\\100%.txt", "C:\\a\\100%.txt", "file:///C:/a/100%25.txt"],
  ])("links the absolute path in %j", (text, covered, href) => {
    expect(linked(text)).toEqual([[covered, href]]);
  });

  it.each([
    ["打开file:///C:/a/报告.pdf然后", "file:///C:/a/报告.pdf"],
    ["see file:///tmp/page.html.", "file:///tmp/page.html"],
    ["see file:///C:/my%20files/a.pdf#L3", "file:///C:/my%20files/a.pdf#L3"],
  ])("links the file URL in %j as written", (text, url) => {
    expect(linked(text)).toEqual([[url, url]]);
  });

  it.each([
    "src/app.ts",
    "./src/app.ts",
    "/etc",
    "/a.txt",
    "and/or",
    "km/h/v.s",
    "C:\\folder\\",
    "C:\\folder\\name",
    "GET /v1/chat/completions",
    "xC:\\a\\b.txt",
  ])("leaves %j as text", (text) => {
    expect(linked(text)).toEqual([]);
  });

  it("does not link paths inside a web address", () => {
    expect(linked("https://a.com/x/y.txt").map(([covered]) => covered)).toEqual(["https://a.com/x/y.txt"]);
    expect(linked("a.com/x/y.txt")).toEqual([]);
  });
});
