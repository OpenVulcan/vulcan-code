import { describe, expect, it } from "vitest";

import {
  continuationClaimSnapshotHasConverged,
  reconcileContinuationClaimSnapshot,
  type ContinuationClaimExpectation,
} from "@/components/conversation/continuation-claim-reconciliation";
import { browserFallbackSnapshot } from "@/service/browser-fallback-workbench";
import type { ConversationSummary, WorkbenchSnapshot } from "@/types";

// expectation identifies the exact Continue token consumed by the test ACK.
// expectation 标识测试 ACK 消费的精确 Continue 令牌。
const expectation: ContinuationClaimExpectation = {
  conversationId: "conversation-continue",
  expectedGeneration: 7,
  expectedLedgerRevision: 13,
  acceptedRevision: 101,
};

// continuationConversation builds one target summary with an optional continuation projection.
// continuationConversation 构造带有可选 Continue 投影的目标会话摘要。
//
// Parameters: continuation is the exact backend continuation summary to expose.
// 参数：continuation 是需要暴露的精确后端 Continue 摘要。
// Returns: a complete conversation summary suitable for a Workbench fixture.
// 返回值：可用于 Workbench 测试夹具的完整会话摘要。
function continuationConversation(
  continuation: ConversationSummary["continuation"],
): ConversationSummary {
  return {
    id: expectation.conversationId,
    title: "Continue target",
    status: continuation?.eligible ? "stopped" : "running",
    runPhase: continuation?.eligible ? "idle" : "preparing",
    continuation,
  };
}

// workbenchWithConversation builds a full browser fixture around one target summary and revision.
// workbenchWithConversation 围绕单个目标摘要与修订号构造完整浏览器夹具。
//
// Parameters: revision is the service generation and conversation is the target summary.
// 参数：revision 是服务代际，conversation 是目标摘要。
// Returns: a complete Workbench snapshot.
// 返回值：完整 Workbench 快照。
function workbenchWithConversation(
  revision: number,
  conversation: ConversationSummary,
): WorkbenchSnapshot {
  return {
    ...browserFallbackSnapshot(),
    revision,
    conversations: [conversation],
    selectedConversationId: conversation.id,
  };
}

// Verifies snapshot convergence requires both ACK coverage and consumption of the exact eligible token.
// 验证快照收敛同时要求覆盖 ACK 修订并消费精确可领取令牌。
describe("continuationClaimSnapshotHasConverged", () => {
  it("rejects stale or still-eligible snapshots and accepts a consumed token", () => {
    // eligibleConversation is the exact pre-ACK button token.
    // eligibleConversation 是 ACK 前按钮使用的精确令牌。
    const eligibleConversation = continuationConversation({
      eligible: true,
      generation: expectation.expectedGeneration,
      expectedLedgerRevision: expectation.expectedLedgerRevision,
    });
    // staleRevisionSnapshot was built before the ACK durability boundary.
    // staleRevisionSnapshot 在 ACK 持久边界之前构建。
    const staleRevisionSnapshot = workbenchWithConversation(100, continuationConversation(null));
    // stillEligibleSnapshot covers the ACK revision but still exposes the consumed token.
    // stillEligibleSnapshot 覆盖 ACK 修订但仍暴露已经消费的令牌。
    const stillEligibleSnapshot = workbenchWithConversation(101, eligibleConversation);
    // convergedSnapshot covers the ACK and no longer publishes the old token.
    // convergedSnapshot 覆盖 ACK 且不再发布旧令牌。
    const convergedSnapshot = workbenchWithConversation(102, continuationConversation(null));

    expect(continuationClaimSnapshotHasConverged(staleRevisionSnapshot, expectation)).toBe(false);
    expect(continuationClaimSnapshotHasConverged(stillEligibleSnapshot, expectation)).toBe(false);
    expect(continuationClaimSnapshotHasConverged(convergedSnapshot, expectation)).toBe(true);
  });
});

// Verifies a lost Continue delta is repaired without replacing unrelated or already-newer state.
// 验证丢失的 Continue 增量会被修复，同时不替换无关状态或已经更新的状态。
describe("reconcileContinuationClaimSnapshot", () => {
  it("replaces only the target summary when the old eligible token remains visible", () => {
    // eligibleConversation is the stale local summary left behind by a lost broadcast delta.
    // eligibleConversation 是广播增量丢失后残留的本地过期摘要。
    const eligibleConversation = continuationConversation({
      eligible: true,
      generation: expectation.expectedGeneration,
      expectedLedgerRevision: expectation.expectedLedgerRevision,
    });
    // unrelatedConversation proves target-only reconciliation preserves neighboring summaries.
    // unrelatedConversation 证明目标级对账会保留相邻摘要。
    const unrelatedConversation: ConversationSummary = {
      id: "conversation-unrelated",
      title: "Unrelated",
      status: "idle",
    };
    // newerLocalConversation simulates target fields updated after the full snapshot while the Continue delta was missing.
    // newerLocalConversation 模拟全量快照之后目标字段已更新、但 Continue 增量缺失的状态。
    const newerLocalConversation: ConversationSummary = {
      ...eligibleConversation,
      title: "Locally newer title",
      runPhase: "streaming",
    };
    // current simulates a later delta that advanced the global revision without consuming the target token.
    // current 模拟较晚增量推进了全局修订，但没有消费目标令牌。
    const current: WorkbenchSnapshot = {
      ...workbenchWithConversation(104, newerLocalConversation),
      conversations: [newerLocalConversation, unrelatedConversation],
    };
    // authoritativeConversation is the post-ACK target summary returned by the full snapshot.
    // authoritativeConversation 是 ACK 后全量快照返回的目标摘要。
    const authoritativeConversation = continuationConversation(null);
    // incoming is older than the local global revision but still covers the ACK and owns the missing target fact.
    // incoming 的全局修订早于本地，但仍覆盖 ACK 并持有缺失的目标事实。
    const incoming = workbenchWithConversation(102, authoritativeConversation);

    const reconciled = reconcileContinuationClaimSnapshot(current, incoming, expectation);

    expect(reconciled).not.toBe(current);
    expect(reconciled.revision).toBe(104);
    expect(reconciled.conversations[0]).toEqual({
      ...newerLocalConversation,
      continuation: null,
    });
    expect(reconciled.conversations[1]).toBe(unrelatedConversation);
  });

  it("does not regress a target that already moved to a newer continuation boundary", () => {
    // newerConversation is a distinct terminal boundary reached after the acknowledged claim.
    // newerConversation 是已确认领取后到达的另一个终止边界。
    const newerConversation = continuationConversation({
      eligible: true,
      generation: expectation.expectedGeneration + 1,
      expectedLedgerRevision: expectation.expectedLedgerRevision + 2,
    });
    // current already owns the newer target state and must remain authoritative.
    // current 已持有更新的目标状态，必须继续保持权威。
    const current = workbenchWithConversation(110, newerConversation);
    // incoming contains the earlier post-ACK preparing summary.
    // incoming 包含较早的 ACK 后准备阶段摘要。
    const incoming = workbenchWithConversation(102, continuationConversation(null));

    expect(reconcileContinuationClaimSnapshot(current, incoming, expectation)).toBe(current);
  });

  it("does not consume a future full-snapshot revision through target-only repair", () => {
    // eligibleConversation is the stale token that cannot justify accepting unrelated future state.
    // eligibleConversation 是无法证明可接收无关未来状态的过期令牌。
    const eligibleConversation = continuationConversation({
      eligible: true,
      generation: expectation.expectedGeneration,
      expectedLedgerRevision: expectation.expectedLedgerRevision,
    });
    // current has not yet accepted the incoming full-snapshot revision.
    // current 尚未接收传入的完整快照修订号。
    const current = workbenchWithConversation(101, eligibleConversation);
    // incoming must first pass the normal full-snapshot merger because it owns a newer global revision.
    // incoming 持有更新的全局修订号，因此必须先通过常规完整快照合并器。
    const incoming = workbenchWithConversation(102, continuationConversation(null));

    expect(reconcileContinuationClaimSnapshot(current, incoming, expectation)).toBe(current);
    expect(current.revision).toBe(101);
  });
});
