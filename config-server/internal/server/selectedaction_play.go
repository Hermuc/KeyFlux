package server

import (
	"encoding/json"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"sync/atomic"

	"github.com/gin-gonic/gin"
	"settings/internal/script"
	"settings/internal/script/model"
)

// 彩蛋 (▶ 真实执行) 请求端点: 设置界面 ▶ 触发, 后端白名单校验后把 typeId 写入
// 一个请求文件 (%TEMP%\kf_play_request.json), 由 AHK 引擎轮询消费并真实执行该类型行为。
//
// 安全边界 (与 AHK 端对称):
//   - 只接受白名单 typeId: 内置文本特征值 / 已配置 group:<name> / 已配置 type:<id>;
//   - 样例内容全程硬编码在 AHK 端, 请求文件不含任何命令/路径参数 ⇒ 无注入面;
//   - group:<name> 由后端折叠为规范化后缀串再写入, 引擎按 matchValue 精确命中组。

// 内置文本特征值 (与 AHK TextFeatureSpecs / Go textfeatures.go 同构; plain 恒兜底)。
var playBuiltinTextFeatures = map[string]bool{
	"url": true, "path": true, "magnet": true, "bilibili": true, "plain": true,
}

// playSeq 自增序号: 每条请求单调递增, 供引擎 seq 去重 (同进程内唯一)。
var playSeq uint64

// playRequestFile 请求文件落点 (%TEMP%\kf_play_request.json)。
const playRequestFile = "kf_play_request.json"

type playRequest struct {
	TypeId string `json:"typeId"`
}

// PlaySelectedActionHandler 彩蛋触发: 白名单校验 typeId → 折叠 group 为后缀串 →
// 原子写请求文件 → 返回 {"ok":true}。非法 typeId 返回 400。
func PlaySelectedActionHandler(c *gin.Context) {
	var req playRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		panic(err)
	}
	typeId := strings.TrimSpace(req.TypeId)
	if typeId == "" {
		c.JSON(http.StatusBadRequest, gin.H{"message": "typeId 不能为空"})
		return
	}

	// 白名单校验 + group 折叠 (需读运行配置确认类型已存在)
	cfg := &model.Config{}
	if diskCfg, err := script.ParseConfig("../data/config.json"); err == nil {
		cfg = diskCfg
	}
	resolved := resolvePlayTypeId(typeId, cfg)
	if resolved == "" {
		c.JSON(http.StatusBadRequest, gin.H{"message": "非法的 typeId: " + typeId})
		return
	}

	if err := writePlayRequestFile(resolved); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"message": "写入请求文件失败: " + err.Error()})
		return
	}
	c.JSON(http.StatusOK, gin.H{"ok": true})
}

// resolvePlayTypeId 校验 typeId 是否在白名单内, 并把 group:<name> 折叠为规范化后缀串:
//   - 内置文本特征 → 原样返回 (特征值即 matchValue);
//   - "type:<id>" → 必须在 cfg.MatchTypes 中存在 (任意 kind), 原样返回 (引用串即 matchValue);
//   - "group:<name>" → 必须在 cfg.FileGroups 中存在, 折叠为 strings.Join(Exts, ",");
//   - 其余 → 返回 "" (拒绝)。
func resolvePlayTypeId(typeId string, cfg *model.Config) string {
	if playBuiltinTextFeatures[typeId] {
		return typeId
	}
	if strings.HasPrefix(typeId, "type:") {
		for _, t := range cfg.MatchTypes {
			if "type:"+t.ID == typeId {
				return typeId
			}
		}
		return ""
	}
	if strings.HasPrefix(typeId, "group:") {
		name := strings.TrimPrefix(typeId, "group:")
		for _, g := range cfg.FileGroups {
			if g.Name == name {
				return strings.Join(g.Exts, ",")
			}
		}
		return ""
	}
	return ""
}

// writePlayRequestFile 原子写请求文件: 先写临时文件再 rename, 避免引擎读到半截内容。
// 文件内容 {"typeId":"...","seq":<自增>} —— 不含任何命令/路径参数。
func writePlayRequestFile(typeId string) error {
	seq := atomic.AddUint64(&playSeq, 1)
	payload, err := json.Marshal(gin.H{"typeId": typeId, "seq": seq})
	if err != nil {
		return err
	}
	dir := os.TempDir()
	final := filepath.Join(dir, playRequestFile)

	// 临时文件带时间戳+pid 前缀, 同目录 rename 保证原子 (同盘内 rename 为 O(1) 且不可见半写)。
	tmp, err := os.CreateTemp(dir, "kf_play_*.json.tmp")
	if err != nil {
		return err
	}
	tmpName := tmp.Name()
	// 失败时清理临时文件
	defer func() {
		if tmp != nil {
			tmp.Close()
			os.Remove(tmpName)
		}
	}()
	if _, err := tmp.Write(payload); err != nil {
		return err
	}
	if err := tmp.Sync(); err != nil {
		return err
	}
	if err := tmp.Close(); err != nil {
		return err
	}
	tmp = nil // 已关闭, defer 不再清理
	return os.Rename(tmpName, final)
}
