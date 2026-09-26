package script

import (
	"encoding/json"
	"os"
	"testing"
)

// 一致性向量 schema (与方案 D.1.1 / F.2 一致, Go 单测与 AHK 自检脚本共用同一份 JSON):
//
//	{ "version": 1, "description": "...", "cases": [ { "op", "value", "content", "expect", "note" } ] }
//
// 作用域约定 (双端必须逐字一致, 这是方案 C7 的核心风控):
//   - equals/prefix: 作用于 Trim(content) 的首个非空行;
//   - suffix/contains: 作用于整个 Trim(content);
//   - 大小写仅 ASCII 折叠 (asciiFold / AHK AsciiLower, 仅 A-Z->a-z), 非 ASCII 原样比较。
type conformanceCase struct {
	Op      string `json:"op"`
	Value   string `json:"value"`
	Content string `json:"content"`
	Expect  bool   `json:"expect"`
	Note    string `json:"note"`
}

type conformanceDoc struct {
	Version     int               `json:"version"`
	Description string            `json:"description"`
	Cases       []conformanceCase `json:"cases"`
}

// TestMatchOpConformance 读共享一致性向量, 逐条跑 matchRuleOp, 断言与 expect 一致。
// 这是"双端同源"的 Go 侧守护: 任何算子实现偏离都会在此变红。
func TestMatchOpConformance(t *testing.T) {
	raw, err := os.ReadFile("testdata/match_ops.json")
	if err != nil {
		t.Fatalf("读取一致性向量失败: %v", err)
	}
	var doc conformanceDoc
	if err := json.Unmarshal(raw, &doc); err != nil {
		t.Fatalf("解析一致性向量失败: %v", err)
	}
	if doc.Version != 1 {
		t.Fatalf("一致性向量 version 应为 1, 实际 %d", doc.Version)
	}
	if len(doc.Cases) < 20 {
		t.Fatalf("一致性向量条数 %d < 20 (方案要求至少 20 条边界覆盖)", len(doc.Cases))
	}
	for i, c := range doc.Cases {
		got := matchRuleOp(c.Op, c.Value, c.Content)
		if got != c.Expect {
			t.Errorf("向量#%d 失配: op=%q value=%q content=%q expect=%v got=%v\n  note=%s",
				i+1, c.Op, c.Value, c.Content, c.Expect, got, c.Note)
		}
	}
}
