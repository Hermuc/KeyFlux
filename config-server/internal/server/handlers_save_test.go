package server

import (
	"bytes"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"testing"

	"github.com/gin-gonic/gin"

	"settings/internal/script"
)

// putConfigRaw 把原始 JSON 打到 PUT /config (debug=false) 并返回状态码与响应体。
// 仅用于「校验拒绝」用例: 这些输入在落盘与重启之前即被 400 拦截, 不产生文件/进程副作用。
func putConfigRaw(t *testing.T, raw string) (int, string) {
	t.Helper()
	gin.SetMode(gin.TestMode)
	r := gin.New()
	r.PUT("/config", SaveConfigHandler(false))
	w := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPut, "/config", bytes.NewReader([]byte(raw)))
	req.Header.Set("Content-Type", "application/json")
	r.ServeHTTP(w, req)
	return w.Code, w.Body.String()
}

// ValidateMatchTypes 接回保存链路 (handlers.go SaveConfigHandler) 后的守门用例。
// 三类非法 matchTypes 必须回 400, 文案与 Rust 侧 (config-ui-reactor/src/server/validate.rs)
// 逐字一致 —— 两端文案漂移会让面板提示与后端不一致。
func TestSaveConfigRejectsInvalidMatchTypes(t *testing.T) {
	cases := []struct {
		name string
		body string
		want string
	}{
		{
			name: "重复 id",
			body: `{"keymaps":[],"matchTypes":[` +
				`{"id":"dup","label":"甲","kind":"text","rules":[{"op":"contains","value":"a"}]},` +
				`{"id":"dup","label":"乙","kind":"text","rules":[{"op":"contains","value":"b"}]}]}`,
			want: `{"message":"保存失败: 内部标识「dup」重复，同一类型的标识必须唯一"}`,
		},
		{
			name: "label 为空",
			body: `{"keymaps":[],"matchTypes":[` +
				`{"id":"empty","label":"   ","kind":"text","rules":[{"op":"contains","value":"a"}]}]}`,
			want: `{"message":"保存失败: 匹配类型「empty」缺少名称"}`,
		},
		{
			name: "op 非法",
			body: `{"keymaps":[],"matchTypes":[` +
				`{"id":"badop","label":"算子","kind":"text","rules":[{"op":"regex","value":"a"}]}]}`,
			want: `{"message":"保存失败: 匹配类型「badop」第 1 条匹配条件的匹配方式无效「regex」（可选：包含该文字 / 完全相同 / 以该文字开头 / 以该文字结尾）"}`,
		},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			code, body := putConfigRaw(t, tc.body)
			if code != http.StatusBadRequest {
				t.Fatalf("应回 400, 实际 %d, body=%s", code, body)
			}
			if body != tc.want {
				t.Fatalf("400 文案不一致:\n got = %s\nwant = %s", body, tc.want)
			}
		})
	}
}

// 出厂样例回归: 仓库 data/config.json 必须仍能通过新增的保存期校验 (读不到时跳过,
// 例如在隔离环境运行单测)。该样例无 matchTypes/fileGroups 段, 属合法降级而非坏状态。
func TestFactoryConfigPassesSaveValidation(t *testing.T) {
	path := filepath.Join("..", "..", "..", "data", "config.json")
	cfg, err := script.ParseConfig(path)
	if err != nil {
		t.Skipf("出厂配置不可用, 跳过: %v", err)
	}
	if err := script.ValidateFileGroups(cfg.FileGroups); err != nil {
		t.Fatalf("出厂配置 fileGroups 应通过校验: %v", err)
	}
	if err := script.ValidateMatchTypes(cfg.MatchTypes, cfg.FileGroups); err != nil {
		t.Fatalf("出厂配置 matchTypes 应通过校验: %v", err)
	}
}
