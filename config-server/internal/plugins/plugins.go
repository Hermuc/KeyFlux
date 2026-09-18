// Package plugins 实现插件页的「第三方插件包」体系。
//
// 插件 = 一个自描述目录包: plugin.json (manifest) + 资源/脚本文件。
// 与行为包 (internal/behaviors) 的目录形态刻意一致, 但语义不同:
//   - 行为包是「选中动作」的执行单元 (引擎渲染期展开);
//   - 插件包是独立功能单元 (如 QuickSwitch), 由插件运行时按需加载。
//
// 现状 (阶段 1): 本包只承载「管理面」——清单校验 / 导入安装 / 卸载 / 列表。
// 引擎侧插件运行时 (script entry 的编译期 Include 与生命周期) 为阶段 2,
// 故导入的插件暂不参与脚本生成, options.plugins.disabled 仅记录启停意愿。
//
// 核心不变量:
//   - 插件 ID 与行为包同款命名空间规则 (^[a-z][a-z0-9_]{0,31}$), 且不得占用内置插件 ID;
//   - manifest 的 id 必须与目录名一致 (导入时按 manifest id 落盘);
//   - zip 导入经临时目录 + rename 原子落盘, 全程拒绝路径穿越 (zip-slip)。
package plugins

import (
	"archive/zip"
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"unicode/utf8"
)

// SpecVersion 当前插件包格式版本; 与包内 specVersion 字段不一致即拒绝 (前向兼容锚点)。
const SpecVersion = 1

var idPattern = regexp.MustCompile(`^[a-z][a-z0-9_]{0,31}$`)

// BuiltinPluginIDs 内置插件保留 ID 集 (引擎随软件分发的插件, 配置段独立管理,
// 不进入用户插件目录)。用户包 ID 不得占用。
var BuiltinPluginIDs = map[string]bool{
	"quick_switch": true,
}

// Entry 插件入口声明。阶段 1 仅接受 script 形态 (file + func);
// 运行时加载为阶段 2, 校验通过即视为合法声明。
type Entry struct {
	Kind string `json:"kind"`           // "script"
	File string `json:"file,omitempty"` // script: 入口脚本文件名 (包内相对路径)
	Func string `json:"func,omitempty"` // script: 入口函数名
}

// 设置项类型词表。词表是协议的一部分: 设置界面按 type 决定编辑器控件,
// 后端按 type 决定值校验口径 —— 两端必须一致, 故在此收口。
const (
	SettingTypeChar   = "char"   // 单个可打印字符 (如触发键)
	SettingTypeText   = "text"   // 任意短文本
	SettingTypeNumber = "number" // 整数 (可带 min/max)
	SettingTypeFile   = "file"   // 本地文件路径 (界面上给文件选择器)
)

// settingTypes 合法类型集。
var settingTypes = map[string]bool{
	SettingTypeChar:   true,
	SettingTypeText:   true,
	SettingTypeNumber: true,
	SettingTypeFile:   true,
}

// SettingsPermission 声明 settings 所必需的能力位 (值必须为单字符)。
const SettingsPermission = "settings"

// MaxSettingsPerPlugin 单个插件可声明的设置项上限 (防 manifest 失控)。
const MaxSettingsPerPlugin = 32

// MaxSettingValueLen 单个设置值的长度上限 (字符数; 路径/文本共用一条够用的线)。
const MaxSettingValueLen = 1024

var settingKeyPattern = regexp.MustCompile(`^[A-Za-z][A-Za-z0-9_]{0,31}$`)

// Setting 插件设置项声明 (manifest.settings[])。
//
// 这是「声明式设置」契约: manifest 只描述**有哪些设置、长什么样、默认值多少**,
// 真实值存在 data/plugin-settings.json (见 store.go), 由设置界面写入、引擎侧
// ConfigProvider.ahk 读取 —— 两端的键空间都是 "<pluginId>:<key>"。
type Setting struct {
	Key     string `json:"key"`
	Type    string `json:"type"`
	Label   string `json:"label"`
	LabelEn string `json:"labelEn,omitempty"`
	Default string `json:"default,omitempty"`
	// Filter 仅 type=file 使用: 文件选择器的类型过滤名 (如 "everything.exe"),
	// 同时充当界面上的后缀提示。空 = 不过滤。
	Filter string `json:"filter,omitempty"`
	Hint   string `json:"hint,omitempty"`
	HintEn string `json:"hintEn,omitempty"`
	// Min/Max 仅 type=number 使用 (闭区间, 整数); nil = 不限。
	Min *float64 `json:"min,omitempty"`
	Max *float64 `json:"max,omitempty"`
	// MaxLength 仅 type=text 使用: 值长度上限 (0 = 用 MaxSettingValueLen)。
	MaxLength int `json:"maxLength,omitempty"`
}

// ValueLimit 该设置项允许的最大值长度 (字符数)。
func (s Setting) ValueLimit() int {
	if s.Type == SettingTypeChar {
		return 1
	}
	if s.Type == SettingTypeText && s.MaxLength > 0 && s.MaxLength < MaxSettingValueLen {
		return s.MaxLength
	}
	return MaxSettingValueLen
}

// Manifest 插件包 manifest (plugin.json)。
type Manifest struct {
	ID          string    `json:"id"`
	Name        string    `json:"name"`
	NameEn      string    `json:"nameEn,omitempty"`
	Version     string    `json:"version,omitempty"`
	SpecVersion int       `json:"specVersion"`
	Description string    `json:"description,omitempty"`
	Author      string    `json:"author,omitempty"`
	Entry       Entry     `json:"entry"`
	Permissions []string  `json:"permissions,omitempty"`
	Settings    []Setting `json:"settings,omitempty"`
}

// HasPermission 该 manifest 是否声明了指定能力位。
func (m *Manifest) HasPermission(name string) bool {
	for _, p := range m.Permissions {
		if p == name {
			return true
		}
	}
	return false
}

// SettingByKey 按 key 取设置项声明 (不存在返回 nil, false)。
func (m *Manifest) SettingByKey(key string) (Setting, bool) {
	for _, s := range m.Settings {
		if s.Key == key {
			return s, true
		}
	}
	return Setting{}, false
}

// AllowedSettings 该 manifest 声明的全部设置键 (校验 PUT 请求用)。
func (m *Manifest) AllowedSettings() map[string]Setting {
	out := make(map[string]Setting, len(m.Settings))
	for _, s := range m.Settings {
		out[s.Key] = s
	}
	return out
}

// Catalog 用户插件目录快照: 按 ID 字典序 + 逐包错误隔离 (坏包不拖垮整个目录)。
type Catalog struct {
	Plugins []*Manifest
	Errors  []string
}

// ValidateManifest 校验 manifest 结构合法性 (加载期与导入 API 共用)。
func ValidateManifest(p *Manifest) error {
	if !idPattern.MatchString(p.ID) {
		return fmt.Errorf("插件 ID %q 不合法 (须匹配 ^[a-z][a-z0-9_]{0,31}$)", p.ID)
	}
	if BuiltinPluginIDs[p.ID] {
		return fmt.Errorf("插件 ID %q 与内置插件冲突", p.ID)
	}
	if p.SpecVersion != SpecVersion {
		return fmt.Errorf("specVersion 必须为 %d (当前 %d)", SpecVersion, p.SpecVersion)
	}
	if strings.TrimSpace(p.Name) == "" {
		return fmt.Errorf("插件「%s」缺少名称 (name)", p.ID)
	}
	switch p.Entry.Kind {
	case "script":
		if strings.TrimSpace(p.Entry.File) == "" || strings.TrimSpace(p.Entry.Func) == "" {
			return fmt.Errorf("插件「%s」的 script entry 缺少 file 或 func", p.ID)
		}
	default:
		return fmt.Errorf("插件「%s」的 entry.kind %q 不合法 (当前仅支持 script)", p.ID, p.Entry.Kind)
	}
	return validateSettings(p)
}

// validateSettings 校验声明式设置块。
//
// 刻意在加载期 (而非保存期) 就把非法声明拦掉: manifest 是设置界面的渲染契约,
// 一处坏声明 (如 type 拼错) 会让界面渲染出无法编辑的控件, 而错误要到用户点开
// 才暴露 —— 坏包在目录加载时即隔离并计入 errors 更省事。
func validateSettings(p *Manifest) error {
	if len(p.Settings) == 0 {
		return nil
	}
	if len(p.Settings) > MaxSettingsPerPlugin {
		return fmt.Errorf("插件「%s」声明的设置项过多 (%d > %d)", p.ID, len(p.Settings), MaxSettingsPerPlugin)
	}
	// 设置值经 APIBridge 的 config.* 读写, 必须持有 settings 能力位,
	// 否则插件读得到声明却读不到值 —— 属于声明自相矛盾, 直接拒绝。
	if !p.HasPermission(SettingsPermission) {
		return fmt.Errorf("插件「%s」声明了 settings 但未申请 %q 权限", p.ID, SettingsPermission)
	}
	seen := make(map[string]bool, len(p.Settings))
	for i, s := range p.Settings {
		if !settingKeyPattern.MatchString(s.Key) {
			return fmt.Errorf("插件「%s」第 %d 个设置项的 key %q 不合法 (须匹配 ^[A-Za-z][A-Za-z0-9_]{0,31}$)", p.ID, i+1, s.Key)
		}
		if seen[s.Key] {
			return fmt.Errorf("插件「%s」设置项 key %q 重复", p.ID, s.Key)
		}
		seen[s.Key] = true
		if !settingTypes[s.Type] {
			return fmt.Errorf("插件「%s」设置项 %q 的 type %q 不合法 (仅支持 char/text/number/file)", p.ID, s.Key, s.Type)
		}
		if strings.TrimSpace(s.Label) == "" {
			return fmt.Errorf("插件「%s」设置项 %q 缺少 label", p.ID, s.Key)
		}
		if s.Min != nil && s.Max != nil && *s.Min > *s.Max {
			return fmt.Errorf("插件「%s」设置项 %q 的 min 大于 max", p.ID, s.Key)
		}
		if s.Type != SettingTypeNumber && (s.Min != nil || s.Max != nil) {
			return fmt.Errorf("插件「%s」设置项 %q 不是 number 类型, 不应带 min/max", p.ID, s.Key)
		}
		if s.Type != SettingTypeFile && s.Filter != "" {
			return fmt.Errorf("插件「%s」设置项 %q 不是 file 类型, 不应带 filter", p.ID, s.Key)
		}
		// number 是整数语义, 小数边界会让界面与后端校验口径分叉
		for _, v := range []*float64{s.Min, s.Max} {
			if v != nil && *v != float64(int64(*v)) {
				return fmt.Errorf("插件「%s」设置项 %q 的 min/max 必须是整数", p.ID, s.Key)
			}
		}
		// 默认值必须自洽, 否则界面一打开就显示一个存不进去的值
		if err := ValidateSettingValue(s, s.Default); err != nil {
			return fmt.Errorf("插件「%s」设置项 %q 的默认值不合法: %w", p.ID, s.Key, err)
		}
	}
	return nil
}

// ValidateSettingValue 校验单个值是否符合设置项声明 (PUT 请求与默认值自查共用)。
// 约定: 空串一律合法 (语义 = 未设置, 回落 Default); 非空串按类型逐项校验。
func ValidateSettingValue(s Setting, value string) error {
	if value == "" {
		return nil
	}
	if utf8.RuneCountInString(value) > s.ValueLimit() {
		return fmt.Errorf("%q 超过长度上限 %d", value, s.ValueLimit())
	}
	if strings.ContainsRune(value, 0) {
		return errors.New("值不能包含 NUL 字符")
	}
	switch s.Type {
	case SettingTypeChar:
		// 单字符 + 必须可打印: 控制字符 (换行/制表) 做不了触发键, 放进来只会得到
		// 一个「看着有值却永远触发不了」的设置。
		r := []rune(value)[0]
		if r < 0x20 || r == 0x7F {
			return fmt.Errorf("%q 不是可打印字符", value)
		}
	case SettingTypeNumber:
		n, err := strconv.ParseInt(value, 10, 64)
		if err != nil {
			return fmt.Errorf("%q 不是整数", value)
		}
		if s.Min != nil && float64(n) < *s.Min {
			return fmt.Errorf("%d 小于下限 %v", n, *s.Min)
		}
		if s.Max != nil && float64(n) > *s.Max {
			return fmt.Errorf("%d 大于上限 %v", n, *s.Max)
		}
	}
	return nil
}

// LoadCatalog 加载用户插件目录 (config.json 同级 plugins/, 缺目录 = 空)。
func LoadCatalog(userDir string) *Catalog {
	c := &Catalog{}
	entries, err := os.ReadDir(userDir)
	if err != nil {
		if !os.IsNotExist(err) {
			c.Errors = append(c.Errors, fmt.Sprintf("读取插件目录失败: %v", err))
		}
		return c
	}
	for _, de := range entries {
		if !de.IsDir() || strings.HasPrefix(de.Name(), ".") {
			continue // 点前缀 = 导入临时目录 (中断残留), 不入目录
		}
		m, err := readManifest(filepath.Join(userDir, de.Name()))
		if err != nil {
			c.Errors = append(c.Errors, fmt.Sprintf("%s: %v", de.Name(), err))
			continue
		}
		c.Plugins = append(c.Plugins, m)
	}
	sort.SliceStable(c.Plugins, func(i, j int) bool { return c.Plugins[i].ID < c.Plugins[j].ID })
	return c
}

// readManifest 读取单个插件目录的 plugin.json (防误存 BOM, 目录名须与 id 一致)。
func readManifest(dir string) (*Manifest, error) {
	raw, err := os.ReadFile(filepath.Join(dir, "plugin.json"))
	if err != nil {
		return nil, err
	}
	m, err := parseManifest(raw)
	if err != nil {
		return nil, err
	}
	if m.ID != filepath.Base(dir) {
		return nil, fmt.Errorf("目录名 %q 与 manifest id %q 不一致", filepath.Base(dir), m.ID)
	}
	return m, nil
}

// parseManifest 解析并校验 manifest 字节 (导入路径与目录加载共用;
// 导入时文件尚在临时目录, 目录名比对由调用方按最终落盘位置另行保证)。
func parseManifest(raw []byte) (*Manifest, error) {
	raw = bytes.TrimPrefix(raw, []byte{0xEF, 0xBB, 0xBF})
	var m Manifest
	if err := json.Unmarshal(raw, &m); err != nil {
		return nil, fmt.Errorf("plugin.json 解析失败: %w", err)
	}
	if err := ValidateManifest(&m); err != nil {
		return nil, err
	}
	return &m, nil
}

// --------------------------------------------------------------- 导入 (zip)

// zip 导入的防护上限: 防解压炸弹 (数量/总量/单文件)。
const (
	maxZipEntries = 500
	maxZipTotal   = 64 << 20 // 64 MB
	maxZipFile    = 32 << 20 // 32 MB
)

// InstallFromZip 从 zip 数据流安装插件: 解压到临时目录 -> 校验 manifest ->
// 按 manifest id 原子落盘到 userDir/<id>。zip 根可直接是包内容, 也可带唯一顶层目录。
func InstallFromZip(r io.Reader, userDir string) (*Manifest, error) {
	tmpDir, err := os.MkdirTemp(userDir, ".import-")
	if err != nil {
		// 父目录可能不存在 (首次导入)
		if os.MkdirAll(userDir, 0o755) != nil {
			return nil, err
		}
		tmpDir, err = os.MkdirTemp(userDir, ".import-")
		if err != nil {
			return nil, err
		}
	}
	defer os.RemoveAll(tmpDir) // 失败即清空临时目录; 成功路径已 rename 走

	if err := extractZip(r, tmpDir); err != nil {
		return nil, err
	}

	// 定位 plugin.json: 优先解压根, 其次唯一子目录 (GitHub 源码 zip 带顶层目录)。
	root := tmpDir
	if _, err := os.Stat(filepath.Join(root, "plugin.json")); err != nil {
		entries, err := os.ReadDir(root)
		if err != nil || len(entries) != 1 || !entries[0].IsDir() {
			return nil, errors.New("zip 中找不到 plugin.json (应在压缩包根或唯一顶层目录下)")
		}
		root = filepath.Join(root, entries[0].Name())
		if _, err := os.Stat(filepath.Join(root, "plugin.json")); err != nil {
			return nil, errors.New("zip 中找不到 plugin.json (应在压缩包根或唯一顶层目录下)")
		}
	}

	raw, err := os.ReadFile(filepath.Join(root, "plugin.json"))
	if err != nil {
		return nil, err
	}
	m, err := parseManifest(raw)
	if err != nil {
		return nil, err
	}

	dest := filepath.Join(userDir, m.ID)
	if _, err := os.Stat(dest); err == nil {
		return nil, fmt.Errorf("插件「%s」(ID %s) 已存在, 如需覆盖请先删除", m.Name, m.ID)
	}
	if err := os.Rename(root, dest); err != nil {
		return nil, err
	}
	return m, nil
}

// extractZip 安全解压: 拒绝路径穿越 (zip-slip) / 绝对路径 / 盘符, 限量防炸弹。
func extractZip(r io.Reader, dest string) error {
	// zip.NewReader 需要 ReaderAt + 尺寸, 先读入内存 (有 maxZipTotal 上限兜底)
	data, err := io.ReadAll(io.LimitReader(r, maxZipTotal+1))
	if err != nil {
		return err
	}
	if len(data) > maxZipTotal {
		return fmt.Errorf("插件包超过大小上限 (%d MB)", maxZipTotal>>20)
	}
	zr, err := zip.NewReader(bytes.NewReader(data), int64(len(data)))
	if err != nil {
		return fmt.Errorf("不是有效的 zip 文件: %w", err)
	}
	if len(zr.File) > maxZipEntries {
		return fmt.Errorf("插件包文件数超过上限 (%d)", maxZipEntries)
	}

	total := uint64(0)
	for _, f := range zr.File {
		name := filepath.ToSlash(f.Name)
		if !safeZipName(name) {
			return fmt.Errorf("插件包含不安全路径: %q", f.Name)
		}
		if f.FileInfo().IsDir() {
			continue
		}
		if uint64(f.UncompressedSize64) > maxZipFile {
			return fmt.Errorf("插件包内单文件超过上限 (%d MB): %s", maxZipFile>>20, f.Name)
		}
		if err := extractZipFile(f, filepath.Join(dest, filepath.FromSlash(name))); err != nil {
			return err
		}
		total += f.UncompressedSize64
		if total > maxZipTotal {
			return fmt.Errorf("插件包解压总量超过上限 (%d MB)", maxZipTotal>>20)
		}
	}
	return nil
}

// safeZipName 拒绝绝对路径、盘符与 .. 穿越 (仅接受包内相对路径)。
func safeZipName(name string) bool {
	if name == "" || strings.HasPrefix(name, "/") || strings.HasPrefix(name, "\\") {
		return false
	}
	if len(name) >= 2 && name[1] == ':' { // Windows 盘符 (C:/...)
		return false
	}
	for _, seg := range strings.Split(name, "/") {
		if seg == ".." {
			return false
		}
	}
	return true
}

func extractZipFile(f *zip.File, dest string) error {
	if err := os.MkdirAll(filepath.Dir(dest), 0o755); err != nil {
		return err
	}
	rc, err := f.Open()
	if err != nil {
		return err
	}
	defer rc.Close()

	out, err := os.OpenFile(dest, os.O_WRONLY|os.O_CREATE|os.O_TRUNC, 0o644)
	if err != nil {
		return err
	}
	defer out.Close()
	_, err = io.Copy(out, rc)
	return err
}

// Remove 删除用户插件目录 (id 已由正则保证无路径穿越字符; 内置 ID 双重拒绝)。
func Remove(userDir, id string) error {
	if BuiltinPluginIDs[id] {
		return fmt.Errorf("插件 ID %q 与内置插件冲突", id)
	}
	if !idPattern.MatchString(id) {
		return fmt.Errorf("插件 ID %q 不合法", id)
	}
	dir := filepath.Join(userDir, id)
	if _, err := os.Stat(dir); err != nil {
		return fmt.Errorf("插件「%s」不存在", id)
	}
	return os.RemoveAll(dir)
}
