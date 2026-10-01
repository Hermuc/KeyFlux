// 外接脚本/函数正式化 (L1 插件) 生成端接入: docs/CONTRACTS.md §3.7 + §6
// ("外接脚本/函数 | L1 插件 = custom_functions.ahk 正式化; 长任务走 ScriptHost")。
//
// 扫描 config.json 同级的 scripts/*.ahk (平铺一层), 把用户散装外接脚本纳入既有 L1 插件管线:
//   - 每个 .ahk 渲染 #Include 行 (进程内编译, 其函数可被 ahkCode 动作/缩写调用);
//   - 导出 L1 入口契约 (§4: Register(api)) 的脚本进一步合成 PluginManager.Register/LoadEntry
//     引导, 成为带权限裁剪 API 视图的一等 L1 插件 —— 长任务经 api.run.RunScript 委托
//     ScriptHost 子进程 (约束 4: 回调 ≤50ms, 长任务不进主进程);
//   - 未导出入口的脚本仅作为函数库挂载 (等价 custom_functions.ahk 的既有用法),
//     不产生运行时 plugin_error 噪声;
//   - 生成 CustomScriptRun(id) 名录分发函数挂接 ScriptHost: 以数据式引用 (约束 7: config.json
//     只存数据不存代码) 经内置 AutoHotkey64.exe 子进程执行整份外接脚本。
//
// 兼容性:
//   - config.json 零新增字段 (约束 5: 不动 schemaVersion, 不触发 DTO 双轨五处同步);
//     启停复用既有 options.plugins.disabled (合成 ID 带 custom: 前缀, 与插件市场 ID 词表
//     ^[a-z][a-z0-9_]{0,31}$ 无冒号天然不冲突);
//   - 与插件同口径, DumpPlan 不建模外接脚本 (§3.7 注记: plan 侧原样透传);
//   - %selected% 等运行时替换全部留在选中动作既有链路, 本文件不处理任何占位符;
//   - 零外接脚本时三块均为空串 —— 模板行尾拼接约定下生成产物与历史字节一致
//     (golden/parity 不漂移)。
package generators

import (
	"bytes"
	"fmt"
	"io"
	"log"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"sync"
)

// CustomScriptsDir 外接脚本数据目录 (config.json 同级, 内含 scripts/ 子目录与
// custom_functions.ahk 约定文件), 由调用方注入 (script.GenerateScripts 固定 ../data;
// command.GenerateAHK 按配置路径推导)。空串 = 不注入 (三块恒空, 无磁盘副作用)。
// 与 generators.PluginsDir / generators.Cfg 同款全局注入模式。
var CustomScriptsDir string

var (
	customMu             sync.Mutex
	customIncludeBlock   string
	customBootstrapBlock string
	customFnBlock        string
	customScanDone       bool
)

// SetCustomScriptsDir 注入外接脚本目录并失效已缓存块 (每进程生成一次, 保留重置能力便于测试)。
func SetCustomScriptsDir(dir string) {
	customMu.Lock()
	defer customMu.Unlock()
	CustomScriptsDir = dir
	customIncludeBlock, customBootstrapBlock, customFnBlock = "", "", ""
	customScanDone = false
}

// CustomScriptIncludes 模板函数 {{ CUSTOM_SCRIPT_INCLUDES }}: 外接脚本 #Include 行,
// 每行以 \n 前导 (行尾拼接约定), 末行不带换行。
func CustomScriptIncludes() string {
	inc, _, _ := customBlocksOnce()
	return inc
}

// CustomScriptBootstrap 模板函数 {{ CUSTOM_SCRIPT_BOOTSTRAP }}: 合成插件的
// Register + LoadEntry 引导行与跳过/警告注释。
func CustomScriptBootstrap() string {
	_, boot, _ := customBlocksOnce()
	return boot
}

// CustomScriptBlocks 模板函数 {{ CUSTOM_SCRIPT_BLOCKS }}: CustomScriptRun(id) 名录分发
// 函数定义 (挂接点在模板 custom_functions.ahk 行尾 —— 定义区, 不插入 auto-exec 执行流)。
func CustomScriptBlocks() string {
	_, _, fn := customBlocksOnce()
	return fn
}

func customBlocksOnce() (string, string, string) {
	customMu.Lock()
	defer customMu.Unlock()
	if !customScanDone {
		customScanDone = true
		customIncludeBlock, customBootstrapBlock, customFnBlock = renderCustomScriptBlocks(CustomScriptsDir)
	}
	return customIncludeBlock, customBootstrapBlock, customFnBlock
}

const (
	// customFunctionsFile 传统约定文件 (模板 keyflux.tmpl 无条件
	// #Include ../data/custom_functions.ahk): 缺失会让整个引擎脚本加载失败。
	customFunctionsFile = "custom_functions.ahk"
	// customEntryFunc L1 入口契约函数名 (§4: main.ahk 必须导出 Register(api);
	// PluginManager.LoadEntry 对空 entry.func 亦回落此名)。
	customEntryFunc = "Register"
	// customIDPrefix 合成插件 ID 前缀: 插件市场 ID 词表不含冒号, 前缀保证两命名空间不冲突。
	customIDPrefix = "custom:"
	// customScanMaxBytes 入口契约扫描的读取上限 (只找一行函数定义, 不整读超大文件)。
	customScanMaxBytes = 1 << 20
)

// customFunctionsStub 自愈空桩内容: 仅注释无代码 (约束 7), 与仓库 data/custom_functions.ahk
// 的开头一致; 带 UTF-8 BOM (AHK v2 解析非 ASCII 注释需要)。
const customFunctionsStub = "\xef\xbb\xbf; 自定义的函数写在这个文件里,  然后能在 KeyFlux 中调用\r\n" +
	"\r\n" +
	"; 使用如下写法，来加载当前目录下的其他 AutoHotKey v2 脚本\r\n" +
	"; #Include ../data/test.ahk\r\n"

// customEntryRe 行锚定的入口契约识别: 仅认行首 `Register(api) {` 定义 (允许空白)。
// 多行签名/非常规排版会被判为函数库 —— 良性降级 (仅 #Include 挂载), 不会误报 plugin_error。
var customEntryRe = regexp.MustCompile(`(?m)^[ \t]*Register[ \t]*\([ \t]*api[ \t]*\)[ \t]*\{`)

// renderCustomScriptBlocks 单脚本失败只产注释行, 不影响其他脚本 (契约约束 4 错误隔离)。
// include 文本恒以 ../data/ 为基准 (生成脚本位于 bin/, 与 plugins.go 的
// ../data/plugins/ 约定一致, 不随 CLI 的 cwd 漂移)。
func renderCustomScriptBlocks(dir string) (includes, bootstrap, fnBlock string) {
	if dir == "" {
		return "", "", ""
	}
	ensureCustomFunctionsStub(dir)

	scriptsDir := filepath.Join(dir, "scripts")
	entries, err := os.ReadDir(scriptsDir)
	if err != nil {
		// 无 scripts/ 目录 = 零外接脚本, 静默 (与插件目录缺失同口径, 不产噪声)
		return "", "", ""
	}
	disabled := disabledPluginSet()

	// 入口函数名单一真源: AHK 全局函数不可重复定义, 两个被 Include 的载荷都定义
	// Register 会让引擎加载失败。custom_functions.ahk 由模板无条件 Include (不可跳过),
	// 优先占名; scripts/ 内按文件名字典序先到者胜 (约束 4 同款), 后到者跳过 Include。
	entryTaken := false
	if exported, err := exportsEntryFunc(filepath.Join(dir, customFunctionsFile)); err == nil && exported {
		entryTaken = true
	}

	var inc, boot, fn strings.Builder
	var dispatch [][2]string // {id, 相对 bin/ 的脚本路径}
	for _, de := range entries {
		if de.IsDir() {
			continue // 仅平铺一层: 子目录不是外接脚本
		}
		name := de.Name()
		if !validCustomScriptName(name) {
			continue // 非 .ahk (README 等) 静默跳过
		}
		base := strings.TrimSuffix(name, filepath.Ext(name))
		id := customIDPrefix + base
		if disabled[id] {
			boot.WriteString(fmt.Sprintf("\n; [外接脚本] %s 已在配置中停用, 跳过加载", id))
			continue
		}
		// 入口契约生成期校验 (读取失败不拦截 —— Include 仍产出, 仅落警告):
		// 导出 Register(api) 且名额已被占的载荷必须整体跳过, 否则两个被 Include 的文件
		// 都定义全局 Register, AHK 加载期直接报重复定义拖垮引擎
		exported, scanErr := exportsEntryFunc(filepath.Join(scriptsDir, name))
		if scanErr == nil && exported && entryTaken {
			boot.WriteString(fmt.Sprintf("\n; [外接脚本警告] %s: 入口 Register(api) 与其他载荷重名"+
				" (AHK 全局函数不可重复定义), 已整体跳过; 请改用唯一入口函数名", name))
			continue
		}
		if scanErr == nil && exported {
			entryTaken = true
		}
		rel := "scripts/" + filepath.ToSlash(name)
		inc.WriteString(fmt.Sprintf("\n#Include ../data/%s", rel))
		switch {
		case scanErr != nil:
			boot.WriteString(fmt.Sprintf("\n; [外接脚本警告] %s: 读取失败 (%v), 未校验入口契约, 仅函数库挂载", name, scanErr))
		case exported:
			boot.WriteString("\nPluginManager.Register(" + customManifestLiteral(id, base, rel) + ")")
			boot.WriteString(fmt.Sprintf("\nPluginManager.LoadEntry(%s)", ahkStringLit(id)))
		default:
			boot.WriteString(fmt.Sprintf("\n; [外接脚本] %s 未导出 Register(api), 仅作为函数库挂载", name))
		}
		dispatch = append(dispatch, [2]string{base, `A_ScriptDir "\..\data\scripts\` + ahkRawEscape(name) + `"`})
	}
	if len(dispatch) > 0 {
		fn.WriteString(renderCustomScriptRun(dispatch))
	}
	return inc.String(), boot.String(), fn.String()
}

// validCustomScriptName 外接脚本文件名校验: 仅接受平铺的 *.ahk (扩展名大小写不敏感,
// Windows 文件系统本就不敏感)。os.ReadDir 产物正常不含路径分隔符/盘符, 此处与
// plugins.safePluginRelFile 同为纵深防御; ".ahk"/"..ahk" 这类空基名一并拒绝
// (否则会合成 custom: / custom:. 这类坏 ID)。
func validCustomScriptName(name string) bool {
	ext := filepath.Ext(name)
	if !strings.EqualFold(ext, ".ahk") {
		return false
	}
	base := strings.TrimSuffix(name, ext)
	if base == "" || base == "." || base == ".." {
		return false
	}
	if strings.ContainsAny(name, `/\:`) || strings.ContainsRune(name, 0) {
		return false
	}
	return true
}

// exportsEntryFunc 校验脚本是否导出 L1 入口契约 (§4: Register(api))。
// 读取上限 customScanMaxBytes; BOM 剥离 (行首锚定才会命中首行定义)。
func exportsEntryFunc(path string) (bool, error) {
	f, err := os.Open(path)
	if err != nil {
		return false, err
	}
	defer f.Close()
	data, err := io.ReadAll(io.LimitReader(f, customScanMaxBytes))
	if err != nil {
		return false, err
	}
	data = bytes.TrimPrefix(data, []byte{0xEF, 0xBB, 0xBF})
	return customEntryRe.Match(data), nil
}

// customManifestLiteral 合成 L1 插件清单 (契约 §4 形态, 渲染为 AHK Map 字面量)。
// permissions 给满冻结词表: 外接脚本是用户自己的第一方代码, 信任级别等同
// custom_functions.ahk (本就可调用任意全局函数), 权限视图只为走统一 API 面。
func customManifestLiteral(id, name, relFile string) string {
	var b strings.Builder
	b.WriteString(`Map("id", ` + ahkStringLit(id))
	b.WriteString(`, "name", ` + ahkStringLit(name))
	b.WriteString(`, "specVersion", 1`)
	b.WriteString(`, "description", ` + ahkStringLit("外接脚本 (生成端合成的 L1 插件; 回调保持短平快, 长任务经 api.run.RunScript 走 ScriptHost 子进程)"))
	b.WriteString(`, "entry", Map("kind", "script", "file", ` + ahkStringLit(relFile) + `, "func", ` + ahkStringLit(customEntryFunc) + `)`)
	b.WriteString(`, "permissions", ["selection", "run", "clipboard", "window", "settings", "events"]`)
	b.WriteString(")")
	return b.String()
}

// ahkRawEscape 转义已处于 AHK 双引号字面量内的片段 (不包外层引号 —— 供拼接进
// `A_ScriptDir "\.."` 这类表达式值; ahkStringLit 会把实参整体变成纯字符串, 不适用)。
func ahkRawEscape(s string) string {
	s = strings.ReplaceAll(s, "`", "``")
	return strings.ReplaceAll(s, `"`, "`\"")
}

// renderCustomScriptRun 生成 CustomScriptRun(id) 名录分发函数: 数据式引用外接脚本
// (约束 7 —— ahkCode 里填 CustomScriptRun("foo") 即可, config.json 无代码载荷),
// 经 ScriptHost 以内置 AutoHotkey64.exe 子进程执行 (约束 4 —— 长任务不进主进程)。
// 双引号经 Chr(34) 构造 (repo 既有结论: """" 会解析为两个空字符串)。
func renderCustomScriptRun(pairs [][2]string) string {
	var b strings.Builder
	b.WriteString("\n; ============================================================")
	b.WriteString("\n; 外接脚本名录 (生成端编排, generators/customscripts.go): 长任务走 ScriptHost 子进程")
	b.WriteString("\n; 用法: 动作代码里填 CustomScriptRun(\"<文件名去扩展名>\", \"附加参数\"); 约束 7 下")
	b.WriteString("\n; 该引用是纯数据, 脚本本体在 data/scripts/ 下维护 (约束 4: 不阻塞主进程回调)。")
	b.WriteString("\n; ============================================================")
	b.WriteString("\nCustomScriptRun(id, args := \"\") {")
	b.WriteString("\n  static table := Map(")
	for i, p := range pairs {
		if i > 0 {
			b.WriteString(", ")
		}
		// p[0] = 名录 ID (纯字符串, 走 ahkStringLit); p[1] = 已构造好的路径表达式
		// (A_ScriptDir "\..", 必须原样输出, 不能再过 ahkStringLit)
		b.WriteString(ahkStringLit(p[0]) + ", " + p[1])
	}
	b.WriteString(")")
	b.WriteString("\n  path := table.Has(id) ? table[id] : \"\"")
	b.WriteString("\n  if (path == \"\")")
	b.WriteString("\n    return false")
	b.WriteString("\n  return ScriptHost.Run(A_ScriptDir \"\\AutoHotkey64.exe\", Chr(34) path Chr(34) (args != \"\" ? \" \" args : \"\"))")
	b.WriteString("\n}")
	return b.String()
}

// ensureCustomFunctionsStub 生成端自愈: 模板对 custom_functions.ahk 的 #Include 是
// 无条件的 (keyflux.tmpl 既有约定), 文件缺失 = 引擎脚本加载失败 —— 生成期补一份仅注释的
// 空桩; 已存在则绝不触碰 (用户函数库不可覆盖)。best-effort: 失败只记 stderr, 不向生成产物
// 写任何注释 (golden/parity 逐字节对账不被磁盘状态污染)。
func ensureCustomFunctionsStub(dir string) {
	// 数据目录自身不存在 = 未正确注入/空夹具, 保持全静默 (真实运行路径下目录恒存在:
	// config.json 就在其中; 不存在的父目录也无需自愈)
	if fi, err := os.Stat(dir); err != nil || !fi.IsDir() {
		return
	}
	path := filepath.Join(dir, customFunctionsFile)
	if _, err := os.Stat(path); err == nil {
		return
	}
	if err := os.WriteFile(path, []byte(customFunctionsStub), 0o644); err != nil {
		log.Printf("[customscripts] 自愈 %s 失败 (引擎脚本将因 #Include 指向缺失文件而无法加载): %v", path, err)
	}
}
