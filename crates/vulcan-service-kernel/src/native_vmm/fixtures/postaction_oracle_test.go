package usecase

import (
	"encoding/json"
	"os"
	"strings"
	"testing"

	logicdomain "github.com/openvulcan/vmm/internal/logic/domain"
)

// parityBudget records the exact budget and selected chronological reference IDs.
// parityBudget 记录准确预算与选中的时间顺序参考身份。
type parityBudget struct {
	// MaxTokens includes the compact target object and reference details.
	// MaxTokens 包含紧凑目标对象与参考详情。
	MaxTokens int `json:"max_tokens"`
	// Selected stores only the IDs returned by the production selector.
	// Selected 仅保存生产选择器返回的身份。
	Selected []uint64 `json:"selected"`
}

// parityPayload records real external serialization and history selection for one admitted dialogue.
// parityPayload 记录一条准入后对话的真实外置序列化与历史选择。
type parityPayload struct {
	// Name identifies a synthetic case, never a user session.
	// Name 标识合成用例，而非用户会话。
	Name string `json:"name"`
	// Payload is the exact unmodified buildPostActionTurnPayload output.
	// Payload 为未经修改的 buildPostActionTurnPayload 准确输出。
	Payload string `json:"payload"`
	// Tokens is the production estimator result for Payload.
	// Tokens 为 Payload 的生产估算结果。
	Tokens int `json:"tokens"`
	// References preserves oldest-to-newest synthetic details.
	// References 保留从旧到新的合成详情。
	References []string `json:"references"`
	// Budgets records all integer boundary probes for this target.
	// Budgets 记录该目标的全部整数边界探测。
	Budgets []parityBudget `json:"budgets"`
}

// TestNativeParityPostActionOracle invokes real package functions through a read-only Go overlay.
// TestNativeParityPostActionOracle 通过只读 Go 覆盖层调用真实包函数。
// The required output path receives generated synthetic fixtures; no service, provider, or user data is used.
// 必填输出路径接收生成的合成样本；不使用服务、供应商或用户数据。
func TestNativeParityPostActionOracle(t *testing.T) {
	// cases covers admitted business text; cleaner parity has its own independent corpus.
	// cases 覆盖准入后的业务文本；清洗对照有独立样本集。
	cases := []struct {
		// name provides a stable fixture label.
		// name 提供稳定样本标签。
		name string
		// turn is the real external business input type.
		// turn 为真实外置业务输入类型。
		turn logicdomain.TurnRecord
	}{
		{"english", logicdomain.TurnRecord{UserContent: " Use PostgreSQL. ", AssistantContent: " Confirmed. "}},
		{"chinese", logicdomain.TurnRecord{UserContent: "我喜欢西瓜和葡萄。", AssistantContent: "已记录水果偏好。"}},
		{"middle-clarification", logicdomain.TurnRecord{UserContent: "数据库选择", Timeline: []logicdomain.TurnTimelineItem{{Type: "assistant", Content: " 需要事务吗？ "}, {Type: "user", Content: " 是，必须支持事务。 "}}, AssistantContent: "使用 PostgreSQL。"}},
		{"missing-final", logicdomain.TurnRecord{UserContent: "查询数据库", Timeline: []logicdomain.TurnTimelineItem{{Type: "assistant", Content: "哪个环境？"}}}},
		{"html-operators", logicdomain.TurnRecord{UserContent: "Keep x < 3 && y > 2; <tag> is literal text.", AssistantContent: "A & B"}},
		{"unicode-separators", logicdomain.TurnRecord{UserContent: "第一行\u2028第二行\u2029第三段", AssistantContent: "😀 café e\u0301"}},
		{"literal-escape-text", logicdomain.TurnRecord{UserContent: `Keep literal \u003c and \u2028 unchanged; actual < differs.`, AssistantContent: `C:\users\test & next`}},
		{"json-and-control", logicdomain.TurnRecord{UserContent: "{\"path\":\"C:\\\\data\"}\nnext\tcolumn\r\nend", AssistantContent: "Quote: \"yes\"; \\ slash"}},
		{"mixed-long", logicdomain.TurnRecord{UserContent: strings.Repeat("保留业务事实 PostgreSQL transaction.\n", 80), AssistantContent: "最终确认。"}},
	}
	// fixtures is populated solely with real production outputs.
	// fixtures 仅填入真实生产输出。
	fixtures := make([]parityPayload, 0, len(cases))
	for _, sample := range cases {
		// body and targetCost come from the unmodified external serializer.
		// body 与 targetCost 来自未经修改的外置序列化器。
		body, targetCost, err := buildPostActionTurnPayload(sample.turn)
		if err != nil {
			t.Fatal(err)
		}
		// history uses real persisted row types and positive cached detail budgets.
		// history 使用真实持久行类型及正数缓存详情预算。
		history := []logicdomain.SessionTurnRecord{
			{ID: 1, Details: "旧事实：数据库使用 PostgreSQL。"},
			{ID: 2, Details: "中间澄清：需要事务与备份。"},
			{ID: 3, Details: "最新决定：每天备份，保留七天。"},
		}
		// fixture collects chronological text and the source-computed target cost.
		// fixture 汇集时间顺序正文与来源计算的目标成本。
		fixture := parityPayload{Name: sample.name, Payload: body, Tokens: targetCost}
		// total tracks the complete reference cost without saturation.
		// total 记录全部参考成本，不执行饱和截断。
		total := 0
		for i := range history {
			history[i].DetailsBudget = estimatePostActionTextBudget(history[i].Details)
			total += history[i].DetailsBudget
			fixture.References = append(fixture.References, history[i].Details)
		}
		for _, limit := range []int{0, targetCost - 1, targetCost, targetCost + 1, targetCost + history[2].DetailsBudget, targetCost + total - 1, targetCost + total} {
			// selected follows the exact target-first branch in buildTurnAnalysisInputWithResolvedTime.
			// selected 遵循 buildTurnAnalysisInputWithResolvedTime 的准确目标优先分支。
			selected := history
			if limit > 0 {
				selected = trimHistoryTurnsByBudget(history, max(0, limit-targetCost))
			}
			// ids stays an empty JSON array when no reference is admitted.
			// 未准入参考时 ids 保持为空 JSON 数组。
			ids := make([]uint64, 0, len(selected))
			for _, row := range selected {
				ids = append(ids, row.ID)
			}
			fixture.Budgets = append(fixture.Budgets, parityBudget{MaxTokens: limit, Selected: ids})
		}
		fixtures = append(fixtures, fixture)
	}
	// path is an explicit private generated-artifact destination supplied by the caller.
	// path 为调用方提供的明确私有生成产物路径。
	path := os.Getenv("VULCAN_PARITY_ORACLE_OUTPUT")
	if path == "" {
		t.Fatal("VULCAN_PARITY_ORACLE_OUTPUT is required")
	}
	// output contains generated evidence, not a handwritten replacement oracle.
	// output 包含生成证据，而非手写替代预期。
	output, err := json.MarshalIndent(fixtures, "", "  ")
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, append(output, '\n'), 0600); err != nil {
		t.Fatal(err)
	}
	t.Logf("generated %d production payloads and %d history selections", len(fixtures), len(fixtures)*7)
}
