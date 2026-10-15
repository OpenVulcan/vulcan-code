// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as service from "@/service/knowledge-base-service";
import type { KnowledgePackageInspection } from "@/types/knowledge-base";
import { KnowledgePackageImportDialog } from "./knowledge-package-dialogs";
import { KnowledgeBaseArea } from "./knowledge-base-area";
import { knowledgeLibraryFixture } from "./knowledge-test-fixtures";

// Stable keys keep assertions independent of wording.
// 稳定键使断言不依赖文案措辞。
// Parameters: key is a catalog identity. Returns that identity.
// 参数：key 为目录身份。返回该身份。
function translate(key: string): string { return key; }
vi.mock("@/i18n", () => ({ useI18n: () => ({ t: translate, locale: "en-US" }) }));

// This synthetic complete package cannot access a real filesystem.
// 此合成完整套件不能访问真实文件系统。
const inspection: KnowledgePackageInspection = {
  archivePath: "X:/synthetic/test.vkb", sha256: "b".repeat(64), byteSize: 256,
  manifest: { format: "vulcan-knowledge-package", version: 3, name: "New package", description: "Purpose", settings: { indexMode: "bm25", embedding: null }, customWords: [], documentCount: 1, folderCount: 1, contentSha256: "a".repeat(64), exportedAtMs: 1 },
};
// Each root owns its dialogs and portals.
// 每个根拥有其弹窗及门户。
let root: Root;
// Host is isolated from the running application.
// 宿主与运行中应用隔离。
let host: HTMLDivElement;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

// Finds one semantic localized control in the requested surface.
// 在指定界面找到一个语义本地化控件。
// Parameters: label identifies the catalog key; scope confines ownership. Returns the actual button or fails with its missing key.
// 参数：label 标识目录键；scope 限定归属。返回实际按钮，缺失时携带键失败。
function button(label: string, scope: ParentNode = document): HTMLButtonElement {
  // Matching uses semantic labels rather than an evolving positional index.
  // 匹配使用语义标签，而非会演进的位置下标。
  const found = [...scope.querySelectorAll("button")].find((entry) => entry.textContent === label);
  if (found === undefined) throw new Error("Missing control: " + label);
  return found;
}

describe("independent package import and deletion confirmation", () => {
  it("imports only after explicit confirmation and prevents duplicate submissions while preserving failures", async () => {
    // The deferred import simulates an exact committed service request.
    // 延迟导入模拟精确提交服务请求。
    let reject!: (reason: Error) => void;
    // The callback accepts only a new library name, with no replacement strategy.
    // 回调仅接受新库名称，不含覆盖策略。
    const onImport = vi.fn(() => new Promise<void>((_, fail) => { reject = fail; }));
    // Close remains independent from a failed mutation.
    // 关闭与失败变更保持独立。
    const onClose = vi.fn();
    await act(async () => root.render(<KnowledgePackageImportDialog inspection={inspection} busy={false} onClose={onClose} onImport={onImport} />));
    expect(onImport).not.toHaveBeenCalled();
    expect(document.querySelector('input[type="radio"]')).toBeNull();
    await act(async () => { button("knowledge.package.import").click(); button("knowledge.package.import").click(); });
    expect(onImport).toHaveBeenCalledExactlyOnceWith(inspection.manifest.name);
    await act(async () => reject(new Error("synthetic import failure")));
    expect(document.querySelector('[role="alert"]')?.textContent).toContain("synthetic import failure");
    expect(document.querySelector<HTMLInputElement>("input")?.value).toBe(inspection.manifest.name);
    expect(onClose).not.toHaveBeenCalled();
  });

  it("deletes the frozen library only after confirmation and does not double submit", async () => {
    vi.spyOn(service, "listKnowledgeLibraries").mockResolvedValue([{ ...knowledgeLibraryFixture, lifecycleState: "error", settings: null, errorMessage: "Unsupported format" }]);
    // Keep deletion pending while the confirmation is clicked twice.
    // 确认双击期间保持删除等待。
    let reject!: (reason: Error) => void;
    // Only this synthetic service owns deletion.
    // 仅此合成服务拥有删除。
    const remove = vi.spyOn(service, "deleteKnowledgeLibrary").mockImplementation(() => new Promise((_, fail) => { reject = fail; }));
    await act(async () => root.render(<KnowledgeBaseArea projects={[]} />));
    await act(async () => host.querySelector<HTMLButtonElement>('[aria-pressed="false"]')!.click());
    await act(async () => button("knowledge.delete", host).click());
    expect(remove).not.toHaveBeenCalled();
    // The immutable confirmation surface names the selected library.
    // 不可变确认界面指明选定知识库。
    const confirmation = document.querySelector('[role="alertdialog"]')!;
    // Translation keys do not interpolate names, so service arguments prove the exact frozen target.
    // 翻译键不插值名称，因此服务参数证明精确冻结目标。
    await act(async () => { button("knowledge.delete", confirmation).click(); button("knowledge.delete", confirmation).click(); });
    expect(remove).toHaveBeenCalledExactlyOnceWith(knowledgeLibraryFixture.id);
    await act(async () => reject(new Error("synthetic deletion failure")));
    expect(confirmation.textContent).toContain("synthetic deletion failure");
    expect(document.querySelector('[role="alertdialog"]')).not.toBeNull();
  });
});
