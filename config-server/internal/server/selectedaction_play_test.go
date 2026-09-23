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
)

// newPlayRouter 构造仅注册 selected-action/play 路由的测试引擎 (不触磁盘配置)。
func newPlayRouter() *gin.Engine {
	gin.SetMode(gin.TestMode)
	r := gin.New()
	r.POST("/api/selected-action/play", PlaySelectedActionHandler)
	return r
}

// playPost 发请求并返回状态码 + 解析后的请求文件内容 (若存在)。
func playPost(t *testing.T, body map[string]string) (int, map[string]interface{}) {
	t.Helper()
	raw, err := json.Marshal(body)
	if err != nil {
		t.Fatalf("序列化请求失败: %v", err)
	}
	w := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/api/selected-action/play", bytes.NewReader(raw))
	req.Header.Set("Content-Type", "application/json")
	newPlayRouter().ServeHTTP(w, req)

	var resp map[string]interface{}
	if w.Body.Len() > 0 {
		if err := json.Unmarshal(w.Body.Bytes(), &resp); err != nil {
			t.Fatalf("响应非 JSON: %v (body=%s)", err, w.Body.String())
		}
	}
	return w.Code, resp
}

// 读取轮询请求文件并解析 (不存在返回 nil)。
func readPlayFile(t *testing.T) map[string]interface{} {
	t.Helper()
	p := filepath.Join(os.TempDir(), playRequestFile)
	raw, err := os.ReadFile(p)
	if err != nil {
		return nil
	}
	var m map[string]interface{}
	if err := json.Unmarshal(raw, &m); err != nil {
		t.Fatalf("请求文件非 JSON: %v (body=%s)", err, string(raw))
	}
	return m
}

// TestPlay_BuiltinTextFeature_WritesFile 合法内置特征值: 200 + 请求文件含正确 typeId/seq。
func TestPlay_BuiltinTextFeature_WritesFile(t *testing.T) {
	p := filepath.Join(os.TempDir(), playRequestFile)
	defer os.Remove(p)

	code, resp := playPost(t, map[string]string{"typeId": "url"})
	if code != http.StatusOK {
		t.Fatalf("期望 200, got %d: %v", code, resp)
	}
	if resp["ok"] != true {
		t.Fatalf("应返回 ok:true: %v", resp)
	}
	file := readPlayFile(t)
	if file == nil {
		t.Fatalf("应写入请求文件 %s", p)
	}
	if file["typeId"] != "url" {
		t.Fatalf("typeId 应为 url: %v", file)
	}
	seq, ok := file["seq"].(float64)
	if !ok || seq <= 0 {
		t.Fatalf("seq 应为正整数: %v", file)
	}
	// 落点必须是 %TEMP%
	if info, err := os.Stat(p); err != nil || info.IsDir() {
		t.Fatalf("请求文件应位于 %s: %v", p, err)
	}
}

// TestPlay_PlainFeature_WritesFile 兜底文本特征 plain 同样放行并写文件。
func TestPlay_PlainFeature_WritesFile(t *testing.T) {
	p := filepath.Join(os.TempDir(), playRequestFile)
	defer os.Remove(p)

	code, _ := playPost(t, map[string]string{"typeId": "plain"})
	if code != http.StatusOK {
		t.Fatalf("期望 200, got %d", code)
	}
	if readPlayFile(t) == nil {
		t.Fatalf("应写入请求文件")
	}
}

// TestPlay_InvalidTypeId_Rejected 非法 typeId (空 / 未知串 / 不存在的 group/type) 一律 400。
func TestPlay_InvalidTypeId_Rejected(t *testing.T) {
	cases := []string{"", "bogus", "group:image", "type:ghost"}
	for _, tc := range cases {
		code, resp := playPost(t, map[string]string{"typeId": tc})
		if code != http.StatusBadRequest {
			t.Fatalf("typeId=%q 应 400, got %d: %v", tc, code, resp)
		}
		if msg, _ := resp["message"].(string); msg == "" {
			t.Fatalf("400 应携带 message: %v", resp)
		}
		// 不应写入请求文件
		if readPlayFile(t) != nil {
			os.Remove(filepath.Join(os.TempDir(), playRequestFile))
			t.Fatalf("非法 typeId=%q 不应写入请求文件", tc)
		}
	}
}
