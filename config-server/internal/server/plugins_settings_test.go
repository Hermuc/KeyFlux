package server

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"

	"github.com/gin-gonic/gin"

	"settings/internal/plugins"
)

// withTempPluginDirs 把包级插件依赖指向临时目录, 测试结束自动还原。
// 生产路径是 ../data/* (相对 bin/), 直接跑会动到真实数据区, 故必须隔离。
func withTempPluginDirs(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	oldDir, oldStore := userPluginsDir, pluginSettings
	userPluginsDir = dir
	pluginSettings = plugins.NewSettingsStore(filepath.Join(dir, plugins.SettingsFileName))
	t.Cleanup(func() { userPluginsDir, pluginSettings = oldDir, oldStore })
	return dir
}

// seedPlugin 在临时目录里落一个带 settings 声明的插件包 (绕过 zip 导入, 直接建目录)。
func seedPlugin(t *testing.T, dir, id string, settings []plugins.Setting) *plugins.Manifest {
	t.Helper()
	m := &plugins.Manifest{
		ID: id, Name: "测试插件", SpecVersion: 1,
		Entry:       plugins.Entry{Kind: "script", File: "main.ahk", Func: "Main"},
		Permissions: []string{plugins.SettingsPermission},
		Settings:    settings,
	}
	raw, err := json.Marshal(m)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(dir, id), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, id, "plugin.json"), raw, 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, id, "main.ahk"), []byte("#Requires AutoHotkey v2.0\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	return m
}

func settingsRouter() *gin.Engine {
	gin.SetMode(gin.TestMode)
	r := gin.New()
	r.GET("/api/plugins/:id/settings", GetPluginSettingsHandler)
	r.PUT("/api/plugins/:id/settings", SavePluginSettingsHandler)
	return r
}

func callSettings(t *testing.T, method, id string, body any) (int, map[string]any) {
	t.Helper()
	var reader *bytes.Reader
	if body == nil {
		reader = bytes.NewReader(nil)
	} else {
		raw, err := json.Marshal(body)
		if err != nil {
			t.Fatal(err)
		}
		reader = bytes.NewReader(raw)
	}
	w := httptest.NewRecorder()
	req := httptest.NewRequest(method, "/api/plugins/"+id+"/settings", reader)
	req.Header.Set("Content-Type", "application/json")
	settingsRouter().ServeHTTP(w, req)
	var resp map[string]any
	if w.Body.Len() > 0 {
		if err := json.Unmarshal(w.Body.Bytes(), &resp); err != nil {
			t.Fatalf("响应非 JSON: %v (body=%s)", err, w.Body.String())
		}
	}
	return w.Code, resp
}

// limitMin/Max 简化 Setting 的指针边界写法。
func iptr(v float64) *float64 { return &v }

func demoPluginSettings() []plugins.Setting {
	return []plugins.Setting{
		{Key: "triggerKey", Type: plugins.SettingTypeChar, Label: "前置触发键", Default: " "},
		{Key: "everythingPath", Type: plugins.SettingTypeFile, Label: "Everything 路径", Filter: "everything.exe"},
		{Key: "esPath", Type: plugins.SettingTypeFile, Label: "es.exe 路径", Filter: "es.exe"},
		{Key: "limit", Type: plugins.SettingTypeNumber, Label: "结果条数上限", Default: "20", Min: iptr(1), Max: iptr(100)},
	}
}

func TestPluginSettings_Get_MergesDefaults(t *testing.T) {
	dir := withTempPluginDirs(t)
	seedPlugin(t, dir, "everything_search", demoPluginSettings())

	code, resp := callSettings(t, http.MethodGet, "everything_search", nil)
	if code != http.StatusOK {
		t.Fatalf("GET 应 200, 实际 %d (%v)", code, resp)
	}
	values, _ := resp["values"].(map[string]any)
	if len(values) != 4 {
		t.Fatalf("值表应含全部 4 个声明键: %v", values)
	}
	if values["triggerKey"] != " " || values["limit"] != "20" {
		t.Fatalf("未存过的键应回落默认值: %v", values)
	}
	if values["everythingPath"] != "" {
		t.Fatalf("无默认值的键应为空串: %v", values)
	}
	// 声明必须一并返回 (端点自洽: 调用方不必另外拉 manifest)
	if settings, _ := resp["settings"].([]any); len(settings) != 4 {
		t.Fatalf("响应应带 settings 声明: %v", resp["settings"])
	}
}

func TestPluginSettings_Put_RoundTripAndReGet(t *testing.T) {
	dir := withTempPluginDirs(t)
	seedPlugin(t, dir, "everything_search", demoPluginSettings())

	code, resp := callSettings(t, http.MethodPut, "everything_search", map[string]any{
		"values": map[string]string{
			"triggerKey":     ";",
			"everythingPath": `D:\Everything\everything.exe`,
			"limit":          "50",
		},
	})
	if code != http.StatusOK {
		t.Fatalf("PUT 应 200, 实际 %d (%v)", code, resp)
	}
	// PUT 的响应已是合并后的最新值表 (界面可直接采用, 无需再 GET)
	values, _ := resp["values"].(map[string]any)
	if values["triggerKey"] != ";" || values["limit"] != "50" {
		t.Fatalf("PUT 响应值不对: %v", values)
	}

	// 独立 GET 必须读到刚保存的值 (落盘成功)
	code, resp = callSettings(t, http.MethodGet, "everything_search", nil)
	if code != http.StatusOK {
		t.Fatalf("GET 应 200, 实际 %d", code)
	}
	values, _ = resp["values"].(map[string]any)
	if values["triggerKey"] != ";" || values["everythingPath"] != `D:\Everything\everything.exe` {
		t.Fatalf("二次 GET 读到的值与保存的不一致: %v", values)
	}

	// 落盘文件必须是引擎侧可读的扁平格式
	raw, err := os.ReadFile(filepath.Join(dir, plugins.SettingsFileName))
	if err != nil {
		t.Fatal(err)
	}
	var flat map[string]string
	if err := json.Unmarshal(raw, &flat); err != nil {
		t.Fatalf("落盘文件不是扁平字符串表: %v (%s)", err, raw)
	}
	if flat["everything_search:triggerKey"] != ";" {
		t.Fatalf("键空间应带插件前缀: %v", flat)
	}
}

func TestPluginSettings_Put_RejectsInvalid(t *testing.T) {
	dir := withTempPluginDirs(t)
	seedPlugin(t, dir, "everything_search", demoPluginSettings())

	cases := []struct {
		name string
		id   string
		body any
		want int
	}{
		{
			name: "插件不存在", id: "ghost",
			body: map[string]any{"values": map[string]string{"limit": "3"}},
			want: http.StatusNotFound,
		},
		{
			name: "未声明的键", id: "everything_search",
			body: map[string]any{"values": map[string]string{"evil": "1"}},
		},
		{
			name: "char 多字符", id: "everything_search",
			body: map[string]any{"values": map[string]string{"triggerKey": "abc"}},
		},
		{
			name: "char 控制字符", id: "everything_search",
			body: map[string]any{"values": map[string]string{"triggerKey": "\t"}},
		},
		{
			name: "number 非整数", id: "everything_search",
			body: map[string]any{"values": map[string]string{"limit": "abc"}},
		},
		{
			name: "number 越上限", id: "everything_search",
			body: map[string]any{"values": map[string]string{"limit": "101"}},
		},
		{
			name: "number 越下限", id: "everything_search",
			body: map[string]any{"values": map[string]string{"limit": "0"}},
		},
		{
			name: "file 超长", id: "everything_search",
			body: map[string]any{"values": map[string]string{"esPath": longString(2000)}},
		},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			code, _ := callSettings(t, http.MethodPut, tc.id, tc.body)
			if tc.want == 0 {
				tc.want = http.StatusBadRequest
			}
			if code != tc.want {
				t.Fatalf("期望 %d, 实际 %d", tc.want, code)
			}
		})
	}

	// 校验失败必须整单拒绝: 合法键也不得被写入 (半成功会让界面与服务端状态分叉)
	code, _ := callSettings(t, http.MethodPut, "everything_search", map[string]any{
		"values": map[string]string{"limit": "50", "triggerKey": "abc"},
	})
	if code != http.StatusBadRequest {
		t.Fatalf("混合非法输入应整单拒绝, 实际 %d", code)
	}
	code, resp := callSettings(t, http.MethodGet, "everything_search", nil)
	if code != http.StatusOK {
		t.Fatal(code)
	}
	if values, _ := resp["values"].(map[string]any); values["limit"] != "20" {
		t.Fatalf("被拒绝的请求不应留下任何写入: %v", values)
	}
}

func TestPluginSettings_Put_ClearsValueOnEmptyString(t *testing.T) {
	dir := withTempPluginDirs(t)
	seedPlugin(t, dir, "everything_search", demoPluginSettings())

	if code, _ := callSettings(t, http.MethodPut, "everything_search", map[string]any{
		"values": map[string]string{"triggerKey": "-"},
	}); code != http.StatusOK {
		t.Fatalf("首次保存失败: %d", code)
	}
	// 置空 = 清掉覆盖, 回落默认值 (空格)
	if code, resp := callSettings(t, http.MethodPut, "everything_search", map[string]any{
		"values": map[string]string{"triggerKey": ""},
	}); code != http.StatusOK {
		t.Fatalf("清空失败: %d", code)
	} else if values, _ := resp["values"].(map[string]any); values["triggerKey"] != " " {
		t.Fatalf("清空后应回落默认值: %v", values)
	}

	raw, err := os.ReadFile(filepath.Join(dir, plugins.SettingsFileName))
	if err != nil {
		t.Fatal(err)
	}
	if bytes.Contains(raw, []byte("everything_search:triggerKey")) {
		t.Fatalf("清空的键不应留在文件里: %s", raw)
	}
}

func TestPluginSettings_NoDeclaredSettings(t *testing.T) {
	dir := withTempPluginDirs(t)
	seedPlugin(t, dir, "bare_plugin", nil)

	code, resp := callSettings(t, http.MethodGet, "bare_plugin", nil)
	if code != http.StatusOK {
		t.Fatalf("无设置声明的插件 GET 应 200, 实际 %d", code)
	}
	if values, _ := resp["values"].(map[string]any); len(values) != 0 {
		t.Fatalf("无声明应得空值表: %v", values)
	}
	// 任何键都不得被写入
	if code, _ := callSettings(t, http.MethodPut, "bare_plugin", map[string]any{
		"values": map[string]string{"a": "1"},
	}); code != http.StatusBadRequest {
		t.Fatalf("未声明任何设置时写入应 400, 实际 %d", code)
	}
}

func longString(n int) string {
	b := make([]byte, n)
	for i := range b {
		b[i] = 'a'
	}
	return string(b)
}
