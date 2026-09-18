// 插件设置存储 —— data/plugin-settings.json 的读写端 (Go 侧)。
//
// 这是引擎侧 ConfigProvider.ahk 的**同一份文件、同一种格式**:
//
//	{"<pluginId>:<key>": "<value>", ...}
//
// (docs/CONTRACTS.md §3.8; 值一律字符串, 键空间按 pluginId 前缀隔离)。
// 之所以由 Go 承担写入而不是设置界面直写文件: 单一写入者 = 无跨进程写竞争,
// 且校验能集中在后端一处; AHK 侧只读 (everything_search 不写回设置)。
//
// 两个必须守住的细节:
//  1. 关闭 HTML 转义 —— Go 默认把 < > & 转成 \u003c 等, 而 AHK 侧的 _Unescape 只认识
//     \\ \" \n \r \t 五种序列, \uXXXX 会原样留在值里 (路径里出现该序列的概率不为零)。
//  2. 原子落盘 —— 先写临时文件再 rename 覆盖。AHK 在每次触发时都会全量读这个文件,
//     非原子写会让它读到半截 JSON 从而丢掉全部设置。
package plugins

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
)

// SettingsFileName 是 plugin-settings.json 的固定文件名 (与 ConfigProvider.ahk 约定一致)。
const SettingsFileName = "plugin-settings.json"

// SettingsStore 读写一份扁平键值设置文件。
// 零值不可用, 请用 NewSettingsStore; 内部有互斥锁, 可被多个 HTTP 请求并发复用。
type SettingsStore struct {
	Path string

	mu sync.Mutex
}

// NewSettingsStore 按文件路径构造 (路径由调用方给出, 便于测试注入临时目录)。
func NewSettingsStore(path string) *SettingsStore {
	return &SettingsStore{Path: path}
}

// Key 拼出该插件名下的完整存储键 ("<pluginId>:<key>")。
func Key(pluginID, key string) string {
	return pluginID + ":" + key
}

// splitKey 拆出键的属主插件 ID; 无冒号的键 (外部手写) 视为无主, 返回空串。
func splitKey(full string) string {
	i := strings.IndexByte(full, ':')
	if i <= 0 {
		return ""
	}
	return full[:i]
}

// LoadFor 读取该插件名下的设置 (返回值不带 pluginId 前缀)。
//
// 容错口径与 AHK ConfigProvider._ReadAll 一致: 文件不存在 / 读失败 / JSON 坏,
// 一律当作「没有设置」返回空表, 不向上抛错 —— 设置文件损坏不该让插件页打不开,
// 插件自身也都有默认值兜底。
func (s *SettingsStore) LoadFor(pluginID string) map[string]string {
	out := map[string]string{}
	for full, raw := range s.readAll() {
		if splitKey(full) != pluginID {
			continue
		}
		var v string
		if err := json.Unmarshal(raw, &v); err != nil {
			continue // 非字符串值 = 外人不按约定写的, 忽略而非报错
		}
		out[full[len(pluginID)+1:]] = v
	}
	return out
}

// Save 写入该插件的若干设置键。
//
// 语义: 只覆盖 values 里出现的键 (未提及的键原样保留, 含其它插件的键);
// 值为空串 = 删除该键 (回落 manifest 默认值, 也让文件不因「清空设置」而无限膨胀)。
// 合并在锁内完成 (读-改-写), 避免并发 PUT 互相覆盖。
func (s *SettingsStore) Save(pluginID string, values map[string]string) error {
	if !idPattern.MatchString(pluginID) {
		return fmt.Errorf("插件 ID %q 不合法", pluginID)
	}
	s.mu.Lock()
	defer s.mu.Unlock()

	raw := s.readAll()
	for k, v := range values {
		full := Key(pluginID, k)
		if v == "" {
			delete(raw, full)
			continue
		}
		enc, err := marshalStringNoHTML(v)
		if err != nil {
			return err
		}
		raw[full] = enc
	}
	return s.writeAll(raw)
}

// ---------------- 内部 ----------------

// readAll 读出全部原始键值 (值保留 JSON 原文, 以便原样回写非字符串条目)。
// 任何读取/解析问题都退化为空表。
func (s *SettingsStore) readAll() map[string]json.RawMessage {
	out := map[string]json.RawMessage{}
	data, err := os.ReadFile(s.Path)
	if err != nil {
		return out
	}
	data = bytes.TrimPrefix(data, []byte{0xEF, 0xBB, 0xBF}) // 防手写文件带 BOM
	if len(bytes.TrimSpace(data)) == 0 {
		return out
	}
	// 顶层必须是对象; 数组/标量等异形内容一律忽略 (不原地清零, 见 writeAll 的保留策略)
	if err := json.Unmarshal(data, &out); err != nil {
		return map[string]json.RawMessage{}
	}
	return out
}

// writeAll 原子写入 (临时文件 -> rename)。键按字典序输出, 保证同内容同字节。
func (s *SettingsStore) writeAll(raw map[string]json.RawMessage) error {
	dir := filepath.Dir(s.Path)
	if dir != "" && dir != "." {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			return fmt.Errorf("创建设置目录失败: %w", err)
		}
	}

	var buf bytes.Buffer
	if len(raw) == 0 {
		buf.WriteString("{}\n")
	} else {
		enc := json.NewEncoder(&buf)
		enc.SetEscapeHTML(false) // 见文件头「细节 1」: AHK 只认标准五种转义
		enc.SetIndent("", "  ")
		if err := enc.Encode(sortedRaw(raw)); err != nil {
			return err
		}
	}

	tmp, err := os.CreateTemp(dir, ".plugin-settings-*.tmp")
	if err != nil {
		return fmt.Errorf("创建设置临时文件失败: %w", err)
	}
	tmpName := tmp.Name()
	defer func() {
		_ = tmp.Close()
		_ = os.Remove(tmpName) // rename 成功后这里是空操作
	}()
	if _, err := tmp.Write(buf.Bytes()); err != nil {
		return fmt.Errorf("写设置临时文件失败: %w", err)
	}
	if err := tmp.Sync(); err != nil {
		return fmt.Errorf("刷盘失败: %w", err)
	}
	if err := tmp.Close(); err != nil {
		return fmt.Errorf("关闭设置临时文件失败: %w", err)
	}
	if err := os.Rename(tmpName, s.Path); err != nil {
		return fmt.Errorf("替换设置文件失败: %w", err)
	}
	return nil
}

// rawPair 是键值对 (键有序, 值保留 JSON 原文)。
type rawPair struct {
	Key   string
	Value json.RawMessage
}

// rawPairs 是有序的键值对序列; 自定义 MarshalJSON 让 Encoder 不重排 (切片顺序即输出顺序)。
type rawPairs []rawPair

func sortedRaw(raw map[string]json.RawMessage) rawPairs {
	keys := make([]string, 0, len(raw))
	for k := range raw {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	out := make(rawPairs, 0, len(keys))
	for _, k := range keys {
		out = append(out, rawPair{Key: k, Value: raw[k]})
	}
	return out
}

// MarshalJSON 把 rawPairs 序列化成 JSON 对象 (保持切片顺序)。
func (p rawPairs) MarshalJSON() ([]byte, error) {
	var buf bytes.Buffer
	buf.WriteByte('{')
	for i, kv := range p {
		if i > 0 {
			buf.WriteByte(',')
		}
		k, err := marshalStringNoHTML(kv.Key)
		if err != nil {
			return nil, err
		}
		buf.Write(k)
		buf.WriteByte(':')
		if len(kv.Value) == 0 {
			buf.WriteString(`""`)
		} else {
			buf.Write(kv.Value)
		}
	}
	buf.WriteByte('}')
	return buf.Bytes(), nil
}

// marshalStringNoHTML 序列化单个字符串且不转义 < > & (见文件头「细节 1」)。
func marshalStringNoHTML(s string) ([]byte, error) {
	var buf bytes.Buffer
	enc := json.NewEncoder(&buf)
	enc.SetEscapeHTML(false)
	if err := enc.Encode(s); err != nil {
		return nil, err
	}
	return bytes.TrimRight(buf.Bytes(), "\n"), nil
}

// ErrPluginNotInstalled 设置读写目标插件不在目录中时返回 (handler 映射为 404)。
var ErrPluginNotInstalled = errors.New("插件不存在")
