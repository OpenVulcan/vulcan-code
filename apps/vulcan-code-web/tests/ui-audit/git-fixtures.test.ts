import { describe, expect, it } from "vitest";
import { auditGitCatalog, auditGitSnapshot } from "./git-fixtures";
import { serviceClient } from "@/service/webui-service-runtime";

describe("isolated multi-repository inspection", () => {
  it("keeps duplicate labels scoped to their exact roots and preserves an empty staging area", () => {
    // catalog is fictional and never probes the host filesystem.
    // catalog 是虚构目录，绝不探测宿主文件系统。
    const catalog = auditGitCatalog("X:/ui-audit/project", "long-list");
    expect(catalog.repositories).toHaveLength(4);
    expect(catalog.repositories[0].name).toBe(catalog.repositories[1].name);
    expect(catalog.repositories[3].monitoringEnabled).toBe(false);
    for (const repository of catalog.repositories.slice(0, 2)) {
      expect(auditGitSnapshot(catalog.projectRoot, repository.repositoryKey, "unstaged")).toMatchObject({ repositoryRoot: repository.repositoryRoot, changedFiles: 1 });
      expect(auditGitSnapshot(catalog.projectRoot, repository.repositoryKey, "staged")).toMatchObject({ unifiedDiff: "", changedFiles: 0 });
    }
    expect(auditGitCatalog(catalog.projectRoot, "empty").repositories).toEqual([]);
  });
  it("exposes read failures and rejects write operations instead of simulating success", async () => {
    expect(() => auditGitSnapshot("X:/ui-audit/project", "third_party/unavailable", "unstaged")).toThrow("隔离验收");
    expect(() => auditGitSnapshot("X:/ui-audit/project", "unknown", "unstaged")).toThrow("UI_AUDIT_UNKNOWN_REPOSITORY");
    await expect(serviceClient.request("workspace.git.setMonitoring", {})).rejects.toThrow("UI_AUDIT_UNIMPLEMENTED_RPC");
    await expect(serviceClient.request("service_run_git_commit_action", {})).rejects.toThrow("UI_AUDIT_UNIMPLEMENTED_RPC");
  });
});
