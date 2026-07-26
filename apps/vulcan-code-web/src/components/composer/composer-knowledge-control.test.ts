import { describe, expect, it } from "vitest";

import { nextExplicitKnowledgeLibraryIds } from "./composer-knowledge-control";
import {
  notifyKnowledgeProjectBindingsChanged,
  subscribeKnowledgeProjectBindingsChanged,
} from "@/service/knowledge-base-service";

describe("nextExplicitKnowledgeLibraryIds", () => {
  it("starts an explicit selection from the effective inherited defaults", () => {
    expect(nextExplicitKnowledgeLibraryIds({
      mode: "inherit",
      libraryIds: [],
      effectiveLibraryIds: ["default-library"],
    }, "selected-library", true)).toEqual(["default-library", "selected-library"]);
  });

  it("removes one inherited default without clearing the other effective libraries", () => {
    expect(nextExplicitKnowledgeLibraryIds({
      mode: "inherit",
      libraryIds: [],
      effectiveLibraryIds: ["library-a", "library-b"],
    }, "library-a", false)).toEqual(["library-b"]);
  });

  it("deduplicates additions and removes only the requested library", () => {
    const selection = {
      mode: "explicit" as const,
      libraryIds: ["library-a", "library-b"],
      effectiveLibraryIds: ["library-a", "library-b"],
    };

    expect(nextExplicitKnowledgeLibraryIds(selection, "library-a", true)).toEqual([
      "library-a",
      "library-b",
    ]);
    expect(nextExplicitKnowledgeLibraryIds(selection, "library-a", false)).toEqual([
      "library-b",
    ]);
  });
});

describe("knowledge project binding revisions", () => {
  it("notifies active subscribers with the exact project and stops after unsubscribe", () => {
    // receivedProjectIds records only revisions delivered while the subscription is active.
    // receivedProjectIds 仅记录订阅有效期间收到的修订。
    const receivedProjectIds: string[] = [];
    // unsubscribe owns the exact listener registration under test.
    // unsubscribe 持有测试中的精确监听器注册。
    const unsubscribe = subscribeKnowledgeProjectBindingsChanged((projectId) => {
      receivedProjectIds.push(projectId);
    });

    notifyKnowledgeProjectBindingsChanged("project-a");
    unsubscribe();
    unsubscribe();
    notifyKnowledgeProjectBindingsChanged("project-b");

    expect(receivedProjectIds).toEqual(["project-a"]);
  });

  it("isolates a failing subscriber from later listeners", () => {
    // receivedProjectIds proves later listeners still receive the exact revision.
    // receivedProjectIds 证明后续监听器仍能收到精确修订。
    const receivedProjectIds: string[] = [];
    // originalConsoleError suppresses the expected diagnostic while preserving the global function.
    // originalConsoleError 在保留全局函数的同时抑制预期诊断。
    const originalConsoleError = console.error;
    console.error = () => {};
    // unsubscribeFailing owns the deliberately failing listener registration.
    // unsubscribeFailing 持有刻意失败的监听器注册。
    const unsubscribeFailing = subscribeKnowledgeProjectBindingsChanged(() => {
      throw new Error("expected listener failure");
    });
    // unsubscribeHealthy owns the listener that must still receive the notification.
    // unsubscribeHealthy 持有仍必须收到通知的监听器注册。
    const unsubscribeHealthy = subscribeKnowledgeProjectBindingsChanged((projectId) => {
      receivedProjectIds.push(projectId);
    });

    try {
      expect(() => notifyKnowledgeProjectBindingsChanged("project-c")).not.toThrow();
      expect(receivedProjectIds).toEqual(["project-c"]);
    } finally {
      unsubscribeFailing();
      unsubscribeHealthy();
      console.error = originalConsoleError;
    }
  });
});
