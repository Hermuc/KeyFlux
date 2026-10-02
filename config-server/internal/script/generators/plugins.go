// 插件注入渲染: 扫描 config.json 同级 plugins/ 目录, 渲染模板注入块。
// 设计: docs/CONTRACTS.md §3.7 (生成端 #Include + PluginManager.Register/LoadEntry)。
// 零插件 (目录缺失/为空) 时两块均为空串 —— 模板采用「行尾拼接」约定
// (SetWorkingDir("../"){{ PLUGIN_BOOTSTRAP }}), 空块下生成产物与历史字节一致。
package generators

import (
	"fmt"
	"os"
	"path/filepath"
	"settings/internal/plugins"
	"settings/internal/script/model"
	"strings"
	"sync"
)

// PluginsDir 插件目录 (config.json 同级 plugins/), 由调用方注入
// (command.GenerateAHK 按配置路径推导; script.GenerateScripts 固定 ../data/plugins)。
// 空串 = 不注入。与 generators.Cfg / BehaviorCatalog 同款全局注入模式。
var PluginsDir string

var (
	pluginMu             sync.Mutex
	pluginIncludeBlock   string
	pluginBootstrapBlock string
	pluginLateInitBlock  string
	pluginScanDone       bool
)

// SetPluginsDir 注入插件目录并失效已缓存块 (每进程生成一次, 保留重置能力便于测试)。
func SetPluginsDir(dir string) {
	pluginMu.Lock()
	defer pluginMu.Unlock()
	PluginsDir = dir
	pluginIncludeBlock, pluginBootstrapBlock, pluginLateInitBlock = "", "", ""
	pluginScanDone = false
}

// PluginIncludes 模板函数 {{ PLUGIN_INCLUDES }}: 各插件入口 #Include 行,
// 每行以 \n 前导 (行尾拼接约定), 末行不带换行。
func PluginIncludes() string {
	inc, _, _ := pluginBlocks()
	return inc
}

// PluginBootstrap 模板函数 {{ PLUGIN_BOOTSTRAP }}: Register + LoadEntry 引导行。
func PluginBootstrap() string {
	_, boot, _ := pluginBlocks()
	return boot
}

// PluginLateInit 模板函数 {{ PLUGIN_LATE_INIT }}: 插件晚初始化扩展点
// (2026-10-01 QuickSwitch 插件化 P2 引入; 模板中位于 InitKeymap() 与 OnExit 之间)。
// 产出「必须晚于 InitKeymap()」的插件初始化行, 每行以 \n 前导, 空块 = 零字节
// (行尾拼接约定, 与 PLUGIN_INCLUDES/BOOTSTRAP 同款)。
// 当前唯一消费方 = quick_switch 的 InitQuickSwitch(...) 调用行: 代码已随插件搬入
// data/plugins/quick_switch/ (提案 docs/contracts-proposals/quickswitch-pluginization.md),
// 调用时机与参数来源 (config.json options.quickSwitch 的生成期渲染) 保持与硬编码时期
// 逐字节一致; 插件被禁用/入口缺失 => 不产出 (可删除性保证)。配置段迁移到
// plugin-settings.json + 声明式设置属 P5, 届时本函数对 quick_switch 的特判移除。
func PluginLateInit() string {
	_, _, late := pluginBlocks()
	return late
}

func pluginBlocks() (string, string, string) {
	pluginMu.Lock()
	defer pluginMu.Unlock()
	if !pluginScanDone {
		pluginScanDone = true
		pluginIncludeBlock, pluginBootstrapBlock, pluginLateInitBlock = renderPluginBlocks(PluginsDir)
	}
	return pluginIncludeBlock, pluginBootstrapBlock, pluginLateInitBlock
}

// disabledPluginSet 读取 config.options.plugins.disabled (启停持久化, 契约 §1 数据进配置)。
// generators.Cfg 由 SaveAHK 在模板执行前注入 (本函数仅在模板执行期被调用)。
func disabledPluginSet() map[string]bool {
	out := map[string]bool{}
	if Cfg != nil && Cfg.Options.Plugins.Disabled != nil {
		for _, id := range Cfg.Options.Plugins.Disabled {
			out[id] = true
		}
	}
	return out
}

// removedPluginSet 读取 config.options.plugins.removed (2026-10-02 P4 墓碑):
// 用户主动删除的随包内置插件 ID —— 目录可能因随包同步/重装仍在, 墓碑让生成器
// 跳过注入 (否则删了又活)。
func removedPluginSet() map[string]bool {
	out := map[string]bool{}
	if Cfg != nil && Cfg.Options.Plugins.Removed != nil {
		for _, id := range Cfg.Options.Plugins.Removed {
			out[id] = true
		}
	}
	return out
}

// renderPluginBlocks 单插件失败只产注释行, 不影响其他插件 (契约约束 4 错误隔离)。
// 入口文件在生成期做存在性与路径安全校验: AHK 的 #Include 指向缺失文件会让整个
// 脚本加载失败 (拖垮引擎), 必须在生成期拦下。
// 第三块 = 晚初始化行 (PluginLateInit / {{ PLUGIN_LATE_INIT }}), 与前两块共享同一
// 存在性/禁用判定 —— 插件被跳过时三块同步跳过, 保证「插件不可用 ⇒ 初始化行不产出」。
func renderPluginBlocks(dir string) (includes, bootstrap, lateInit string) {
	if dir == "" {
		return "", "", ""
	}
	cat := plugins.LoadCatalog(dir)
	disabled := disabledPluginSet()
	removed := removedPluginSet()
	var inc, boot, late strings.Builder
	for _, m := range cat.Plugins {
		if removed[m.ID] {
			// 墓碑 (config.options.plugins.removed, 2026-10-02 P4): 用户主动删除的
			// 随包内置插件 —— 目录被同步带回时不复活
			boot.WriteString(fmt.Sprintf("\n; [插件] %s 已被用户移除 (墓碑), 跳过加载", m.ID))
			continue
		}
		if disabled[m.ID] {
			// 启停持久化 (config.options.plugins.disabled): 停用插件不注入不注册,
			// 落一行注释便于用户在生成产物里看到过滤结果
			boot.WriteString(fmt.Sprintf("\n; [插件] %s 已在配置中停用, 跳过加载", m.ID))
			continue
		}
		if m.Entry.Kind != "script" || m.Entry.File == "" {
			boot.WriteString(fmt.Sprintf("\n; [插件警告] %s: 仅支持 script 入口 (entry.file), 已跳过", m.ID))
			continue
		}
		if !safePluginRelFile(m.Entry.File) {
			boot.WriteString(fmt.Sprintf("\n; [插件警告] %s: entry.file 非法 %q, 已跳过", m.ID, m.Entry.File))
			continue
		}
		abs := filepath.Join(dir, m.ID, filepath.FromSlash(m.Entry.File))
		if _, err := os.Stat(abs); err != nil {
			boot.WriteString(fmt.Sprintf("\n; [插件警告] %s: 入口文件缺失 (%s), 已跳过", m.ID, m.Entry.File))
			continue
		}
		// 生成脚本位于 bin/, 用户插件在 ../data/plugins/<id>/ (目录名=manifest.id, 导入时已校验)
		inc.WriteString(fmt.Sprintf("\n#Include ../data/plugins/%s/%s", m.ID, m.Entry.File))
		boot.WriteString(fmt.Sprintf("\nPluginManager.Register(%s)", ahkManifestLiteral(m)))
		boot.WriteString(fmt.Sprintf("\nPluginManager.LoadEntry(%s)", ahkStringLit(m.ID)))
		if m.ID == "quick_switch" && Cfg != nil {
			late.WriteString(renderQuickSwitchLateInit(Cfg.Options.QuickSwitch))
		}
	}
	for _, e := range cat.Errors {
		boot.WriteString(fmt.Sprintf("\n; [插件错误] %s", e))
	}
	return inc.String(), boot.String(), late.String()
}

// renderQuickSwitchLateInit 渲染 quick_switch 的晚初始化行 (InitQuickSwitch 调用)。
// 🔴 字节等价约束: 字段顺序 / 分隔符 / bool 文本 (%t) / ahkString 转义 必须与迁移前
// 模板硬编码行完全一致 (否则 parity 12 份基线全漂)。迁移前形态见 git 历史
// keyflux.tmpl (InitQuickSwitch({...}), 2026-10-01 P2 前最后一版)。
// 参数来源 = config.json options.quickSwitch (Go 模板引擎原渲染点), 非 plugin-settings
// (迁移属 P5)。
func renderQuickSwitchLateInit(q model.QuickSwitchOption) string {
	var b strings.Builder
	b.WriteString("\nInitQuickSwitch({collectEnabled: ")
	b.WriteString(fmt.Sprintf("%t", q.CollectEnabled))
	b.WriteString(", autoShow: ")
	b.WriteString(fmt.Sprintf("%t", q.AutoShow))
	b.WriteString(", autoJumpOpen: ")
	b.WriteString(fmt.Sprintf("%t", q.AutoJumpOpen))
	b.WriteString(", autoJumpSave: ")
	b.WriteString(fmt.Sprintf("%t", q.AutoJumpSave))
	b.WriteString(fmt.Sprintf(", pollIntervalMs: %d", q.PollIntervalMs))
	b.WriteString(fmt.Sprintf(", maxHistory: %d", q.MaxHistory))
	b.WriteString(fmt.Sprintf(", overlayRows: %d", q.OverlayRows))
	b.WriteString(fmt.Sprintf(", overlayRowsCompact: %d", q.OverlayRowsCompact))
	b.WriteString(", excludedPrefixes: [")
	for i, p := range q.ExcludedPrefixes {
		if i > 0 {
			b.WriteString(", ")
		}
		b.WriteString(model.AhkString(p))
	}
	b.WriteString("]})")
	return b.String()
}

// ahkManifestLiteral 把 manifest 渲染为 AHK Map 字面量
// (AHK v2 无内置 JSON 解析, 生成端代为解析后以原生字面量下发)。
func ahkManifestLiteral(m *plugins.Manifest) string {
	var b strings.Builder
	b.WriteString(`Map("id", ` + ahkStringLit(m.ID))
	b.WriteString(`, "name", ` + ahkStringLit(m.Name))
	if m.NameEn != "" {
		b.WriteString(`, "nameEn", ` + ahkStringLit(m.NameEn))
	}
	if m.Version != "" {
		b.WriteString(`, "version", ` + ahkStringLit(m.Version))
	}
	b.WriteString(fmt.Sprintf(`, "specVersion", %d`, m.SpecVersion))
	if m.Description != "" {
		b.WriteString(`, "description", ` + ahkStringLit(m.Description))
	}
	if m.Author != "" {
		b.WriteString(`, "author", ` + ahkStringLit(m.Author))
	}
	b.WriteString(`, "entry", Map("kind", ` + ahkStringLit(m.Entry.Kind) +
		`, "file", ` + ahkStringLit(m.Entry.File) +
		`, "func", ` + ahkStringLit(m.Entry.Func) + `)`)
	if len(m.Permissions) > 0 {
		perms := make([]string, len(m.Permissions))
		for i, p := range m.Permissions {
			perms[i] = ahkStringLit(p)
		}
		b.WriteString(`, "permissions", [` + strings.Join(perms, ", ") + `]`)
	}
	b.WriteString(")")
	return b.String()
}

// ahkStringLit 渲染 AHK v2 双引号字符串字面量 (转义反引号与双引号; 换行拍平为空格,
// 避免 string literal 跨行)。
func ahkStringLit(s string) string {
	s = strings.ReplaceAll(s, "\r\n", " ")
	s = strings.ReplaceAll(s, "\n", " ")
	s = strings.ReplaceAll(s, "\r", " ")
	s = strings.ReplaceAll(s, "`", "``")
	s = strings.ReplaceAll(s, `"`, "`\"")
	return `"` + s + `"`
}

// safePluginRelFile 入口相对路径安全校验: 拒绝绝对路径、盘符与 .. / . 段
// (防插件逃逸出自身目录; manifest 来自用户 zip, 不可信)。
func safePluginRelFile(f string) bool {
	if f == "" || strings.HasPrefix(f, "/") || strings.HasPrefix(f, "\\") || strings.Contains(f, ":") {
		return false
	}
	for _, seg := range strings.FieldsFunc(f, func(r rune) bool { return r == '/' || r == '\\' }) {
		if seg == ".." || seg == "." {
			return false
		}
	}
	return true
}
