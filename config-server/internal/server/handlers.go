package server

import (
	"bytes"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"

	"github.com/gin-gonic/gin"

	"settings/internal/proc"
	"settings/internal/script"

	"sync"
	"time")

func GetConfigHandler(c *gin.Context) {
	config, err := script.ParseConfig("../data/config.json")
	if err != nil {
		panic(err)
	}
	// 以计划任务真实生效态回填开机自启显示态 (详见 syncStartupFromTask)
	config.Options.Startup = getCachedStartupState()
	// DTO 转换在 syncStartupFromTask 之后, 确保任务回填值体现在响应中
	dto := ConfigToDTO(config)
	c.JSON(http.StatusOK, dto)
}

// ---- 开机自启显示态缓存 ----
// syncStartupFromTask 每次 GET /config 都同步 spawn schtasks.exe (冷启动 ~1s,
// 是设置面板"连接后端"耗时的大头)。改为进程启动时异步预热 + 结果缓存 (TTL 2s)
// + 保存路径写时失效: GET 命中缓存零 IO, 显示态滞后至多 2s (显示态本就无同步机制)。

var (
	startupMu       sync.Mutex
	startupCacheVal bool
	startupCacheAt  time.Time
)

func queryStartupFromTask() bool {
	out, err := exec.Command("schtasks", "/query", "/tn", "KeyFlux").Output()
	return err == nil && bytes.Contains(out, []byte("KeyFlux"))
}

// PreloadStartup 后台预热开机自启缓存: settings.exe 启动即发起, 与 GUI 冷启动
// (Avalonia 初始化 ~0.5s) 并行, GUI 首次 GET /config 时缓存大概率已就绪。
func PreloadStartup() {
	v := queryStartupFromTask()
	startupMu.Lock()
	startupCacheVal = v
	startupCacheAt = time.Now()
	startupMu.Unlock()
}

func invalidateStartupCache() {
	startupMu.Lock()
	startupCacheAt = time.Time{} // 归零 = 失效, 下次 GET 同步重查
	startupMu.Unlock()
}

// getCachedStartupState 读缓存; 未预热/过期时同步查一次并回填 (只此一处同步)。
func getCachedStartupState() bool {
	startupMu.Lock()
	defer startupMu.Unlock()
	if time.Since(startupCacheAt) < 2*time.Second {
		return startupCacheVal
	}
	v := queryStartupFromTask()
	startupCacheVal = v
	startupCacheAt = time.Now()
	return v
}

// syncStartupFromTask 用计划任务 KeyFlux 的真实状态回填 options.startup。
// 计划任务是开机自启的真实生效态 (bin/MiscTools.ahk RunAtStartup 经
// schtasks /create /xml 建/删, 2026-09-10 起替代 HKCU\Run 注册表方案),
// config.json 的 options.startup 仅是 UI 显示态且无同步机制, 外部删除任务
// 后 UI 会显示失真, 故 GET /config 时以任务存在性为准回填。
// 特意不放进 ParseConfig: 它还服务于 GenerateAHK/DumpPlan 等验证路径, 需保持
// 确定性, 回填只应作用于对外 HTTP 响应。
// 查询失败 (任务不存在返回非零/权限等) 均回 false, 不报错 (与注册表行为一致)。
func syncStartupFromTask(startup *bool) {
	out, err := exec.Command("schtasks", "/query", "/tn", "KeyFlux").Output()
	if err != nil || !bytes.Contains(out, []byte("KeyFlux")) {
		*startup = false
		return
	}
	*startup = true
}

func GetShortcutsHandler(c *gin.Context) {
	type shortcut struct {
		Path string `json:"path"`
	}
	exe, err := os.Executable()
	if err != nil {
		panic(err)
	}
	root := filepath.Dir(filepath.Dir(exe))
	pattern := filepath.Join(root, "shortcuts", "*.lnk")

	files, err := filepath.Glob(pattern)
	if err != nil {
		panic(err)
	}
	var data []shortcut
	for _, f := range files {
		data = append(data, shortcut{
			Path: f[len(root)+1:],
		})
	}
	c.JSON(http.StatusOK, data)
}

func ServerCommandHandler(c *gin.Context) {
	m := map[string]struct {
		exe  string
		args []string
	}{
		"2": {
			exe:  "./KeyFlux.exe",
			args: []string{"/script", "bin/WindowSpy.ahk"},
		},
		"3": {
			exe:  "./KeyFlux.exe",
			args: []string{"/script", "./bin/MiscTools.ahk", "RunAtStartup", "On"},
		},
		"4": {
			exe:  "./KeyFlux.exe",
			args: []string{"/script", "./bin/MiscTools.ahk", "RunAtStartup", "Off"},
		},
	}
	if c, ok := m[c.Param("id")]; ok {
		proc.ExecCmd(c.exe, c.args...)
	}

	c.JSON(http.StatusOK, gin.H{})
}

func SaveConfigHandler(debug bool) gin.HandlerFunc {
	return func(c *gin.Context) {
		var dto ConfigDTO
		if err := c.ShouldBindJSON(&dto); err != nil {
			panic(err)
		}
		// DTO→model 映射在校验与落盘之前
		config := DTOToConfig(&dto)

		// 校验选中动作单键分发组合合法性 (entry 引用的行为必须存在且覆盖匹配前提), 非法组合拒绝保存
		if err := script.ValidateSelectedAction(config.SelectedAction, loadBehaviorCatalog()); err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"message": "保存失败: " + err.Error()})
			return
		}

		// 校验文件分组表结构 (名称/显示名/后缀列表非空), 非法分组拒绝保存
		if err := script.ValidateFileGroups(config.FileGroups); err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"message": "保存失败: " + err.Error()})
			return
		}

		script.SaveConfigFile(config) // 保存配置文件
		invalidateStartupCache()      // 配置已变 (含开机自启), 显示态缓存失效待重查

		if debug {
			script.GenerateScripts(config) // 生成脚本文件
			// proc.ExecCmd("./KeyFlux.exe", "./bin/KeyFlux.ahk") // 重启程序且跳过 ahk 脚本生成
		}
		// 重启程序, 此时 launcher 会重新生成脚本; 启动失败时经 restartFailed 告知前端
		// (旧前端不读该字段, 保持向后兼容)
		restartFailed := !proc.ExecCmd("./KeyFlux.exe")

		c.JSON(http.StatusOK, gin.H{"message": "ok", "restartFailed": restartFailed})
	}
}
