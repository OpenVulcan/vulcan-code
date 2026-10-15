import { beforeEach, expect, it, vi } from "vitest";
import { createKnowledgeLibrary, importKnowledgePackage, readKnowledgeChangeDocument, readManagedKnowledgeDocument, setKnowledgeIndexMode, setKnowledgeProjectBinding, setSessionKnowledgeLibraries } from "./knowledge-base-service";

// The transport mock verifies exact wire parameters without a running service.
// 传输模拟验证精确线协议参数，无需运行服务。
const request = vi.hoisted(() => vi.fn());
vi.mock("@/service/webui-service-runtime", () => ({ serviceClient: { request } }));
beforeEach(() => { request.mockReset().mockResolvedValue(undefined); });

it("sends explicit create and index-mode request DTOs with no removed policy fields", async () => {
  // New libraries have a conscious mode and no embedding fallback.
  // 新库具有主动选择模式，不含嵌入兜底。
  const create = { name: "Synthetic", description: "Purpose", settings: { indexMode: "bm25" as const, embedding: null }, projectIds: [] };
  await createKnowledgeLibrary(create);
  expect(request).toHaveBeenLastCalledWith("service_create_knowledge_library", { request: create });
  // Mode writes target one exact library rather than rewriting metadata.
  // 模式写入定位一个精确库，而非重写元信息。
  const mode = { libraryId: "library", settings: { indexMode: "browse" as const, embedding: null } };
  await setKnowledgeIndexMode(mode);
  expect(request).toHaveBeenLastCalledWith("service_set_knowledge_index_mode", { request: mode });
});

it("keeps current and historical document selectors on their respective endpoints", async () => {
  // Current reads use stable IDs and exact line bounds.
  // 当前读取使用稳定身份及精确行边界。
  const current = { libraryId: "library", documentId: "document", startLine: 7 };
  await readManagedKnowledgeDocument(current);
  expect(request).toHaveBeenLastCalledWith("service_read_knowledge_library_document", { request: current });
  // Historical reads additionally freeze sequence and side.
  // 历史读取额外冻结序号及侧别。
  const historical = { ...current, seq: 3, side: "before" as const };
  await readKnowledgeChangeDocument(historical);
  expect(request).toHaveBeenLastCalledWith("service_read_knowledge_change_document", { request: historical });
});

it("binds import to the inspected full fingerprint and exposes no replacement parameters", async () => {
  // A path alone cannot identify a reviewed mutable archive.
  // 仅路径不能标识已审阅可变归档。
  const imported = { archivePath: "X:/synthetic.vkb", name: "New library", expectedSha256: "b".repeat(64) };
  await importKnowledgePackage(imported);
  expect(request).toHaveBeenCalledExactlyOnceWith("service_import_knowledge_package", { request: imported });
});

it("sends project grants and session ID selections through the current exact request wrappers", async () => {
  // Authorization and inherited default are distinct explicit flags.
  // 授权及继承默认为不同明确标记。
  const binding = { projectId: "project", libraryId: "library", authorized: true, defaultEnabled: false };
  await setKnowledgeProjectBinding(binding);
  expect(request).toHaveBeenLastCalledWith("service_set_knowledge_project_binding", { request: binding });
  // Session selection contains only IDs and declared inheritance mode.
  // 会话选择仅包含身份及声明继承模式。
  const selection = { projectId: "project", conversationId: "session", mode: "explicit" as const, libraryIds: ["library"] };
  await setSessionKnowledgeLibraries(selection);
  expect(request).toHaveBeenLastCalledWith("service_set_session_knowledge_libraries", { request: selection });
});
