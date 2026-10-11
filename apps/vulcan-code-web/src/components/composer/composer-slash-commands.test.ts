import { describe, expect, it } from "vitest";
import { composerSlashCommandsForSession } from "./composer-model";
import type { ComposerSlashCommandSummary } from "@/types";

// command builds one service catalog row from its stable id.
// command 根据稳定标识构建一条服务目录记录。
const command = (id: string): ComposerSlashCommandSummary => ({ id, command: `/${id}` });

// ids projects the filtered commands back to their stable identities.
// ids 将筛选后的指令映射回稳定标识。
const ids = (commands: ComposerSlashCommandSummary[]) => commands.map((entry) => entry.id);

describe("composerSlashCommandsForSession", () => {
  const catalog = ["model", "compact", "plan", "goal", "settings", "style", "collapse", "skill", "skills", "future"]
    .map(command);

  it("lists every routable command for ordinary sessions in catalog order", () => {
    expect(ids(composerSlashCommandsForSession(catalog, null))).toEqual(
      ["model", "compact", "plan", "goal", "settings", "style", "collapse", "skill", "skills"],
    );
  });

  it("hides frozen and unused features from dedicated sessions", () => {
    expect(ids(composerSlashCommandsForSession(catalog, "standard"))).toEqual(
      ["model", "settings", "collapse", "skill", "skills"],
    );
  });

  it("hides prompt-skill commands when the dedicated session has no tools", () => {
    expect(ids(composerSlashCommandsForSession(catalog, "none"))).toEqual(["model", "settings", "collapse"]);
  });
});
