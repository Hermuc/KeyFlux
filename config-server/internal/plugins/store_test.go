package plugins

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// ---------------- manifest settings 校验 ----------------

func TestValidateManifest_Settings(t *testing.T) {
	// 带 settings 的合法基线 (必须同时申请 settings 权限)
	withSettings := func() *Manifest {
		m := validManifest()
		m.Permissions = []string{"selection", SettingsPermission}
		m.Settings = []Setting{
			{Key: "triggerKey", Type: SettingTypeChar, Label: "前置触发键", Default: " "},
			{Key: "everythingPath", Type: SettingTypeFile, Label: "Everything 路径", Filter: "everything.exe"},
			{Key: "limit", Type: SettingTypeNumber, Label: "上限", Default: "20", Min: f(1), Max: f(100)},
			{Key: "note", Type: SettingTypeText, Label: "备注", MaxLength: 32},
		}
		return m
	}
	if err := ValidateManifest(withSettings()); err != nil {
		t.Fatalf("合法 settings 被拒绝: %v", err)
	}

	cases := []struct {
		name   string
		mutate func(*Manifest)
		want   string
	}{
		{"未申请 settings 权限", func(m *Manifest) { m.Permissions = nil }, "未申请"},
		{"key 非法", func(m *Manifest) { m.Settings[0].Key = "1bad" }, "不合法"},
		{"key 含冒号 (会污染键空间)", func(m *Manifest) { m.Settings[0].Key = "a:b" }, "不合法"},
		{"key 重复", func(m *Manifest) { m.Settings[1].Key = m.Settings[0].Key }, "重复"},
		{"type 非法", func(m *Manifest) { m.Settings[0].Type = "toggle" }, "type"},
		{"缺 label", func(m *Manifest) { m.Settings[0].Label = "  " }, "label"},
		{"min > max", func(m *Manifest) { m.Settings[2].Min = f(200) }, "min 大于 max"},
		{"non-number 带 min", func(m *Manifest) { m.Settings[0].Min = f(1) }, "不应带 min/max"},
		{"non-file 带 filter", func(m *Manifest) { m.Settings[0].Filter = "x" }, "不应带 filter"},
		{"number 边界非整数", func(m *Manifest) { m.Settings[2].Max = f(10.5) }, "必须是整数"},
		{"默认值越界", func(m *Manifest) { m.Settings[2].Default = "999" }, "默认值不合法"},
		{"char 默认值是控制字符", func(m *Manifest) { m.Settings[0].Default = "\t" }, "默认值不合法"},
		{"file 默认值超长", func(m *Manifest) { m.Settings[1].Default = strings.Repeat("a", 2000) }, "默认值不合法"},
		{"设置项过多", func(m *Manifest) {
			m.Settings = make([]Setting, MaxSettingsPerPlugin+1)
			for i := range m.Settings {
				m.Settings[i] = Setting{Key: "k" + string(rune('a'+i%26)), Type: SettingTypeText, Label: "x"}
			}
		}, "设置项过多"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			m := withSettings()
			tc.mutate(m)
			err := ValidateManifest(m)
			if err == nil || !strings.Contains(err.Error(), tc.want) {
				t.Fatalf("期望错误含 %q, 实际: %v", tc.want, err)
			}
		})
	}
}

func TestValidateSettingValue(t *testing.T) {
	char := Setting{Key: "k", Type: SettingTypeChar, Label: "x"}
	text := Setting{Key: "k", Type: SettingTypeText, Label: "x", MaxLength: 4}
	num := Setting{Key: "k", Type: SettingTypeNumber, Label: "x", Min: f(1), Max: f(100)}
	numNoBound := Setting{Key: "k", Type: SettingTypeNumber, Label: "x"}
	file := Setting{Key: "k", Type: SettingTypeFile, Label: "x"}

	ok := []struct {
		s Setting
		v string
	}{
		{char, ""}, {char, " "}, {char, "a"}, {char, "中"}, {char, "-"},
		{text, ""}, {text, "abcd"},
		{num, ""}, {num, "20"}, {num, "1"}, {num, "100"},
		{numNoBound, "-5"},
		{file, `D:\PortableApps\Everything\everything.exe`},
	}
	for _, tc := range ok {
		if err := ValidateSettingValue(tc.s, tc.v); err != nil {
			t.Fatalf("%q 应合法: %v", tc.v, err)
		}
	}

	bad := []struct {
		s Setting
		v string
	}{
		{char, "ab"},     // 长度
		{char, "\n"},     // 控制字符
		{char, "\x7f"},   // DEL
		{text, "abcde"},  // 超 MaxLength
		{num, "abc"},     // 非整数
		{num, "20.0"},    // 小数
		{num, "0"},       // 低于下限
		{num, "101"},     // 高于上限
		{file, "a\x00b"}, // NUL
	}
	for _, tc := range bad {
		if err := ValidateSettingValue(tc.s, tc.v); err == nil {
			t.Fatalf("%q 应被拒绝", tc.v)
		}
	}
}

// ---------------- SettingsStore ----------------

func TestSettingsStore_RoundTripAndFormat(t *testing.T) {
	path := filepath.Join(t.TempDir(), SettingsFileName)
	st := NewSettingsStore(path)

	// 缺文件 = 空表, 不报错
	if got := st.LoadFor("p1"); len(got) != 0 {
		t.Fatalf("缺文件应得空表: %+v", got)
	}

	if err := st.Save("p1", map[string]string{
		"triggerKey":     " ",
		"everythingPath": `D:\Everything\everything.exe`,
		"limit":          "20",
	}); err != nil {
		t.Fatalf("保存失败: %v", err)
	}
	got := st.LoadFor("p1")
	want := map[string]string{
		"triggerKey":     " ",
		"everythingPath": `D:\Everything\everything.exe`,
		"limit":          "20",
	}
	if len(got) != len(want) {
		t.Fatalf("键数不一致: %+v", got)
	}
	for k, v := range want {
		if got[k] != v {
			t.Fatalf("键 %q 期望 %q 实际 %q", k, v, got[k])
		}
	}

	// 文件格式必须与 AHK ConfigProvider 同构: 顶层是对象, 键含 "<id>:" 前缀, 值为字符串
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		t.Fatalf("输出不是合法 JSON: %v\n%s", err, data)
	}
	for _, k := range []string{"p1:triggerKey", "p1:everythingPath", "p1:limit"} {
		if _, ok := raw[k]; !ok {
			t.Fatalf("缺少前缀键 %q: %s", k, data)
		}
	}
	// 关键: 值里不得出现 \uXXXX (AHK 的 _Unescape 不认这种转义)
	if strings.Contains(string(data), `\u`) {
		t.Fatalf("输出含 \\uXXXX 转义, AHK 读不回: %s", data)
	}
	// 反斜杠路径必须写成 \\ (AHK _Unescape 认识的那种)
	if !strings.Contains(string(data), `D:\\Everything\\everything.exe`) {
		t.Fatalf("路径反斜杠未按 \\\\ 转义: %s", data)
	}
}

func TestSettingsStore_PartialUpdatePreservesOthers(t *testing.T) {
	path := filepath.Join(t.TempDir(), SettingsFileName)
	st := NewSettingsStore(path)

	if err := st.Save("p1", map[string]string{"a": "1", "b": "2"}); err != nil {
		t.Fatal(err)
	}
	if err := st.Save("p2", map[string]string{"a": "9"}); err != nil {
		t.Fatal(err)
	}
	// 只改 p1 的 a: b 与 p2 必须原样保留
	if err := st.Save("p1", map[string]string{"a": "10"}); err != nil {
		t.Fatal(err)
	}
	if got := st.LoadFor("p1"); got["a"] != "10" || got["b"] != "2" {
		t.Fatalf("部分更新破坏了其它键: %+v", got)
	}
	if got := st.LoadFor("p2"); got["a"] != "9" {
		t.Fatalf("跨插件污染: %+v", got)
	}
	// 空串 = 删除
	if err := st.Save("p1", map[string]string{"b": ""}); err != nil {
		t.Fatal(err)
	}
	if got := st.LoadFor("p1"); len(got) != 1 || got["a"] != "10" {
		t.Fatalf("空值应删除键: %+v", got)
	}
}

func TestSettingsStore_PreservesForeignEntries(t *testing.T) {
	path := filepath.Join(t.TempDir(), SettingsFileName)
	// 模拟外人手写 / 旧版本留下的内容: 非字符串值 + 无冒号的键
	seed := `{"p1:a":"1","weird":123,"flag":true,"arr":[1,2]}`
	if err := os.WriteFile(path, []byte(seed), 0o644); err != nil {
		t.Fatal(err)
	}
	st := NewSettingsStore(path)
	if got := st.LoadFor("p1"); got["a"] != "1" {
		t.Fatalf("读不到既有字符串值: %+v", got)
	}
	if err := st.Save("p2", map[string]string{"b": "2"}); err != nil {
		t.Fatal(err)
	}
	data, _ := os.ReadFile(path)
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		t.Fatalf("输出非法 JSON: %v", err)
	}
	for _, k := range []string{"weird", "flag", "arr", "p1:a", "p2:b"} {
		if _, ok := raw[k]; !ok {
			t.Fatalf("写回时丢了外来键 %q: %s", k, data)
		}
	}
}

func TestSettingsStore_CorruptFileRecovers(t *testing.T) {
	path := filepath.Join(t.TempDir(), SettingsFileName)
	for _, seed := range []string{"not json at all", "[1,2,3]", "", "   "} {
		if err := os.WriteFile(path, []byte(seed), 0o644); err != nil {
			t.Fatal(err)
		}
		st := NewSettingsStore(path)
		if got := st.LoadFor("p1"); len(got) != 0 {
			t.Fatalf("坏文件应得空表 (seed=%q): %+v", seed, got)
		}
		if err := st.Save("p1", map[string]string{"a": "1"}); err != nil {
			t.Fatalf("坏文件应可被覆盖恢复 (seed=%q): %v", seed, err)
		}
		if got := st.LoadFor("p1"); got["a"] != "1" {
			t.Fatalf("恢复后读不到值 (seed=%q): %+v", seed, got)
		}
	}
}

func TestSettingsStore_AtomicWriteLeavesNoTemp(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, SettingsFileName)
	st := NewSettingsStore(path)
	for i := 0; i < 5; i++ {
		if err := st.Save("p1", map[string]string{"a": "1"}); err != nil {
			t.Fatal(err)
		}
	}
	entries, err := os.ReadDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 1 || entries[0].Name() != SettingsFileName {
		names := make([]string, 0, len(entries))
		for _, e := range entries {
			names = append(names, e.Name())
		}
		t.Fatalf("目录应只有设置文件, 实际: %v", names)
	}
}

func TestSettingsStore_RejectsBadPluginID(t *testing.T) {
	st := NewSettingsStore(filepath.Join(t.TempDir(), SettingsFileName))
	for _, id := range []string{"", "Bad", "../evil", "a:b", "quick switch"} {
		if err := st.Save(id, map[string]string{"a": "1"}); err == nil {
			t.Fatalf("非法 ID %q 应被拒绝", id)
		}
	}
}

func TestSettingsStore_MissingDirIsCreated(t *testing.T) {
	path := filepath.Join(t.TempDir(), "data", "nested", SettingsFileName)
	st := NewSettingsStore(path)
	if err := st.Save("p1", map[string]string{"a": "1"}); err != nil {
		t.Fatalf("应自动建目录: %v", err)
	}
	if _, err := os.Stat(path); err != nil {
		t.Fatal(err)
	}
}

// f 是 *float64 的构造糖 (Setting.Min/Max 用指针表达「不限」)。
func f(v float64) *float64 { return &v }
