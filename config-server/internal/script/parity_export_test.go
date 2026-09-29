package script

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

// TestExportParityCorpus 把覆盖矩阵的合成配置导出为差分对账语料
// (`tools/parity/corpus/synthetic/config.json`), 供 Rust 迁移的 parity harness 使用。
//
// 动机: `TestGoldenKeyFluxAHK` 只能证明"产物没变", `TestSyntheticConfigCoversMatrix`
// 只能证明"矩阵每一项都被行使" —— 两者都活在 Go 进程内, 而迁移需要一份**文件形式**的等价
// 输入才能跨实现比对。没有它, 语料只剩出厂样例 `factory`, 生成器的大部分分支(9 个 TypeID、
// 缩写注册表、hotifHeader 分支、windowGroups…)在 Rust 侧无人守。
//
// 只在 UPDATE_PARITY_CORPUS=1 时写盘 (与 UPDATE_GOLDEN 同风格):
//
//	cd config-server && UPDATE_PARITY_CORPUS=1 go test ./internal/script/ -run TestExportParityCorpus
//	pwsh tools/parity/run_parity.ps1 -Capture     # 导出后必须重录基线
//
// 刻意**不调用** `Preprocess`: 注入 `!f17` 由 CLI 侧负责, 让语料保持"未处理"状态反而能
// 抓住"新实现忘了 Preprocess"这类缺陷(Gold 侧会注入, 缺注入的一方产物会不等)。
func TestExportParityCorpus(t *testing.T) {
	if os.Getenv("UPDATE_PARITY_CORPUS") != "1" {
		t.Skip("set UPDATE_PARITY_CORPUS=1 to export tools/parity/corpus/synthetic/config.json")
	}

	raw, err := json.MarshalIndent(syntheticConfig(), "", "  ")
	if err != nil {
		t.Fatalf("序列化合成配置失败: %v", err)
	}
	raw = append(raw, '\n')

	// 本包目录 = config-server/internal/script ⇒ 仓库根在上三级。
	out := filepath.Join("..", "..", "..", "tools", "parity", "corpus", "synthetic", "config.json")
	if err := os.MkdirAll(filepath.Dir(out), 0o755); err != nil {
		t.Fatalf("创建语料目录失败: %v", err)
	}
	if err := os.WriteFile(out, raw, 0o644); err != nil {
		t.Fatalf("写入语料失败: %v", err)
	}
	t.Logf("已导出 %d 字节到 %s", len(raw), filepath.ToSlash(out))
}
