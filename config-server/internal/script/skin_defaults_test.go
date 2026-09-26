package script

import (
	"os"
	"reflect"
	"regexp"
	"strings"
	"testing"
)

// TestCommandInputSkinDefaultsConsistency 守护两处默认值真源的一致性:
//   - config.go DefaultCommandInputSkin() (ParseConfig 皮肤全空时整体填充)
//   - templates/CommandInputSkin.tmpl 头部 else 兜底 (单字段为空时的兜底)
//
// 从模板逐行正则提取 else 兜底字面量, 与 DefaultCommandInputSkin 逐字段比对,
// 18 字段全部相等 (缺漏/多出/不等即 fail), 防止改动一处漏改另一处导致默认值漂移。
func TestCommandInputSkinDefaultsConsistency(t *testing.T) {
	const templateFile = "../../templates/CommandInputSkin.tmpl"
	data, err := os.ReadFile(templateFile)
	if err != nil {
		t.Fatalf("读取 %s 失败: %v", templateFile, err)
	}

	want := DefaultCommandInputSkin()
	wantValue := reflect.ValueOf(want)
	if wantValue.NumField() != 18 {
		t.Fatalf("DefaultCommandInputSkin 字段数 = %d, 期望 18; 模型字段变更后须同步默认值表与模板兜底", wantValue.NumField())
	}

	// 行格式: jsonName = {{ if .Options.CommandInputSkin.Field }}{{ .Options.CommandInputSkin.Field }}{{ else }}VALUE{{ end }}
	// m[1]=json 键名, m[2]=if 引用字段, m[3]=输出引用字段, m[4]=else 兜底字面量
	lineRe := regexp.MustCompile(`^(\w+)\s*=\s*\{\{ if \.Options\.CommandInputSkin\.(\w+)\s*\}\}\{\{ \.Options\.CommandInputSkin\.(\w+)\s*\}\}\{\{ else \}\}(\S+)\{\{ end \}\}$`)

	got := map[string]string{}
	for lineNo, line := range strings.Split(string(data), "\n") {
		line = strings.TrimSpace(strings.TrimRight(line, "\r"))
		if line == "" || strings.HasPrefix(line, "{{/*") {
			continue // 跳过空行与模板注释行
		}
		m := lineRe.FindStringSubmatch(line)
		if m == nil {
			t.Errorf("%s 第 %d 行不符合预期格式, 无法提取 else 兜底字面量: %q", templateFile, lineNo+1, line)
			continue
		}
		if m[2] != m[3] {
			t.Errorf("%s 第 %d 行 if 与输出引用的字段不一致: %s vs %s", templateFile, lineNo+1, m[2], m[3])
		}
		got[m[1]] = m[4]
	}

	for i := 0; i < wantValue.NumField(); i++ {
		field := wantValue.Type().Field(i)
		// json 键名 = Go 字段名首字母小写 (CommandInputSkin 的 json tag 均满足此规律)
		jsonName := strings.ToLower(field.Name[:1]) + field.Name[1:]
		gotVal, ok := got[jsonName]
		if !ok {
			t.Errorf("模板缺少字段 %s 的 else 兜底行", jsonName)
			continue
		}
		if gotVal != wantValue.Field(i).String() {
			t.Errorf("默认值漂移: %s: 模板 else 兜底 = %q, config.go 默认值 = %q", jsonName, gotVal, wantValue.Field(i).String())
		}
	}
	if len(got) != wantValue.NumField() {
		t.Errorf("模板 else 兜底行数 = %d, config.go 默认值字段数 = %d, 两处必须一一对应", len(got), wantValue.NumField())
	}
}
