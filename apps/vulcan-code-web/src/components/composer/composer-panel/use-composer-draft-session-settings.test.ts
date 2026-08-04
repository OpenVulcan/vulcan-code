import { describe, expect, it } from "vitest";

import {
  captureDraftSessionSettingsSnapshot,
  clearDraftSessionSettingsEntry,
} from "./use-composer-draft-session-settings";

describe("captureDraftSessionSettingsSnapshot", () => {
  it("captures only the selected new-conversation slot", () => {
    const entries = {
      "draft-a": {
        vmmPolicy: {
          precheckMode: "disabled" as const,
          memoryWritebackMode: "enabled" as const,
        },
      },
      "draft-b": {
        knowledgeSelection: {
          mode: "explicit" as const,
          libraryIds: ["library-b"],
        },
      },
    };

    expect(captureDraftSessionSettingsSnapshot(entries, "draft-b", null)).toEqual({
      knowledgeSelection: {
        mode: "explicit",
        libraryIds: ["library-b"],
      },
    });
  });

  it("returns null for persisted conversations and empty draft slots", () => {
    const entries = {
      draft: {
        initialGoal: {
          objective: "Finish the task",
          tokenBudget: 4096,
        },
      },
    };

    expect(captureDraftSessionSettingsSnapshot(entries, "draft", "conversation-a")).toBeNull();
    expect(captureDraftSessionSettingsSnapshot(entries, "missing", null)).toBeNull();
  });
});

describe("clearDraftSessionSettingsEntry", () => {
  it("consumes only the accepted draft slot", () => {
    const entries = {
      "draft-a": {
        initialGoal: {
          objective: "Finish A",
          tokenBudget: null,
        },
      },
      "draft-b": {
        knowledgeSelection: {
          mode: "inherit" as const,
          libraryIds: [],
        },
      },
    };

    expect(clearDraftSessionSettingsEntry(entries, "draft-a", null)).toEqual({
      "draft-b": entries["draft-b"],
    });
  });

  it("preserves settings for persisted sessions and unknown slots", () => {
    const entries = {
      draft: {
        initialGoal: {
          objective: "Keep this goal",
          tokenBudget: 2048,
        },
      },
    };

    expect(clearDraftSessionSettingsEntry(entries, "draft", "conversation-a")).toBe(entries);
    expect(clearDraftSessionSettingsEntry(entries, "missing", null)).toBe(entries);
  });
});

describe("output-style draft inheritance", () => {
  it("keeps other draft settings when the output-style override is cleared", () => {
    const draft: {
      outputStyleId?: string;
      initialGoal: { objective: string; tokenBudget: null };
    } = {
      outputStyleId: "rigorous",
      initialGoal: {
        objective: "Keep this goal",
        tokenBudget: null,
      },
    };
    const next = { ...draft };
    delete next.outputStyleId;

    expect(next).toEqual({
      initialGoal: draft.initialGoal,
    });
  });
});
