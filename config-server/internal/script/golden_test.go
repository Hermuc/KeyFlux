package script

import (
	"bytes"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// 黄金文件测试: 用 Go 字面量合成的最小配置走完整渲染管线 (Preprocess -> SaveAHK),
// 把产物与 testdata/golden.keyflux.ahk 逐行比对, 守护"配置 -> AHK 脚本"的生成契约。
//
// 严禁读取 data/config.json (含个人数据, 且样例变动会反复打破快照); 合成配置见 syntheticConfig()。
//
// 相对路径口径与既有 skin_defaults_test.go 一致: go test 的工作目录是本包目录
// (config-server/internal/script), 故模板为 ../../templates/keyflux.tmpl。
const (
	goldenTemplate  = "../../templates/keyflux.tmpl"
	goldenFile      = "testdata/golden.keyflux.ahk"
	updateGoldenEnv = "UPDATE_GOLDEN"
)

// ============================ 确定性约束 (勿破坏) ============================
// 生成路径里有两处依赖 map 迭代顺序的非稳定处理, 快照要不 flake 就必须让合成配置规避它们。
// 修改本文件的配置前请先读完这两条, 否则 -count=N 会偶发红。
//
// 约束 1 —— sortHotkeys (generators/generators.go):
//   它把 keymap.Hotkeys (map) 摊平后用 **非稳定** 的 sort.Slice 排序, 排序键只有
//   (TypeID, len(hotkey), hotkey 字典序)。渲染路径 **没有** 使用 plan.go 里的
//   deterministicSort (那个多了一级 WindowGroupID 兜底, 只服务 Oracle 计划, 不服务渲染)。
//   => 同一个被渲染的 keymap 内, 每个 Action 的 (TypeID, hotkey) 组合必须唯一,
//      否则并列项的相对顺序取决于 map 随机序。
//   本配置的做法: 每个被渲染 keymap 里"一个 hotkey 只挂一个 Action", 且 hotkey 互不相同,
//   于是 (TypeID, hotkey) 天然唯一。
//
// 约束 2 —— handleKeyRemapping (model/methods.go):
//   它 range custom.Hotkeys (map, 随机序) 收集所有 TypeID==5 (RemapKey) 动作, 之后只用
//   sort.SliceStable 按 WindowGroupID 排序。SliceStable 保留并列项的原始(随机)顺序。
//   => ID==1 keymap 里的重映射动作必须使用互不相同的 WindowGroupID。
//   本配置的做法: 六个重映射分别用 WindowGroupID 0..5, 全互异, 排序后顺序完全确定。
// ============================================================================

// TestGoldenKeyFluxAHK 是快照断言主体。
// UPDATE_GOLDEN=1 时改写黄金文件而非断言 (用于渲染规则有意变更后刷新基线)。
func TestGoldenKeyFluxAHK(t *testing.T) {
	gotRaw := generateAHK(t)

	if os.Getenv(updateGoldenEnv) == "1" {
		if err := os.MkdirAll(filepath.Dir(goldenFile), 0755); err != nil {
			t.Fatalf("创建 testdata 目录失败: %v", err)
		}
		// 落盘 SaveAHK 的原始字节 (含模板带来的 BOM 与 CRLF); 比对时两侧都做归一化。
		if err := os.WriteFile(goldenFile, []byte(gotRaw), 0644); err != nil {
			t.Fatalf("写入黄金文件失败: %v", err)
		}
		t.Logf("%s=1: 已写入 %d 字节到 %s", updateGoldenEnv, len(gotRaw), goldenFile)
		return
	}

	wantRaw, err := os.ReadFile(goldenFile)
	if err != nil {
		t.Fatalf("读取黄金文件 %s 失败 (先用 %s=1 go test ./internal/script/... 生成): %v", goldenFile, updateGoldenEnv, err)
	}

	// BOM 断言: AHK v2 解析非 ASCII 需要 UTF-8 BOM, 产物必须携带;
	// 此检查独立于归一化比对, 避免 normalizeAHK 双侧剥 BOM 导致缺失不可观测。
	if !bytes.HasPrefix([]byte(gotRaw), []byte{0xEF, 0xBB, 0xBF}) {
		t.Errorf("生成产物缺少 UTF-8 BOM (0xEF 0xBB 0xBF); AHK v2 解析非 ASCII 字符需要 BOM")
	}

	got, want := normalizeAHK(gotRaw), normalizeAHK(string(wantRaw))
	if got == want {
		return
	}
	reportFirstDiff(t, want, got)
	t.FailNow()
}

// TestSyntheticConfigCoversMatrix 逐个断言合成配置真的把覆盖矩阵里每一项都渲染进了产物。
// 快照相等只能证明"没变", 无法证明"覆盖到了"; 本测试用标记串显式钉住每个特性,
// 一旦有人改配置/生成器导致某条路径不再被行使, 这里会先于快照失去意义而报警。
func TestSyntheticConfigCoversMatrix(t *testing.T) {
	out := normalizeAHK(generateAHK(t))

	// desc 说明覆盖点, needle 是产物里必须出现的确定性子串。
	matrix := []struct{ desc, needle string }{
		// Preprocess 注入 + 全部 9 个 TypeID
		{`Preprocess 注入 !f17 (TypeID9/ValueID2, 免疫 suspend)`, `km.Map("!f17", _ => KeyFluxReload()`},
		{`TypeID1 activateOrRun1`, `ActivateOrRun(`},
		{`TypeID2 systemActions2 (SystemLockScreen)`, `SystemLockScreen()`},
		{`TypeID3 windowActions3 (SmartCloseWindow)`, `SmartCloseWindow()`},
		{`TypeID3 windowActions3 ValueID4 特殊分支 (taskSwitch)`, `Send("^!{tab}")`},
		{`TypeID4 mouseActions4 移动 (fast.MoveMouseUp)`, `fast.MoveMouseUp`},
		{`TypeID4 mouseActions4 滚轮 ValueID>=5 分支 (fast.ScrollWheelUp)`, `fast.ScrollWheelUp`},
		{`TypeID5 remapKey5 普通分支 km.RemapKey`, `km.RemapKey("t", "x")`},
		{`TypeID5 remapKey5 HotIf 分支 km.RemapInHotIf`, `km.RemapInHotIf(`},
		{`TypeID6 sendKeys6`, `Send(`},
		{`TypeID7 textFeatures7 send 分支`, `Send("{blind}^{left}")`},
		{`TypeID7 textFeatures7 callMap 分支 (HoldDownModifierKey)`, `HoldDownModifierKey("LShift")`},
		{`TypeID8 builtinFunctions8 (AHKCode 直出)`, `MsgBox("hello")`},
		{`TypeID9 ValueID6 capslock 缩写启用`, `EnterCapslockAbbr(capsHook)`},
		{`TypeID9 ValueID5 semicolon 缩写启用`, `EnterSemicolonAbbr(semiHook, semiHookAbbrWindow)`},
		// TypeID5 重映射特殊路径 -> 模板尾部 .KeyMapping
		{`.KeyMapping 渲染重映射 (a::b, WindowGroupID0)`, `a::b`},
		{`.KeyMapping 渲染重映射 (k::l, WindowGroupID5)`, `k::l`},
		// hotifHeader 全分支 (仅由重映射路径产生)
		{`hotifHeader conditionType1 WinActive`, `#HotIf WinActive(`},
		{`hotifHeader conditionType2 WinExist`, `#HotIf WinExist(`},
		{`hotifHeader conditionType3 !WinActive`, `#HotIf !WinActive(`},
		{`hotifHeader conditionType4 !WinExist`, `#HotIf !WinExist(`},
		{`hotifHeader conditionType5 表达式 (保留单引号)`, `#HotIf 'WinActive("A") && GetKeyState("Shift")'`},
		// 缩写注册表 (AbbrRegistryCode)
		{`capslock 缩写注册`, `CommandResolver.Register("capslock"`},
		{`semicolon 缩写注册`, `CommandResolver.Register("semicolon"`},
		{`缩写 conditionType5 守卫去单引号 (abbr_registry.go:46)`, `CommandStep(() => Run("calc.exe"), WinActive("A") && GetKeyState("Shift"), 5)`},
		{`缩写多动作 (multi)`, `CommandResolver.Register("capslock", "multi"`},
		// windowGroups: 单行直出 vs 多行 ahk_group
		{`windowGroup 单行 value 直出 (GroupToWinTile)`, `"ahk_exe code.exe"`},
		{`windowGroup 多行 value ahk_group 分支`, `ahk_group MY_WINDOW_GROUP_2`},
		{`windowGroup 多行 GroupAdd 展开`, `GroupAdd("MY_WINDOW_GROUP_2", "ahk_exe chrome.exe")`},
		{`keymap 多行 disableAt -> GROUP_DISABLE_KEYMAP`, `ahk_group GROUP_DISABLE_KEYMAP_6`},
		// selectedAction (方案 D 单键分发)
		{`selectedActionCode 渲染 + SelectedActionInit`, `SelectedActionInit(">^p", SelectedActionData)`},
		{`selectedAction 数据列 key/behavior 透传 (菜单序号 1..9)`, `key: 2, behavior: "search"`},
		{`selectedAction 规则字段透传 (matchType/matchValue)`, `matchType: "fileExt", matchValue: "jpg,png"`},
		{`selectedAction 显示名 (目录缺失回退 behavior ID)`, `name: "open_url"`},
		// pathVariables: 普通值 vs ahk-expression 前缀
		{`pathVariable 普通值 (AhkString)`, `editor := "D:\tools\edit.exe"`},
		{`pathVariable ahk-expression 前缀原样输出`, `desktop := A_Desktop`},
	}

	for _, m := range matrix {
		if !strings.Contains(out, m.needle) {
			t.Errorf("覆盖点未被行使: %s\n  期望产物包含子串: %q", m.desc, m.needle)
		}
	}
}

// generateAHK 复现 command.GenerateAHK / script.GenerateScripts 的确切管线:
// 先 Preprocess (向 ID==1 注入 !f17) 再 SaveAHK, 输出写到 t.TempDir() 后读回原始字节,
// 从而 100% 复用 SaveAHK 的字节管线 (CRLF 归一化 + 模板自带 BOM 随渲染进产物)。
func generateAHK(t *testing.T) string {
	t.Helper()

	cfg := syntheticConfig()
	Preprocess(cfg) // 必须与 SaveAHK 保持同序, 否则 !f17 不会出现在快照里

	outPath := filepath.Join(t.TempDir(), "KeyFlux.ahk")
	if err := SaveAHK(cfg, goldenTemplate, outPath); err != nil {
		t.Fatalf("SaveAHK 失败: %v", err)
	}
	raw, err := os.ReadFile(outPath)
	if err != nil {
		t.Fatalf("读取生成产物失败: %v", err)
	}
	return string(raw)
}

// normalizeAHK 让比对对 BOM 与行尾漂移免疫:
// SaveAHK 产出 CRLF, 模板头部带 UTF-8 BOM (会随渲染进产物); 而编辑器可能把黄金文件
// 重存为 LF 或增删 BOM。比对前对产物与黄金文件统一去 BOM + 统一折成 LF, 避免假失败。
func normalizeAHK(s string) string {
	s = strings.TrimPrefix(s, "\ufeff")
	s = strings.ReplaceAll(s, "\r\n", "\n")
	s = strings.ReplaceAll(s, "\r", "\n")
	return s
}

// reportFirstDiff 定位首个差异行并打印上下文 (用 %q 让行尾空格/不可见字符可见),
// 而不是把整份产物的原始字节吐出来。
func reportFirstDiff(t *testing.T, want, got string) {
	t.Helper()

	wl := strings.Split(want, "\n")
	gl := strings.Split(got, "\n")

	minLen := len(wl)
	if len(gl) < minLen {
		minLen = len(gl)
	}
	idx := minLen // 默认: 一方是另一方的前缀, 差异落在长度边界
	for i := 0; i < minLen; i++ {
		if wl[i] != gl[i] {
			idx = i
			break
		}
	}

	maxLen := len(wl)
	if len(gl) > maxLen {
		maxLen = len(gl)
	}
	lo := idx - 3
	if lo < 0 {
		lo = 0
	}
	hi := idx + 4
	if hi > maxLen {
		hi = maxLen
	}

	var b strings.Builder
	fmt.Fprintf(&b, "快照失配: 首个差异在第 %d 行 (黄金文件 %d 行, 生成产物 %d 行)\n", idx+1, len(wl), len(gl))
	fmt.Fprintf(&b, "上下文 (>> 标记首个差异行):\n")
	for i := lo; i < hi; i++ {
		mark := "  "
		if i == idx {
			mark = ">>"
		}
		wv, gv := "<无此行>", "<无此行>"
		if i < len(wl) {
			wv = wl[i]
		}
		if i < len(gl) {
			gv = gl[i]
		}
		fmt.Fprintf(&b, "%s L%-4d 黄金: %q\n", mark, i+1, wv)
		fmt.Fprintf(&b, "   L%-4d 生成: %q\n", i+1, gv)
	}
	t.Error(b.String())
}

// syntheticConfig 用字面量构造覆盖矩阵所需的最小配置 (不读任何外部文件)。
// 每个被渲染 keymap 都遵守"一 hotkey 一 Action"以满足确定性约束 1;
// ID==1 的六个重映射用互异 WindowGroupID 0..5 以满足确定性约束 2。
func syntheticConfig() *Config {
	return &Config{
		Keymaps: []Keymap{
			// ID1: 全局 "Custom Hotkeys"。Enable && ID==1 => 触发 handleKeyRemapping
			// (TypeID5 -> .KeyMapping), 且被 Preprocess 注入 !f17。
			// 六个重映射 WindowGroupID 取 0..5 (互异, 约束2), 各挂独立 hotkey (约束1),
			// 顺带覆盖 hotifHeader 的 conditionType 0..5 全分支。
			{
				ID: 1, Name: "Custom Hotkeys", Enable: true,
				Hotkey: "customHotkeys", ParentID: 0, Delay: 0, DisableAt: "",
				Hotkeys: map[string][]Action{
					"a": {{TypeID: 5, RemapToKey: "b", WindowGroupID: 0}},
					"c": {{TypeID: 5, RemapToKey: "d", WindowGroupID: 1}},
					"e": {{TypeID: 5, RemapToKey: "f", WindowGroupID: 2}},
					"g": {{TypeID: 5, RemapToKey: "h", WindowGroupID: 3}},
					"i": {{TypeID: 5, RemapToKey: "j", WindowGroupID: 4}},
					"k": {{TypeID: 5, RemapToKey: "l", WindowGroupID: 5}},
				},
			},
			// ID2: capslock 缩写表 —— model.CapslockAbbr 靠 Hotkey=="capslockAbbr" 定位。
			// ID2 不会被 EnabledKeymaps 渲染 (只取 ID==1 或 ID>=5), 其 Hotkeys 经
			// AbbrRegistryCode 渲染。覆盖: 无守卫(wg0) / ct1 守卫 / ct5 去单引号 / 多动作。
			{
				ID: 2, Name: "Command", Enable: true,
				Hotkey: "capslockAbbr", ParentID: 0,
				Hotkeys: map[string][]Action{
					"jk":    {{TypeID: 6, KeysToSend: "{blind}jk", WindowGroupID: 0}},
					"web":   {{TypeID: 1, Target: "chrome.exe", WindowGroupID: 0}},
					"edit":  {{TypeID: 6, KeysToSend: "{blind}code", WindowGroupID: 1}},
					"expr":  {{TypeID: 8, AHKCode: `Run("calc.exe")`, WindowGroupID: 5}},
					"multi": {{TypeID: 2, ValueID: 1, WindowGroupID: 0}, {TypeID: 6, KeysToSend: "{enter}", WindowGroupID: 0}},
				},
			},
			// ID3: semicolon 缩写表 (Hotkey=="semicolonAbbr")。覆盖 ct2 (多行组 ahk_group) 守卫。
			{
				ID: 3, Name: "Abbreviation", Enable: true,
				Hotkey: "semicolonAbbr", ParentID: 0,
				Hotkeys: map[string][]Action{
					",":   {{TypeID: 6, KeysToSend: "{enter}", WindowGroupID: 0}},
					"sys": {{TypeID: 2, ValueID: 1, WindowGroupID: 2}},
				},
			},
			// ID5: 被渲染的模式, 覆盖 TypeID 1/2/3/4/5/6/7/8/9, 并承载缩写启用触发
			// (TypeID9 ValueID6/5)。单行 disableAt。每 hotkey 一 Action (约束1)。
			{
				ID: 5, Name: "CapsLock", Enable: true,
				Hotkey: "*CapsLock", ParentID: 0, Delay: 0, DisableAt: "ahk_exe steam.exe",
				Hotkeys: map[string][]Action{
					"*1": {{TypeID: 1, Target: "notepad.exe", WindowGroupID: 0}},
					"*2": {{TypeID: 2, ValueID: 1, WindowGroupID: 0}},
					"*3": {{TypeID: 3, ValueID: 1, WindowGroupID: 0}},
					"*4": {{TypeID: 3, ValueID: 4, WindowGroupID: 0}},
					"*5": {{TypeID: 4, ValueID: 1, WindowGroupID: 0}},
					"*6": {{TypeID: 4, ValueID: 5, WindowGroupID: 0}},
					"*7": {{TypeID: 6, KeysToSend: "hello\n{text}world", WindowGroupID: 0}},
					"*8": {{TypeID: 7, ValueID: 1, WindowGroupID: 0}},
					"*9": {{TypeID: 7, ValueID: 7, WindowGroupID: 0}},
					"*0": {{TypeID: 8, AHKCode: `MsgBox("hello")`, WindowGroupID: 0}},
					"*t": {{TypeID: 5, RemapToKey: "x", WindowGroupID: 0}},
					"*q": {{TypeID: 9, ValueID: 6, WindowGroupID: 0}},
					"*w": {{TypeID: 9, ValueID: 5, WindowGroupID: 0}},
					"*e": {{TypeID: 9, ValueID: 1, WindowGroupID: 0}},
					"*r": {{TypeID: 9, ValueID: 8, WindowGroupID: 0}},
				},
			},
			// ID6: 第二个被渲染模式 (中文名 -> 走 UTF-8), 带窗口组守卫 (ct1/ct2/ct5)、
			// 多行 disableAt (-> GROUP_DISABLE_KEYMAP)、ActivateOrRun 全参形态、
			// sendKeys 的 ahk:/sleep 分支。
			{
				ID: 6, Name: "媒体控制", Enable: true,
				Hotkey: "*F13", ParentID: 0, Delay: 0,
				DisableAt: "ahk_exe game1.exe\nahk_exe game2.exe",
				Hotkeys: map[string][]Action{
					"m1": {{TypeID: 1, WinTitle: "ahk_exe calc.exe", Target: "calc.exe", Args: "/auto", WorkingDir: `D:\tools`, RunAsAdmin: true, WindowGroupID: 1}},
					"m2": {{TypeID: 2, ValueID: 5, WindowGroupID: 2}},
					"m3": {{TypeID: 6, KeysToSend: "ahk:ToolTip(\"hi\")\nsleep 500\n{enter}", WindowGroupID: 0}},
					"m4": {{TypeID: 7, ValueID: 19, WindowGroupID: 5}},
					"m5": {{TypeID: 4, ValueID: 2, WindowGroupID: 1}},
					"m6": {{TypeID: 9, ValueID: 4, WindowGroupID: 0}},
					"m7": {{TypeID: 9, ValueID: 7, WindowGroupID: 0}},
				},
			},
		},
		Options: Options{
			// KeyfluxVersion 由 ldflags 注入, 测试环境为空; 实测 keyflux.tmpl 不渲染版本号, 保持默认空。
			KeyfluxVersion: "",
			// conditionType 1..5 各一; 含单行 value (直出) 与多行 value (ahk_group)。
			WindowGroups: []WindowGroup{
				{ID: 1, Name: "Editor", Value: "ahk_exe code.exe", ConditionType: 1},
				{ID: 2, Name: "Browsers", Value: "ahk_exe chrome.exe\nahk_exe firefox.exe", ConditionType: 2},
				{ID: 3, Name: "Explorer", Value: "ahk_class CabinetWClass", ConditionType: 3},
				{ID: 4, Name: "NoPhotoshop", Value: "ahk_exe photoshop.exe", ConditionType: 4},
				{ID: 5, Name: "CustomExpr", Value: `WinActive("A") && GetKeyState("Shift")`, ConditionType: 5},
			},
			Mouse: Mouse{
				KeepMouseMode: false, ShowTip: true, TipSymbol: "🐶",
				Delay1: "200", Delay2: "600",
				FastSingle: "6", FastRepeat: "60",
				SlowSingle: "30", SlowRepeat: "150",
			},
			Scroll: Scroll{Delay1: "200", Delay2: "600", OnceLineCount: "3"},
			// 普通值 + ahk-expression: 前缀值 (ToAHKFuncArg 原样输出分支) + 空名跳过分支。
			PathVariables: []PathVariable{
				{Name: "editor", Value: `D:\tools\edit.exe`},
				{Name: "desktop", Value: "ahk-expression: A_Desktop"},
				{Name: "", Value: "ignored-blank-name"},
				{Name: "   ", Value: "ignored-space-name"},
			},
		},
		// 选中动作 (方案 D 单键分发): textType/fileExt 两类匹配; entry 覆盖空 actionValue
		// (内置 ID 直通) 与显式 actionValue (展开后透传) 两种形态; 目录缺失时显示名回退 ID。
		SelectedAction: &SelectedAction{
			Hotkey: ">^p", Enable: true,
			Mappings: []SelectedMapping{
				{
					MatchType: "textType", MatchValue: "url",
					Entries: []SelectedEntry{
						{Behavior: "open_url", Options: RuleOptions{}},
						{Behavior: "search", ActionValue: "https://www.bing.com/search?q=%selected%", Options: RuleOptions{CopyToClipboard: true}},
					},
				},
				{
					MatchType: "fileExt", MatchValue: "jpg,png",
					Entries: []SelectedEntry{
						{Behavior: "open", ActionValue: "%selected%", Options: RuleOptions{Confirm: true}},
					},
				},
			},
		},
	}
}
