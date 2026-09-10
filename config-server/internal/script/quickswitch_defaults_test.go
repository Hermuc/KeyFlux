package script

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"regexp"
	"strconv"
	"strings"
	"testing"
)

// TestQuickSwitchDefaultsConsistency 守护 quickSwitch 默认值真源在 Go 与 AHK 两侧的一致性:
//   - config.go DefaultQuickSwitchOption() (ParseConfig 段全零时整体填充)
//   - bin/lib/quickswitch/QuickSwitch.ahk QuickSwitchDefaultConfig() (引擎内建默认值)
//
// 从 AHK 函数 return {...} 逐行正则提取 "key: value" 字面量, 与 Go 默认值逐字段比对,
// 9 字段全部相等 (缺漏/多出/不等即 fail), 防止改动一处漏改另一处导致默认值漂移。
// (C# 侧 Models/ConfigReadDefaults.cs QuickSwitchDefaults() 同为真源, 其一致由 C# 单测守护。)
func TestQuickSwitchDefaultsConsistency(t *testing.T) {
	const ahkFile = "../../../bin/lib/quickswitch/QuickSwitch.ahk"
	data, err := os.ReadFile(ahkFile)
	if err != nil {
		t.Fatalf("读取 %s 失败: %v", ahkFile, err)
	}

	want := DefaultQuickSwitchOption()
	wv := reflect.ValueOf(want)
	if wv.NumField() != 9 {
		t.Fatalf("DefaultQuickSwitchOption 字段数 = %d, 期望 9; 模型字段变更后须同步默认值表与 AHK 字面量", wv.NumField())
	}

	// 行格式: "    <key>: <value>," (末字段 excludedPrefixes 无尾逗号)。
	lineRe := regexp.MustCompile(`^(\w+)\s*:\s*(.+?)\s*,?\s*$`)
	got := map[string]string{}
	inFunc := false
	inBlock := false
	for lineNo, raw := range strings.Split(string(data), "\n") {
		line := strings.TrimSpace(strings.TrimRight(raw, "\r"))
		if !inFunc {
			// 定义行 QuickSwitchDefaultConfig() { (区别于调用行 c := QuickSwitchDefaultConfig())
			if strings.HasPrefix(line, "QuickSwitchDefaultConfig()") && strings.HasSuffix(line, "{") {
				inFunc = true
			}
			continue
		}
		if !inBlock {
			if line == "return {" {
				inBlock = true
			}
			continue
		}
		if line == "}" {
			break
		}
		if line == "" {
			continue
		}
		m := lineRe.FindStringSubmatch(line)
		if m == nil {
			t.Fatalf("%s 第 %d 行无法解析为 key: value: %q", ahkFile, lineNo+1, line)
		}
		got[m[1]] = m[2]
	}
	if len(got) == 0 {
		t.Fatalf("未能从 %s 提取 QuickSwitchDefaultConfig 字面量 (函数名/格式变更?)", ahkFile)
	}

	getFieldStr := func(v reflect.Value) string {
		switch v.Kind() {
		case reflect.Bool:
			return strconv.FormatBool(v.Bool())
		case reflect.Int:
			return strconv.Itoa(int(v.Int()))
		case reflect.Slice:
			if v.Len() == 0 {
				return "[]"
			}
			parts := make([]string, v.Len())
			for i := 0; i < v.Len(); i++ {
				parts[i] = strconv.Quote(v.Index(i).String())
			}
			return "[" + strings.Join(parts, ", ") + "]"
		default:
			return fmt.Sprintf("%v", v.Interface())
		}
	}

	for i := 0; i < wv.NumField(); i++ {
		field := wv.Type().Field(i)
		key := field.Tag.Get("json")
		if idx := strings.Index(key, ","); idx >= 0 {
			key = key[:idx]
		}
		if key == "" {
			key = strings.ToLower(field.Name[:1]) + field.Name[1:]
		}
		wantStr := getFieldStr(wv.Field(i))
		gotStr, ok := got[key]
		if !ok {
			t.Errorf("AHK QuickSwitchDefaultConfig 缺少字段 %s", key)
			continue
		}
		if gotStr != wantStr {
			t.Errorf("默认值漂移: %s: AHK=%q, config.go=%q", key, gotStr, wantStr)
		}
	}
	if len(got) != wv.NumField() {
		t.Errorf("AHK 字面量字段数 = %d, config.go 默认值字段数 = %d, 两处必须一一对应 (AHK=%v)",
			len(got), wv.NumField(), got)
	}
}

// TestQuickSwitchUpgradePathFillsDefaults 是 P1 缺陷的直接回归网:
// 以「不含 quickSwitch 段」的旧配置为输入 (升级路径), 断言
//  ① ParseConfig 经全零签名补齐出真默认值;
//  ② 渲染产物里 InitQuickSwitch({...}) 得到真默认值而非零值。
// 修复前: ParseConfig 无 QuickSwitch 分支 -> 全零透传 -> 模板无条件渲染 9 字段 -> 引擎侧功能全失效。
func TestQuickSwitchUpgradePathFillsDefaults(t *testing.T) {
	// 基底 = 合成配置 (可完整渲染), 序列化后删除 options.quickSwitch, 模拟"旧配置无该段"。
	base := syntheticConfig()
	raw, err := json.Marshal(base)
	if err != nil {
		t.Fatalf("序列化合成配置失败: %v", err)
	}
	var doc map[string]any
	if err := json.Unmarshal(raw, &doc); err != nil {
		t.Fatalf("反序列化合成配置失败: %v", err)
	}
	opts, ok := doc["options"].(map[string]any)
	if !ok {
		t.Fatalf("合成配置序列化后缺少 options 对象")
	}
	if _, has := opts["quickSwitch"]; !has {
		t.Fatalf("前置条件不成立: 合成配置应当含 quickSwitch 段")
	}
	delete(opts, "quickSwitch")
	stripped, err := json.Marshal(doc)
	if err != nil {
		t.Fatalf("重序列化去段配置失败: %v", err)
	}
	if strings.Contains(string(stripped), "quickSwitch") {
		t.Fatalf("去段失败: 序列化结果仍含 quickSwitch")
	}

	cfgPath := filepath.Join(t.TempDir(), "legacy-config.json")
	if err := os.WriteFile(cfgPath, stripped, 0644); err != nil {
		t.Fatalf("写入临时配置失败: %v", err)
	}

	cfg, err := ParseConfig(cfgPath)
	if err != nil {
		t.Fatalf("ParseConfig 失败: %v", err)
	}
	if !reflect.DeepEqual(cfg.Options.QuickSwitch, DefaultQuickSwitchOption()) {
		t.Errorf("升级路径: ParseConfig 未补齐默认值\n got = %+v\nwant = %+v",
			cfg.Options.QuickSwitch, DefaultQuickSwitchOption())
	}

	Preprocess(cfg) // 与 SaveAHK 同序 (同 generateAHK 管线)
	outPath := filepath.Join(t.TempDir(), "KeyFlux.ahk")
	if err := SaveAHK(cfg, goldenTemplate, outPath); err != nil {
		t.Fatalf("SaveAHK 失败: %v", err)
	}
	outRaw, err := os.ReadFile(outPath)
	if err != nil {
		t.Fatalf("读取生成产物失败: %v", err)
	}
	out := normalizeAHK(string(outRaw))

	wantLine := `InitQuickSwitch({collectEnabled: true, autoShow: true, autoJumpOpen: true, autoJumpSave: false, pollIntervalMs: 800, maxHistory: 200, overlayRows: 8, overlayRowsCompact: 4, excludedPrefixes: []})`
	if !strings.Contains(out, wantLine) {
		t.Errorf("升级路径: 生成脚本未包含真默认值注入行\n期望包含: %s", wantLine)
	}
	if strings.Contains(out, "pollIntervalMs: 0") {
		t.Errorf("升级路径: 生成脚本出现零值注入 (pollIntervalMs: 0), 说明默认值未补齐")
	}
}
