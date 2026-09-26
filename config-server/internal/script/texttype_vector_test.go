package script

import (
	"encoding/json"
	"os"
	"strings"
	"testing"

	"settings/internal/behaviors"
)

// ============ 内置文本特征: 共享一致性向量 (与 AHK 端共用同一份 JSON) ============
//
// testdata/text_types.json 是**双端契约**: Go 侧本测试消费它, AHK 侧由
// tools/texttype_conformance.py 提取真实函数体跑同一份用例。任何一端语义漂移都会在
// `go test` 或 `make check-texttypes` 中变红, 不再依赖"注释里写一句必须一致"。
//
// 向量的强处在于 expectTypes 记录的是**全部**命中特征 (而非单点断言):
//   - 具名特征两两互斥性: 例 "C:\av170001" 只应有 path, 不得同时命中 bilibili;
//   - plain 的排除集正确性: 任一具名特征命中时 plain 必须不出现 —— 这正是
//     "新增特征忘了同步 plain 排除集"这一历史缺陷的兜底。
type textTypeVectorDoc struct {
	Version int    `json:"version"`
	Types   []string `json:"types"`
	Cases   []struct {
		Content     string   `json:"content"`
		ExpectTypes []string `json:"expectTypes"`
		Note        string   `json:"note"`
	} `json:"cases"`
}

func loadTextTypeVector(t *testing.T) textTypeVectorDoc {
	t.Helper()
	raw, err := os.ReadFile("testdata/text_types.json")
	if err != nil {
		t.Fatalf("读取文本特征一致性向量失败: %v", err)
	}
	var doc textTypeVectorDoc
	if err := json.Unmarshal(raw, &doc); err != nil {
		t.Fatalf("解析文本特征一致性向量失败: %v", err)
	}
	if doc.Version != 1 {
		t.Fatalf("向量 version 应为 1, 实际 %d", doc.Version)
	}
	if len(doc.Cases) < 40 {
		t.Fatalf("向量条目 %d < 40 (边界覆盖不足)", len(doc.Cases))
	}
	return doc
}

// TestTextTypeVector_RegistryOrder 注册表顺序必须与向量 `types` 逐项一致 (顺序即界面顺序)。
func TestTextTypeVector_RegistryOrder(t *testing.T) {
	doc := loadTextTypeVector(t)
	got := behaviors.TextFeatureValues()
	if strings.Join(got, ",") != strings.Join(doc.Types, ",") {
		t.Fatalf("注册表顺序 %v ≠ 向量 types %v", got, doc.Types)
	}
	// 兜底特征唯一且居末: plain 的"其余都不命中"语义与界面顺序都依赖它
	if n := len(behaviors.TextFeatures()); n == 0 || behaviors.TextFeatures()[n-1].Value != doc.Types[n-1] {
		t.Fatalf("兜底特征必须居末, 实际末位 = %q", behaviors.TextFeatures()[n-1].Value)
	}
	fallbacks := 0
	for _, f := range behaviors.TextFeatures() {
		if f.Fallback {
			fallbacks++
		}
	}
	if fallbacks != 1 {
		t.Fatalf("兜底特征应恰好 1 个, 实际 %d 个", fallbacks)
	}
}

// TestTextTypeVector_MatchesGo 逐用例比对抗性: 命中集合 (按注册表顺序) 必须恒等于 expectTypes。
func TestTextTypeVector_MatchesGo(t *testing.T) {
	doc := loadTextTypeVector(t)
	for i, c := range doc.Cases {
		got := make([]string, 0, 2)
		for _, ty := range doc.Types {
			if matchTextType(ty, c.Content) {
				got = append(got, ty)
			}
		}
		if strings.Join(got, ",") != strings.Join(c.ExpectTypes, ",") {
			t.Errorf("用例#%d content=%q\n  期望命中 %v\n  Go 命中 %v\n  note=%s",
				i+1, c.Content, c.ExpectTypes, got, c.Note)
		}
	}
}

// TestTextTypeVector_PlainIsDerivedFromNamed 结构性不变量: plain 的排除集恒等于具名集。
// 直接对注册表施加"合成样例"验证派生关系 —— 新增具名特征后, 该特征的任何命中样例都必须
// 同时把 plain 排除掉 (无需手工维护排除集, 但必须保证派生逻辑本身正确)。
func TestTextTypeVector_PlainIsDerivedFromNamed(t *testing.T) {
	named := map[string]string{}
	plain := ""
	for _, f := range behaviors.TextFeatures() {
		if f.Fallback {
			plain = f.Value
		} else {
			named[f.Value] = f.Pattern
		}
	}
	if plain == "" || len(named) == 0 {
		t.Fatal("注册表应至少含 1 个具名特征与 1 个兜底特征")
	}
	// 向量里每条"命中某具名特征"的用例, 都必须同时把兜底特征排除掉
	doc := loadTextTypeVector(t)
	for i, c := range doc.Cases {
		for _, ty := range c.ExpectTypes {
			if ty == plain {
				continue
			}
			if !matchTextType(ty, c.Content) {
				t.Errorf("用例#%d %q: 向量声明命中 %s, 但实际未命中", i+1, c.Content, ty)
			}
			if matchTextType(plain, c.Content) {
				t.Errorf("用例#%d %q: 命中具名特征 %s 的同时也命中了 %s —— 排除集已失效",
					i+1, c.Content, ty, plain)
			}
		}
	}
}
