package server

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/gin-gonic/gin"
	"settings/internal/script/model"
)

// newTestRouter 构造仅注册 selected-action/test 路由的测试引擎 (不触磁盘配置)。
func newTestRouter() *gin.Engine {
	gin.SetMode(gin.TestMode)
	r := gin.New()
	r.POST("/api/selected-action/test", TestSelectedActionHandler)
	return r
}

func postTest(t *testing.T, body interface{}) (int, map[string]interface{}) {
	t.Helper()
	raw, err := json.Marshal(body)
	if err != nil {
		t.Fatalf("序列化请求失败: %v", err)
	}
	w := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/api/selected-action/test", bytes.NewReader(raw))
	req.Header.Set("Content-Type", "application/json")
	newTestRouter().ServeHTTP(w, req)
	var resp map[string]interface{}
	if err := json.Unmarshal(w.Body.Bytes(), &resp); err != nil {
		t.Fatalf("响应非 JSON: %v (body=%s)", err, w.Body.String())
	}
	return w.Code, resp
}

func testSnapshot() *model.SelectedAction {
	return &model.SelectedAction{
		Hotkey: ">^p", Enable: true,
		Mappings: []model.SelectedMapping{
			{MatchType: "textType", MatchValue: "url", Entries: []model.SelectedEntry{
				{Behavior: "open_url"},
				{Behavior: "search", ActionValue: "https://b.com?q=%selected%"},
			}},
			{MatchType: "fileExt", MatchValue: "jpg,png", Entries: []model.SelectedEntry{
				{Behavior: "open", ActionValue: "%selected%"},
			}},
		},
	}
}

// TestSelectedActionTestHit 命中 + 菜单序号 + 预览 (菜单 key 从 1 起, preview 取第一项展开)。
func TestSelectedActionTestHit(t *testing.T) {
	code, resp := postTest(t, map[string]interface{}{
		"content":        "https://example.com",
		"isFile":         false,
		"selectedAction": testSnapshot(),
	})
	if code != http.StatusOK {
		t.Fatalf("期望 200, got %d: %v", code, resp)
	}
	if resp["matched"] != true {
		t.Fatalf("应命中: %v", resp)
	}
	if resp["matchType"] != "textType" || resp["matchValue"] != "url" {
		t.Fatalf("matchType/matchValue 错误: %v", resp)
	}
	menu, ok := resp["menu"].([]interface{})
	if !ok || len(menu) != 2 {
		t.Fatalf("menu 应为 2 项: %v", resp["menu"])
	}
	first := menu[0].(map[string]interface{})
	if first["key"] != float64(1) || first["behavior"] != "open_url" {
		t.Fatalf("菜单第一项 key/behavior 错误: %v", first)
	}
	// name: 测试环境行为目录为空, 显示名回退 behavior ID
	if first["name"] != "open_url" {
		t.Fatalf("显示名应回退 behavior ID: %v", first)
	}
	if second := menu[1].(map[string]interface{}); second["key"] != float64(2) || second["behavior"] != "search" {
		t.Fatalf("菜单第二项 key/behavior 错误: %v", second)
	}
	// preview: open_url 预览语义 (PreviewAction)
	if p, _ := resp["preview"].(string); p == "" || !bytes.Contains([]byte(p), []byte("https://example.com")) {
		t.Fatalf("preview 应基于选中内容生成: %v", resp["preview"])
	}
}

// TestSelectedActionTestFileHit 文件路径命中 fileExt mapping。
func TestSelectedActionTestFileHit(t *testing.T) {
	code, resp := postTest(t, map[string]interface{}{
		"content":        `C:\pic\a.jpg`,
		"isFile":         true,
		"selectedAction": testSnapshot(),
	})
	if code != http.StatusOK || resp["matched"] != true {
		t.Fatalf("应命中 fileExt mapping: %d %v", code, resp)
	}
	if resp["matchType"] != "fileExt" || resp["matchValue"] != "jpg,png" {
		t.Fatalf("matchType/matchValue 错误: %v", resp)
	}
}

// TestSelectedActionTestMiss 未命中时响应 {"matched": false}。
func TestSelectedActionTestMiss(t *testing.T) {
	code, resp := postTest(t, map[string]interface{}{
		"content":        "just plain text",
		"isFile":         false,
		"selectedAction": testSnapshot(),
	})
	if code != http.StatusOK {
		t.Fatalf("期望 200, got %d", code)
	}
	if resp["matched"] != false {
		t.Fatalf("纯文本不应命中 url mapping: %v", resp)
	}
	if _, ok := resp["menu"]; ok {
		t.Fatalf("未命中不应携带 menu: %v", resp)
	}
}

// TestSelectedActionTestSnapshotPriority 快照优先且不落盘:
// 测试 cwd (internal/server) 下 ../data/config.json 不存在, 若 handler 回退磁盘会读文件失败,
// 用例通过本身即证明快照优先; handler 无任何写盘路径。
func TestSelectedActionTestSnapshotPriority(t *testing.T) {
	snapshot := testSnapshot()
	snapshot.Hotkey = "^un_saved" // 未保存的编辑态热键, 磁盘上不可能存在
	code, resp := postTest(t, map[string]interface{}{
		"content":        "https://example.com",
		"isFile":         false,
		"selectedAction": snapshot,
	})
	if code != http.StatusOK || resp["matched"] != true {
		t.Fatalf("编辑中快照应可直接测试: %d %v", code, resp)
	}
}

// TestSelectedActionTestBadRequest 非法组合拒绝测试 (与保存行为一致, 400)。
func TestSelectedActionTestBadRequest(t *testing.T) {
	// 空 entries mapping
	sa := testSnapshot()
	sa.Mappings[0].Entries = []model.SelectedEntry{}
	code, resp := postTest(t, map[string]interface{}{
		"content": "https://example.com", "isFile": false, "selectedAction": sa,
	})
	if code != http.StatusBadRequest {
		t.Fatalf("空 entries 应 400, got %d: %v", code, resp)
	}
	if msg, _ := resp["message"].(string); msg == "" {
		t.Fatalf("400 应携带 message: %v", resp)
	}
	// 启用 + 空热键
	sa2 := &model.SelectedAction{Hotkey: "", Enable: true}
	code, _ = postTest(t, map[string]interface{}{
		"content": "https://example.com", "isFile": false, "selectedAction": sa2,
	})
	if code != http.StatusBadRequest {
		t.Fatalf("启用+空热键应 400, got %d", code)
	}
}
