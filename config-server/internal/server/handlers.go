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
)

func GetConfigHandler(c *gin.Context) {
	config, err := script.ParseConfig("../data/config.json")
	if err != nil {
		panic(err)
	}
	// 以计划任务真实生效态回填开机自启显示态 (详见 syncStartupFromTask)
	syncStartupFromTask(&config.Options.Startup)
	// DTO 转换在 syncStartupFromTask 之后, 确保任务回填值体现在响应中
	dto := ConfigToDTO(config)
	c.JSON(http.StatusOK, dto)
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
