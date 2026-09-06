// Package server 承载 KeyFlux 配置后端的全部 HTTP 层:
// gin 引擎装配、路由注册、handler 实现、DTO 映射。
// 由 cmd/settings/main.go 调用 Run() 启动。
package server

import (
	"errors"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"os"
	"os/exec"

	"github.com/gin-contrib/cors"
	"github.com/gin-contrib/static"
	"github.com/gin-gonic/gin"
)

// Run 启动 HTTP 服务。参数显式传入 main() 侧的状态:
//   - hasError: 代码雨错误通道 (debug 模式下为 nil)
//   - rainDone: 代码雨结束信号
//   - debug: 调试模式 (开启 CORS、gin debug 输出、保存时重新生成脚本)
//   - headless: 无头模式 (Avalonia 壳子进程拉起, 端口通告行)
func Run(hasError chan<- struct{}, rainDone <-chan struct{}, debug bool, headless bool) {
	if !debug {
		gin.SetMode(gin.ReleaseMode)
		gin.DefaultWriter = io.Discard
	}

	router := gin.Default()
	router.Use(PanicHandler(hasError, rainDone))
	if debug {
		router.Use(cors.Default())
	}
	router.NoRoute(static.Serve("/", static.LocalFile("./site", false)), indexHandler)

	router.GET("/", indexHandler)
	router.GET("/config", GetConfigHandler)
	router.PUT("/config", SaveConfigHandler(debug))
	router.POST("/server/command/:id", ServerCommandHandler)
	router.GET("/shortcuts", GetShortcutsHandler)

	// 选中动作单键分发 API (方案 D): 旧 action-schemes CRUD 六路由已随重构移除,
	// 存量配置经 ParseConfig 读时一次性迁移
	router.POST("/api/selected-action/test", TestSelectedActionHandler)

	// 行为包 (选中动作「行为库」)
	router.GET("/api/behaviors", GetBehaviorsHandler)
	router.POST("/api/behaviors", CreateBehaviorHandler)
	router.PUT("/api/behaviors/:id", UpdateBehaviorHandler)
	router.DELETE("/api/behaviors/:id", DeleteBehaviorHandler)
	router.POST("/api/behaviors/apply", ApplyBehaviorsHandler)

	// 先尝试 12333 端口, 失败了则用随机端口. 因为 12333 端口可能已被占用, 或者被禁:
	// An attempt was made to access a socket in a way forbidden by its access permissions.
	ln, err := net.Listen("tcp", "localhost:12333")
	if err != nil {
		ln, err = net.Listen("tcp", "localhost:0")
		if err != nil {
			if hasError != nil { // debug 模式下 hasError 为 nil
				close(hasError)
			}
			<-rainDone
			fmt.Println("Error:", err.Error())
			_, _ = fmt.Scanln()
			os.Exit(1)
		}
	}

	if headless {
		// 端口通告行: 必须为 stdout 第一行输出, 供 Avalonia 壳逐行匹配 "KEYFLUX_PORT=" 前缀 (不打印任何装饰文本)
		fmt.Printf("KEYFLUX_PORT=%d\n", ln.Addr().(*net.TCPAddr).Port)
	}

	if !debug && !headless {
		go func() {
			err := openBrowser(ln.Addr())
			if err != nil {
				hasError <- struct{}{}
				<-rainDone
				fmt.Println("Error:", err.Error())
			}
		}()
	}

	err = router.RunListener(ln)
	if err != nil {
		close(hasError)
		<-rainDone
		log.Fatal(err)
	}
}

func openBrowser(addr net.Addr) error {
	// 端口已就绪 (net.Listen 成功后调用), 无需固定延迟; 立即打开浏览器可显著缩短设置入口感知延迟
	if addr, ok := addr.(*net.TCPAddr); ok {
		// rundll32 位于系统 PATH 中, 无需硬编码绝对路径
		return exec.Command("rundll32.exe", "url.dll,FileProtocolHandler", fmt.Sprintf("http://localhost:%d", addr.Port)).Start()
	}
	return errors.New("addr is not tcp")
}

func PanicHandler(hasError chan<- struct{}, rainDone <-chan struct{}) gin.HandlerFunc {
	return func(c *gin.Context) {
		defer func() {
			if err := recover(); err != nil {
				// 不允许重复 close channel
				if hasError != nil {
					close(hasError)
					<-rainDone
					hasError = nil
				}
				panic(err)
			}
		}()

		c.Next()
	}
}

func indexHandler(c *gin.Context) {
	data, err := os.ReadFile("./site/index.html")
	if err != nil {
		// 旧 Vue web UI 已退役 (Avalonia 原生设置界面替代), site/ 不再包含 index.html:
		// 根路径直接返回 404, 避免 panic 拖垮后端服务
		c.Status(http.StatusNotFound)
		return
	}
	// 设置 Cache-Control: no-store 禁用缓存
	c.Header("Cache-Control", "no-store")
	c.Data(http.StatusOK, "text/html; charset=utf-8", data)
}
