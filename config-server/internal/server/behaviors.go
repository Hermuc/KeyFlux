package server

import (
	"net/http"
	"os"
	"path/filepath"

	"github.com/gin-gonic/gin"
	"settings/internal/behaviors"
	"settings/internal/proc"
	"settings/internal/script/model"
)

// 行为包 REST API: 内置包随软件分发 (settings.exe 同级 behaviors/, 只读),
// 用户包位于 ../data/behaviors (config.json 同级数据区, 随部署目录迁移)。
// 变更不自动重启引擎: 前端经 POST /api/behaviors/apply 显式重启, 避免连续
// 增删多个行为时的重启风暴 (与「保存设置即重启」的方案 CRUD 语义刻意区分)。

const userBehaviorsDir = "../data/behaviors"

func loadBehaviorCatalog() *behaviors.Catalog {
	builtinDir := "behaviors"
	if exePath, err := os.Executable(); err == nil {
		builtinDir = filepath.Join(filepath.Dir(exePath), "behaviors")
	}
	return behaviors.LoadCatalog(builtinDir, userBehaviorsDir)
}

// behaviorRuleRefs 把 selectedAction.mappings 的 entries 投影为删除校验所需引用
// (避免 behaviors 包反向依赖 model); RuleRef.ActionType = entry.behavior,
// 旧实现从 actionSchemes.Rules 收集, 已随「单键分发」重构改造。
func behaviorRuleRefs(sa *model.SelectedAction) []behaviors.RuleRef {
	refs := make([]behaviors.RuleRef, 0)
	if sa == nil {
		return refs
	}
	for _, m := range sa.Mappings {
		for _, e := range m.Entries {
			refs = append(refs, behaviors.RuleRef{MatchType: m.MatchType, MatchValue: m.MatchValue, ActionType: e.Behavior})
		}
	}
	return refs
}

func GetBehaviorsHandler(c *gin.Context) {
	catalog := loadBehaviorCatalog()
	builtin := []*behaviors.Pack{}
	user := []*behaviors.Pack{}
	for _, p := range catalog.Packs {
		if p.Source == "builtin" {
			builtin = append(builtin, p)
		} else {
			user = append(user, p)
		}
	}
	c.JSON(http.StatusOK, gin.H{"builtin": builtin, "user": user, "errors": catalog.Errors})
}

func rejectIfScriptEntry(c *gin.Context, p *behaviors.Pack) bool {
	// 一期仅开放基础动作组合; script entry (编译期 Include + BehaviorRegistry) 为二期
	if p.Entry.Kind == "script" {
		c.JSON(http.StatusBadRequest, gin.H{"message": "脚本行为将在后续版本支持, 当前请选择基础动作组合"})
		return true
	}
	return false
}

func CreateBehaviorHandler(c *gin.Context) {
	var p behaviors.Pack
	if err := c.ShouldBindJSON(&p); err != nil {
		panic(err)
	}
	p.Source = "user"
	if rejectIfScriptEntry(c, &p) {
		return
	}
	if err := behaviors.WriteUserPack(userBehaviorsDir, &p); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"message": err.Error()})
		return
	}
	c.JSON(http.StatusOK, p)
}

func UpdateBehaviorHandler(c *gin.Context) {
	id := c.Param("id")
	var p behaviors.Pack
	if err := c.ShouldBindJSON(&p); err != nil {
		panic(err)
	}
	p.ID = id
	p.Source = "user"
	if rejectIfScriptEntry(c, &p) {
		return
	}
	catalog := loadBehaviorCatalog()
	existing := catalog.Get(id)
	if existing == nil || existing.Source != "user" {
		c.JSON(http.StatusNotFound, gin.H{"message": "仅可编辑用户自定义行为"})
		return
	}
	if err := behaviors.WriteUserPack(userBehaviorsDir, &p); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"message": err.Error()})
		return
	}
	c.JSON(http.StatusOK, p)
}

func DeleteBehaviorHandler(c *gin.Context) {
	id := c.Param("id")
	catalog := loadBehaviorCatalog()
	if err := behaviors.ValidateDelete(catalog, id, behaviorRuleRefs(loadSelectedAction())); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"message": err.Error()})
		return
	}
	if err := behaviors.RemoveUserPack(userBehaviorsDir, id); err != nil {
		panic(err)
	}
	c.JSON(http.StatusOK, gin.H{"message": "ok"})
}

// ApplyBehaviorsHandler 重启 KeyFlux 使行为变更生效 (launcher 重新生成脚本并注册)。
func ApplyBehaviorsHandler(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{"restartFailed": !proc.ExecCmd("./KeyFlux.exe")})
}
