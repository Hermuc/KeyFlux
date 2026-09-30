package script

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

// atomicFixtureConfig 是「落盘字节不变」的固定字面量夹具, 覆盖序列化关心的全部要点:
//   - HTML 特殊字符 (<>&) 证明 SetEscapeHTML(false) 未丢 (丢了会被转成 \u003c 等);
//   - 中文/emoji 证明非 ASCII 原样输出;
//   - 嵌套数组/对象证明 2 空格缩进与 Encoder 自带尾换行未变。
func atomicFixtureConfig() *Config {
	return &Config{
		Keymaps: []Keymap{{
			ID:        1,
			Name:      `网页<>&"引号"`,
			Enable:    true,
			Hotkey:    "^!a",
			Delay:     200,
			DisableAt: `C:\Tools\a&b.exe`,
			Hotkeys:   map[string][]Action{"a": {{WindowGroupID: 1, TypeID: 5, Comment: "重映射 🔥"}}},
		}},
		Options: Options{
			HideMatrix: true,
			Mouse:      Mouse{TipSymbol: "🐶"},
			CommandFont: CommandFontOption{
				SourcePath: `C:\f<ont>.ttf`,
				Weight:     "bold",
			},
		},
		SelectedAction: &SelectedAction{
			Hotkey: "^!s",
			Enable: true,
			Mappings: []SelectedMapping{{
				MatchType:  "fileExt",
				MatchValue: "jpg,png",
				Entries:    []SelectedEntry{{Behavior: "copy"}},
			}},
		},
		FileGroups: []FileGroup{{Name: "img", Label: "图片<>&", Exts: []string{"jpg", "png"}}},
		MatchTypes: []MatchType{{
			ID:    "netdisk",
			Label: "网盘<&>",
			Kind:  "text",
			Rules: []MatchRule{{Op: "contains", Value: "pan.baidu.com"}},
		}},
		OverviewDocMd: "# 使用指南\n\n编辑 <b>配置</b> & 保存\n",
	}
}

// legacySaveBytes 复刻改动前的 SaveConfigFile 编码口径 (Encode + SetIndent + SetEscapeHTML(false)),
// 作为「原子化后落盘字节仍逐字节一致」的基准。
func legacySaveBytes(t *testing.T, config *Config) []byte {
	t.Helper()
	buf := new(bytes.Buffer)
	encoder := json.NewEncoder(buf)
	encoder.SetIndent("", "  ")
	encoder.SetEscapeHTML(false)
	if err := encoder.Encode(config); err != nil {
		t.Fatalf("旧实现编码失败: %v", err)
	}
	return buf.Bytes()
}

// ① 原子写后字节与旧实现逐字节一致。
func TestSaveConfigFileBytesMatchLegacyImplementation(t *testing.T) {
	path := filepath.Join(t.TempDir(), "config.json")
	config := atomicFixtureConfig()
	legacy := legacySaveBytes(t, config)

	if err := saveConfigFileTo(config, path); err != nil {
		t.Fatalf("原子写失败: %v", err)
	}
	got, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("读回落盘文件失败: %v", err)
	}
	if !bytes.Equal(got, legacy) {
		t.Fatalf("落盘字节与旧实现不一致\n--- got ---\n%s\n--- want ---\n%s", got, legacy)
	}
	// 显式锚点: 上游若改 json 默认行为 (HTML 转义/缩进/尾换行), 上面的 Equal 会失败,
	// 这里给出人可读的失败原因定位。
	if !bytes.Contains(got, []byte("<b>配置</b> & 保存")) {
		t.Fatalf("HTML 特殊字符被转义 (SetEscapeHTML(false) 丢失):\n%s", got)
	}
	if !bytes.HasSuffix(got, []byte("}\n")) {
		t.Fatalf("落盘内容缺少尾换行:\n%s", got)
	}
	if !bytes.Contains(got, []byte("\n  \"keymaps\"")) {
		t.Fatalf("缩进不是 2 空格:\n%s", got)
	}
}

// ② 写入成功后不残留 *.tmp (临时文件被 rename 消费或被 defer 清理)。
func TestSaveConfigFileLeavesNoTempFiles(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "config.json")

	if err := saveConfigFileTo(atomicFixtureConfig(), path); err != nil {
		t.Fatalf("原子写失败: %v", err)
	}

	leaked, err := filepath.Glob(filepath.Join(dir, "*.tmp"))
	if err != nil {
		t.Fatalf("glob 失败: %v", err)
	}
	if len(leaked) != 0 {
		t.Fatalf("残留临时文件: %v", leaked)
	}
	// 更强的断言: 目录内只应有目标文件本身
	entries, err := os.ReadDir(dir)
	if err != nil {
		t.Fatalf("读目录失败: %v", err)
	}
	if len(entries) != 1 || entries[0].Name() != "config.json" {
		t.Fatalf("目标目录内容异常 (泄漏): %v", entries)
	}
}

// ③ 覆盖已存在文件成功, 且内容为新配置 (旧内容不残留)。
func TestSaveConfigFileOverwritesExistingFile(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "config.json")
	if err := os.WriteFile(path, []byte("旧内容应被整体替换"), 0o644); err != nil {
		t.Fatalf("预置旧文件失败: %v", err)
	}

	config := atomicFixtureConfig()
	if err := saveConfigFileTo(config, path); err != nil {
		t.Fatalf("覆盖已存在文件失败: %v", err)
	}

	got, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("读回落盘文件失败: %v", err)
	}
	if !bytes.Equal(got, legacySaveBytes(t, config)) {
		t.Fatalf("覆盖后字节异常:\n%s", got)
	}
	if bytes.Contains(got, []byte("旧内容")) {
		t.Fatalf("旧内容残留 (非整体替换):\n%s", got)
	}
	leaked, _ := filepath.Glob(filepath.Join(dir, "*.tmp"))
	if len(leaked) != 0 {
		t.Fatalf("覆盖写残留临时文件: %v", leaked)
	}
}

// 目标目录不存在时返回错误而非 panic (旧实现 os.WriteFile 亦返回错误, 由调用方 panic)。
func TestSaveConfigFileToReturnsErrorWhenDirMissing(t *testing.T) {
	path := filepath.Join(t.TempDir(), "missing", "config.json")
	if err := saveConfigFileTo(atomicFixtureConfig(), path); err == nil {
		t.Fatal("目标目录不存在时应返回错误")
	}
}
