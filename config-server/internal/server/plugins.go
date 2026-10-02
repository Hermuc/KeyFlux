package server

import (
	"net/http"
	"sort"

	"github.com/gin-gonic/gin"

	"settings/internal/plugins"
)

// 插件 REST API (插件页): 用户插件位于 ../data/plugins, 设置值位于
// ../data/plugin-settings.json (两者都是 config.json 同级数据区, 与设置界面同 CWD = bin/)。
//
// 三条链路的分工:
//   - 管理面 (列表/导入/删除) 即时生效 (settings.exe 每次实时读目录, 无需重启);
//   - 启停状态存 config.json options.plugins, 经既有 PUT /config 链路保存并重启引擎;
//   - 设置值存 plugin-settings.json, 由本文件的 GET/PUT 读写。引擎侧不重启 ——
//     插件在每次会话开始时重读该文件 (hot reload), 故保存即生效。

// userPluginsDir / pluginSettings 是包级可变依赖 —— 生产恒为下面的固定值,
// 单测里可临时指向 t.TempDir() (见 plugins_settings_test.go 的 withTempPluginDirs)。
var (
	userPluginsDir = "../data/plugins"

	// pluginSettingsPath 与引擎侧 ConfigProvider.ahk 的
	// A_ScriptDir\..\data\plugin-settings.json 指向同一文件 (A_ScriptDir = <root>/bin)。
	pluginSettingsPath = "../data/" + plugins.SettingsFileName

	// pluginSettings 全局单例: 内部自带互斥锁做读-改-写串行化 (并发 PUT 不互相覆盖)。
	pluginSettings = plugins.NewSettingsStore(pluginSettingsPath)
)

func loadPluginCatalog() *plugins.Catalog {
	return plugins.LoadCatalog(userPluginsDir)
}

// findUserPlugin 在用户插件目录里按 ID 找 manifest (找不到返回 nil)。
func findUserPlugin(id string) *plugins.Manifest {
	for _, m := range loadPluginCatalog().Plugins {
		if m.ID == id {
			return m
		}
	}
	return nil
}

// GetPluginsHandler 返回用户插件目录快照 (ID 字典序 + 逐包加载告警)。
func GetPluginsHandler(c *gin.Context) {
	catalog := loadPluginCatalog()
	pluginsOut := catalog.Plugins
	if pluginsOut == nil {
		pluginsOut = []*plugins.Manifest{}
	}
	c.JSON(http.StatusOK, gin.H{"plugins": pluginsOut, "errors": catalog.Errors})
}

// ImportPluginHandler 接收 multipart 字段 file (插件包 zip), 校验并安装到
// data/plugins/<id>/。成功返回安装后的 manifest。
func ImportPluginHandler(c *gin.Context) {
	file, err := c.FormFile("file")
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"message": "缺少上传文件 (multipart 字段 file)"})
		return
	}
	f, err := file.Open()
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"message": "打开上传文件失败: " + err.Error()})
		return
	}
	defer f.Close()

	m, err := plugins.InstallFromZip(f, userPluginsDir)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"message": err.Error()})
		return
	}
	c.JSON(http.StatusOK, m)
}

// DeletePluginHandler 删除用户插件目录 (内置 ID / 非法 ID / 不存在均拒绝)。
// 启停注册表 (options.plugins.disabled) 的孤儿清理由 UI 保存链路负责 (同行为包先例:
// 后端只管包目录, config.json 的变更统一走 PUT /config)。
//
// 刻意**不**清理 plugin-settings.json: 卸载重装 (或同名重写) 场景下, 用户填过的
// Everything 路径等设置通常还想接着用; 残留键无副作用 (读不到 manifest 就没人读它)。
func DeletePluginHandler(c *gin.Context) {
	id := c.Param("id")
	if err := plugins.Remove(userPluginsDir, id); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"message": err.Error()})
		return
	}
	c.JSON(http.StatusOK, gin.H{"message": "ok"})
}

// ---------------- 插件设置 (声明式 schema + 值) ----------------

// pluginSettingsDTO 是 GET/PUT /api/plugins/:id/settings 的响应体。
//
// 同时带上 settings 声明与 values 值: 声明是渲染契约 (字段名/类型/上下限/提示),
// 值是数据。两者一起返回, 端点即自洽 —— 调用方不需要先另外拉一次 manifest
// 才能打开一个设置对话框。
type pluginSettingsDTO struct {
	ID       string            `json:"id"`
	Settings []plugins.Setting `json:"settings"`
	Values   map[string]string `json:"values"`
}

// mergedValues 把「manifest 默认值」与「已存值」合并成一份完整值表:
// 只含声明的键, 未存过的键回落默认值 —— 界面拿到的永远是可直接绑定的完整状态。
func mergedValues(m *plugins.Manifest, stored map[string]string) map[string]string {
	out := make(map[string]string, len(m.Settings))
	for _, s := range m.Settings {
		if v, ok := stored[s.Key]; ok {
			out[s.Key] = v
			continue
		}
		out[s.Key] = s.Default
	}
	return out
}

// GetPluginSettingsHandler 返回某插件的完整设置 (声明 + 默认值合并后的值)。
func GetPluginSettingsHandler(c *gin.Context) {
	id := c.Param("id")
	m := findUserPlugin(id)
	if m == nil {
		c.JSON(http.StatusNotFound, gin.H{"message": "插件「" + id + "」不存在"})
		return
	}
	c.JSON(http.StatusOK, pluginSettingsDTO{
		ID:       m.ID,
		Settings: m.Settings,
		Values:   mergedValues(m, pluginSettings.LoadFor(m.ID)),
	})
}

// saveSettingsRequest 是 PUT 的请求体 (values 只需带要改的键)。
type saveSettingsRequest struct {
	Values map[string]string `json:"values"`
}

// SavePluginSettingsHandler 校验并保存某插件的设置值。
//
// 校验口径 (全部集中在后端, 界面只做同源的即时提示):
//   - 插件必须在目录中;
//   - 键必须是 manifest 声明过的 (未声明的键一律拒绝 —— 否则键空间会被写脏,
//     且用户会得到一个「填了但没插件读」的静默失效);
//   - 值必须通过 plugins.ValidateSettingValue (类型/长度/上下限)。
//
// 校验失败整单拒绝 (不做部分写入): 界面是一次保存全部字段, 半成功会让界面
// 显示与服务端状态不一致。
func SavePluginSettingsHandler(c *gin.Context) {
	id := c.Param("id")
	m := findUserPlugin(id)
	if m == nil {
		c.JSON(http.StatusNotFound, gin.H{"message": "插件「" + id + "」不存在"})
		return
	}

	var req saveSettingsRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"message": "请求体不是合法 JSON: " + err.Error()})
		return
	}

	allowed := m.AllowedSettings()
	// 先按 key 排序校验, 保证同一份错误输入总是得到同一条错误消息 (便于定位与测试)
	keys := make([]string, 0, len(req.Values))
	for k := range req.Values {
		keys = append(keys, k)
	}
	sort.Strings(keys)

	for _, k := range keys {
		s, ok := allowed[k]
		if !ok {
			c.JSON(http.StatusBadRequest, gin.H{"message": "设置项「" + k + "」不在插件的声明里"})
			return
		}
		if err := plugins.ValidateSettingValue(s, req.Values[k]); err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"message": "设置项「" + s.Label + "」的值不合法: " + err.Error()})
			return
		}
	}

	if err := pluginSettings.Save(m.ID, req.Values); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"message": "保存设置失败: " + err.Error()})
		return
	}
	c.JSON(http.StatusOK, pluginSettingsDTO{
		ID:       m.ID,
		Settings: m.Settings,
		Values:   mergedValues(m, pluginSettings.LoadFor(m.ID)),
	})
}
