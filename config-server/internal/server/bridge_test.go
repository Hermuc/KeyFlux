package server

import (
	"net/http"
	"sort"
	"strings"
	"testing"
)

// TestCallOnceHealth 验证进程内桥能真正打到 gin handler（而非空跑）。
func TestCallOnceHealth(t *testing.T) {
	code, body, err := CallOnce("GET", "/health", nil, "")
	if err != nil {
		t.Fatalf("CallOnce 失败: %v", err)
	}
	if code != http.StatusOK {
		t.Fatalf("status = %d, want %d", code, http.StatusOK)
	}
	if string(body) != "ok" {
		t.Fatalf("body = %q, want \"ok\"", string(body))
	}
}

// TestCallOnceUnknownPathIsHandled 未知路径必须被 NoRoute 处理（不得 panic/空响应）。
func TestCallOnceUnknownPathIsHandled(t *testing.T) {
	code, _, err := CallOnce("GET", "/__parity_nonexistent__", nil, "")
	if err != nil {
		t.Fatalf("CallOnce 失败: %v", err)
	}
	if code != http.StatusNotFound {
		t.Fatalf("status = %d, want %d", code, http.StatusNotFound)
	}
}

// TestRouterRouteSurfaceLocked 钉住路由表 —— 面板侧的 (HTTP ↔ CLI 桥) 两个传输共用这张表，
// 任何一条路由被误删/改名都会让面板功能静默失效；此测试是它的守卫。
func TestRouterRouteSurfaceLocked(t *testing.T) {
	router := NewRouter(nil, nil, false)

	got := make([]string, 0, 32)
	for _, r := range router.Routes() {
		got = append(got, r.Method+" "+r.Path)
	}
	sort.Strings(got)

	want := []string{
		"DELETE /api/behaviors/:id",
		"DELETE /api/plugins/:id",
		"GET /",
		"GET /api/behaviors",
		"GET /api/plugins",
		"GET /api/plugins/:id/settings",
		"GET /config",
		"GET /health",
		"GET /shortcuts",
		"POST /api/behaviors",
		"POST /api/behaviors/apply",
		"POST /api/plugins/import",
		"POST /api/selected-action/play",
		"POST /api/selected-action/test",
		"POST /server/command/:id",
		"PUT /api/behaviors/:id",
		"PUT /api/plugins/:id/settings",
		"PUT /config",
	}
	sort.Strings(want)

	if strings.Join(got, "\n") != strings.Join(want, "\n") {
		t.Fatalf("路由表与契约不符\n--- got ---\n%s\n--- want ---\n%s",
			strings.Join(got, "\n"), strings.Join(want, "\n"))
	}
}
