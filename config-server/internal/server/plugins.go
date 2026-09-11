package server

import (
	"net/http"

	"github.com/gin-gonic/gin"
	"settings/internal/plugins"
)

// 插件 REST API (插件页): 用户插件位于 ../data/plugins (config.json 同级数据区)。
// 管理面 (列表/导入/删除) 即时生效 (settings.exe 每次实时读目录, 无需重启);
// 启停状态存 config.json options.plugins, 经既有 PUT /config 链路保存并重启引擎。
// 引擎侧插件运行时为阶段 2 —— 导入的插件暂不参与脚本生成, 仅入册管理。

const userPluginsDir = "../data/plugins"

func loadPluginCatalog() *plugins.Catalog {
	return plugins.LoadCatalog(userPluginsDir)
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
func DeletePluginHandler(c *gin.Context) {
	id := c.Param("id")
	if err := plugins.Remove(userPluginsDir, id); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"message": err.Error()})
		return
	}
	c.JSON(http.StatusOK, gin.H{"message": "ok"})
}
