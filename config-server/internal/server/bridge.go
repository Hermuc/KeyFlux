package server

// bridge.go —— CLI 进程内 HTTP 桥。
//
// 为什么需要：设置面板要「不连 localhost、不起常驻后端进程」，但后端逻辑（handler 链）
// 必须保持**单一真源**。本桥用 gin 引擎 + 进程内 ResponseWriter 直接执行一次请求，
// **不开 socket、不占端口**，于是 CLI（`settings.exe Call ...`）与 HTTP（`server.Run`）
// 共用同一套 handler ——「换传输不换逻辑」，不会产生第二真源。
//
// 契约（面板侧依赖，**禁改**）：
//
//	settings.exe Call <METHOD> <PATH> <out-file> [--body <file>] [--content-type <ct>]
//	stdout 末行: KEYFLUX_CALL status=<code>
//	响应体原始字节写入 <out-file>
//
// 工作目录语义与 HTTP 模式**一致**（Go 依赖相对 `../data`、`./site`、`./templates`）：
// 调用方必须把 cwd 设为 settings.exe 所在目录（即 `bin/`）。

import (
	"bytes"
	"fmt"
	"net/http"
	"os"
)

// CallStatusPrefix 是 stdout 契约行前缀（**禁改**；Rust 侧按此前缀解析状态码）。
const CallStatusPrefix = "KEYFLUX_CALL status="

// recorder 是最小的 http.ResponseWriter 实现。
// 刻意不用 net/http/httptest —— 那是测试包，不该进生产二进制。
type recorder struct {
	header http.Header
	buf    bytes.Buffer
	code   int
}

func (r *recorder) Header() http.Header { return r.header }

func (r *recorder) Write(b []byte) (int, error) {
	if r.code == 0 {
		r.code = http.StatusOK
	}
	return r.buf.Write(b)
}

func (r *recorder) WriteHeader(code int) {
	if r.code == 0 {
		r.code = code
	}
}

// CallOnce 在进程内执行一次请求，返回（状态码, 响应体）。
//
// debug 恒为 false：与生产 HTTP 路径一致（无 CORS、保存时不重新生成脚本）。
func CallOnce(method, path string, body []byte, contentType string) (int, []byte, error) {
	router := NewRouter(nil, nil, false)

	req, err := http.NewRequest(method, path, bytes.NewReader(body))
	if err != nil {
		return 0, nil, err
	}
	if contentType != "" {
		req.Header.Set("Content-Type", contentType)
	}

	rec := &recorder{header: http.Header{}}
	router.ServeHTTP(rec, req)

	code := rec.code
	if code == 0 {
		code = http.StatusOK
	}
	return code, rec.buf.Bytes(), nil
}

// Call 是 CLI 入口（读 os.Args，与同包其它子命令口径一致；由 command 包注册进 Map）。
func Call(_ ...string) {
	if len(os.Args) < 5 {
		fmt.Fprintln(os.Stderr,
			"Call requires: Call <METHOD> <PATH> <out-file> [--body <file>] [--content-type <ct>]")
		os.Exit(2)
	}
	method := os.Args[2]
	path := os.Args[3]
	outFile := os.Args[4]

	var body []byte
	contentType := ""
	for i := 5; i < len(os.Args); i++ {
		switch os.Args[i] {
		case "--body":
			if i+1 < len(os.Args) {
				data, err := os.ReadFile(os.Args[i+1])
				if err != nil {
					fmt.Fprintln(os.Stderr, "Call: read body failed:", err)
					os.Exit(2)
				}
				body = data
				i++
			}
		case "--content-type":
			if i+1 < len(os.Args) {
				contentType = os.Args[i+1]
				i++
			}
		}
	}

	code, respBody, err := CallOnce(method, path, body, contentType)
	if err != nil {
		fmt.Fprintln(os.Stderr, "Call: request failed:", err)
		os.Exit(2)
	}
	if err := os.WriteFile(outFile, respBody, 0644); err != nil {
		fmt.Fprintln(os.Stderr, "Call: write output failed:", err)
		os.Exit(2)
	}
	fmt.Printf("%s%d\n", CallStatusPrefix, code)
}
