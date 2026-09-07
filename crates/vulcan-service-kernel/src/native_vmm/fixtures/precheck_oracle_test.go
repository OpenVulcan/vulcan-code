package usecase

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"testing"

	logicdomain "github.com/openvulcan/vmm/internal/logic/domain"
	"github.com/openvulcan/vmm/internal/platform/textutil"
)

// parityPrecheckFixture captures the exact mixed-state production history and budget selections.
// parityPrecheckFixture 捕获准确的混合状态生产历史及预算选择。
type parityPrecheckFixture struct {
	// Sources contains original synthetic user inputs before production sanitization.
	// Sources 包含生产清洗之前的合成用户原文。
	Sources []string `json:"sources"`
	// Assistant is the shared canonical final answer.
	// Assistant 为共用的规范最终回答。
	Assistant string `json:"assistant"`
	// Turns holds the real loadRecentTurnContexts output under a nonbinding budget.
	// Turns 保存不受预算限制时真实 loadRecentTurnContexts 的输出。
	Turns []logicdomain.PreCheckTurnContext `json:"turns"`
	// Budgets records identities selected at each positive public-policy boundary.
	// Budgets 记录每个公开策略正数边界所选中的身份。
	Budgets []parityBudget `json:"budgets"`
}

// TestNativeParityPrecheckOracle runs real cleanup, payload, cost and recent-context functions.
// TestNativeParityPrecheckOracle 运行真实清洗、载荷、成本与最近上下文函数。
// Only storage row delivery is stubbed; no provider, service process or external repository file is mutated.
// 仅存储行交付使用桩；不变更供应商、服务进程或外置仓库文件。
func TestNativeParityPrecheckOracle(t *testing.T) {
	// fixture uses two actually refined and two pending row shapes matching the native host acceptance.
	// fixture 使用与原生宿主验收一致的两个已提炼及两个待处理行形态。
	fixture := parityPrecheckFixture{Assistant: "已记录数据库部署要求，后续按这条规则检查配置。"}
	// sanitizer is the same production text-cleaning pipeline used before external persistence.
	// sanitizer 为外置持久化前使用的同一生产文本清洗流水线。
	sanitizer := textutil.NewPostActionTextSanitizer()
	// rows retain chronological IDs and real source-computed budgets.
	// rows 保留时间顺序身份及真实来源计算的预算。
	rows := make([]logicdomain.SessionTurnRecord, 0, 4)
	for index := 0; index < 4; index++ {
		// source stays business prose even when literal operators or Unicode separators are present.
		// 即使含有字面运算符或 Unicode 分隔符，source 仍为业务正文。
		source := fmt.Sprintf("项目数据库采用 PostgreSQL，部署前必须核对备份与连接配置，这是历史验收规则第 %d 条。", index)
		if index == 2 {
			source += "查询条件 x < 3 && y > 2。"
		}
		if index == 3 {
			source += "第一行\u2028第二行\u2029第三段；保留字面 \\u003c。"
		}
		fixture.Sources = append(fixture.Sources, source)
		// raw and cost come from the real serializer, after the real external sanitizer.
		// raw 与 cost 在真实外置清洗后由真实序列化器产生。
		raw, cost, err := buildPostActionTurnPayload(logicdomain.TurnRecord{
			UserContent: sanitizer.Sanitize(source), AssistantContent: sanitizer.Sanitize(fixture.Assistant),
		})
		if err != nil {
			t.Fatal(err)
		}
		// row starts pending; only the first two receive legitimate refined details for this selection fixture.
		// row 初始待处理；只有前两个在此选择夹具中具有合法提炼详情。
		row := logicdomain.SessionTurnRecord{ID: uint64(index + 1), DehydratedContent: raw, DehydratedBudget: cost}
		if index < 2 {
			row.ExtractedStatus = logicdomain.TurnExtractedStatusDone
			row.Details = sanitizer.Sanitize(source)
			row.DetailsBudget = estimatePreCheckBudget(row.Details)
		}
		rows = append(rows, row)
	}
	// usecase invokes actual context-type selection and budget logic over the explicit ordered rows.
	// usecase 对明确有序行调用实际上下文类型选择及预算逻辑。
	usecase := &PreCheckUseCase{store: &stubPreCheckStore{recentTurns: rows}, config: PreCheckConfig{HistoryTurns: 4, MaxInputTokens: 131072}}
	// full is the actual untrimmed production context, not a handwritten expectation.
	// full 为实际未裁剪生产上下文，而非手写预期。
	full, err := usecase.loadRecentTurnContexts(context.Background(), logicdomain.SessionRef{})
	if err != nil {
		t.Fatal(err)
	}
	fixture.Turns = full
	// costs refer to selected DETAILS or RAW_TURN, including the source's persisted budget semantics.
	// costs 对应选中的 DETAILS 或 RAW_TURN，包含来源持久预算语义。
	costs := make([]int, len(rows))
	for i, row := range rows {
		costs[i] = preCheckTurnBudget(row)
	}
	// suffix sums define exact adjacent integer boundaries without native-derived expectations.
	// 后缀和定义准确相邻整数边界，不使用原生派生预期。
	suffix := []int{costs[3], costs[2] + costs[3], costs[1] + costs[2] + costs[3], costs[0] + costs[1] + costs[2] + costs[3]}
	// limits include the one-token latest-row rule and every suffix inclusion edge.
	// limits 包含单令牌保留最新行规则及每个后缀接纳边界。
	limits := []int{1, suffix[0] - 1, suffix[0], suffix[0] + 1, suffix[1] - 1, suffix[1], suffix[2] - 1, suffix[2], suffix[3] - 1, suffix[3]}
	for _, limit := range limits {
		usecase.config.MaxInputTokens = limit
		// contexts contains real selected payloads from the production method.
		// contexts 包含生产方法实际选中的载荷。
		contexts, err := usecase.loadRecentTurnContexts(context.Background(), logicdomain.SessionRef{})
		if err != nil {
			t.Fatal(err)
		}
		// ids preserve exact chronological output order.
		// ids 保留准确的时间顺序输出。
		ids := make([]uint64, 0, len(contexts))
		for _, row := range contexts {
			ids = append(ids, row.TurnID)
		}
		fixture.Budgets = append(fixture.Budgets, parityBudget{MaxTokens: limit, Selected: ids})
	}
	// destination is an explicit generated-evidence path, never an implicit user configuration path.
	// destination 为明确的生成证据路径，绝非隐式用户配置路径。
	destination := os.Getenv("VULCAN_PRECHECK_ORACLE_OUTPUT")
	if destination == "" {
		t.Fatal("VULCAN_PRECHECK_ORACLE_OUTPUT is required")
	}
	// output serializes only synthetic evidence produced above.
	// output 仅序列化上述生成的合成证据。
	output, err := json.MarshalIndent(fixture, "", "  ")
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(destination, append(output, '\n'), 0600); err != nil {
		t.Fatal(err)
	}
	t.Logf("generated %d mixed-state contexts and %d budget boundaries", len(full), len(limits))
}
