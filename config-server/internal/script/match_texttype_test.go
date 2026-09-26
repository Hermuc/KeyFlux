package script

import (
	"path/filepath"
	"strings"
	"testing"

	"settings/internal/behaviors"
)

// ============ matchTextType: 注册表分派壳的少量非向量断言 ============
//
// 逐条命中语义已于 2026-09-17 迁到**共享一致性向量**
// (testdata/text_types.json + texttype_vector_test.go), 覆盖 63 条边界用例并与 AHK 运行时同源。
// 本文件只保留不便写进向量的断言: 特征名归一化口径、未知值口径、行为包覆盖前置。

func TestMatchTextType_ValueNormalization(t *testing.T) {
	// 特征名: 去首尾空白 + 大小写归一 (与旧实现 strings.ToLower(TrimSpace(t)) 同口径)
	for _, name := range []string{"bilibili", "BILIBILI", " bilibili ", "\tBiliBili\n"} {
		if !matchTextType(name, "av170001") {
			t.Errorf("特征名 %q 应归一为 bilibili 并命中", name)
		}
	}
	// 内容一律不 Trim: 该值会被原样拼进视频 URL (bin/behaviors/open_bilibili)
	if matchTextType("bilibili", " av170001") {
		t.Error("内容含前导空格不应命中")
	}
	// 未知特征名: false (与旧 switch 的 default 分支同口径)
	for _, name := range []string{"", "unknown", "type:custom", "url2"} {
		if matchTextType(name, "https://x") {
			t.Errorf("未知特征名 %q 应返回 false", name)
		}
	}
	// 自定义引用 (type:<id>) 不是内置特征: 由 matchActionRule 另走解析路径, 注册表不该认识它
	if behaviors.IsKnownTextType("type:custom") {
		t.Error("IsKnownTextType 不应把 type: 引用当内置特征")
	}
}

// ============ 行为包前置: 每个内置特征必须在「添加映射」弹窗里有可用行为 ============
//
// 方案 C7 的死胡同: 特征值没有行为覆盖时, 弹窗勾选列表为空且确认按钮禁用。
// 本夹具刻意读**真实** bin/behaviors (而非内联夹具): 新增内置特征却漏建行为包时,
// 其余 Go 单测会全绿, 缺陷只在用户界面里暴露 —— 故必须在此处兜住。
//
// 覆盖检查由注册表驱动 —— 新增特征时**无需改本测试**, 只要行为包缺失就会变红。
func TestBuiltinCatalog_CoversEveryTextFeature(t *testing.T) {
	cat := behaviors.LoadCatalog(filepath.Join("..", "..", "..", "bin", "behaviors"), filepath.Join(t.TempDir(), "user"))
	if len(cat.Errors) > 0 {
		t.Fatalf("内置行为包目录加载有错: %v", cat.Errors)
	}
	for _, v := range behaviors.TextFeatureValues() {
		// DefaultFor 无 default 标记时回退"第一条覆盖包" ⇒ 非空 == 存在覆盖该前提的行为包
		if got := cat.DefaultFor("textType", []string{v}); got == "" {
			t.Errorf("内置文本特征 %q 没有任何内置行为包覆盖 (appliesTo textType=%s) —— "+
				"「添加映射」弹窗该类型下会是空列表", v, v)
			continue
		}
	}
	if got := cat.DefaultFor("textType", []string{"bilibili"}); got != "open_bilibili" {
		t.Errorf("textType=bilibili 的默认行为 = %q, want open_bilibili", got)
	}
	// bilibili 的模板必须把选中值拼进 B 站视频地址: entry.action 是通用的 open (RunReplaced),
	// 模板缺失时会把视频号当命令直接 Run。
	p := cat.Get("open_bilibili")
	if p == nil || !strings.Contains(p.Entry.Params.ActionValue, "https://www.bilibili.com/video/") ||
		!strings.Contains(p.Entry.Params.ActionValue, "%selected%") {
		t.Fatalf("open_bilibili 的 entry.params.actionValue 未把 %%selected%% 拼进 B 站视频地址: %+v", p)
	}
}
