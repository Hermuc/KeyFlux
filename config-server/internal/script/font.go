package script

import (
	"encoding/binary"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
)

// FontTargetRel 命令框字体的落地相对路径。
//
// 🔴 该路径**不可配置**: 命令框 exe 内以 UTF-16 字面量硬编码 `font\font.ttf`
// (RVA 0x1ddc8), 相对 exe 自身目录解析 ⇒ 实际文件恒为 <部署树>/bin/font/font.ttf。
// 详见 docs/CONTRACTS.md §3.11.1。
const FontTargetRel = "font/font.ttf"

// FontMaxBytes 字体文件体积上限 (32 MiB)。
//
// 宽松上限, 仅用于挡明显的误选 (选中整个 ISO/视频后复制等于把部署目录撑爆)。
// 历代字体实测: Iosevka 0.5MB / 得意黑 2.6MB / MiSans 7.8MB / Sthginkra 10.5MB /
// Sthginkra 加粗 21.2MB —— 32 MiB 留足余量。
const FontMaxBytes = 32 * 1024 * 1024

// fontCollectionFaceLimit 字体集合 (.ttc/.otc) 允许的最大 face 数。
//
// 与 C# 侧 `CommandFontValidator.FaceCountProbeLimit` **同口径** (两边都是 4096):
// 同一个文件在 UI 侧与生成侧必须得出相同结论, 否则会出现"界面说可用、生成端却拒绝"
// (或反之) 的矛盾反馈 —— 这类漂移是双向的, 改一处必须同时看另一处。
const fontCollectionFaceLimit = 4096

// FontWeightVariants 字重档位 -> 预烘焙变体文件名 (空串 = 用源字体本体)。
//
// 机制: 轮廓膨胀/腐蚀 (几何加粗/减细) 需要布尔运算, 仓库的 Go 侧没有该能力, 也不该
// 为此引入外部进程依赖 (会破坏部署树的可移植性)。故把 5 档**在构建期**烘焙成独立
// 文件, 运行时只按用户所选档位挑一个现成文件复制。
//
// 变体由 `tools/font_weight_prebake.py` 从**用户所选源字体**生成, 与源字体同目录
// (见 VariantPath 命名约定 <源文件名去扩展名>.<档位>.ttf)。
//
// 档位设计 (半径见 prebake 的 WEIGHT_RADII):
//
//	thin     -20  腐蚀减细
//	light    -10  腐蚀减细
//	regular    0  源字体本体
//	semibold  14  膨胀加粗
//	bold      28  膨胀加粗
//
// ⚠ thin/light 靠**腐蚀**得到, 用于源字体本身就是粗体 (如 usWeightClass=700) 时提供
// 更细的选择 —— 对粗体源, 单纯"不膨胀"只是回到源字体, 无法比源字体更细。
//
// ➜ regular 刻意映射空串: 半径 0 就是源字体本身, 直接复制源文件即可,
// 不落冗余副本 (省磁盘也少一处可能不同步的状态)。
var FontWeightVariants = map[string]string{
	"thin":     "thin",
	"light":    "light",
	"regular":  "",
	"semibold": "semibold",
	"bold":     "bold",
}

// NormalizeFontWeight 把配置里的字重值规范成已知档位名。
//
// 未知/空值一律回落 "regular"。宁可回落中性档, 也不猜一个可能更粗的档位
// (猜粗会在某些字体上把 CJK 字腔填死 —— 实测 Sarasa r=+28 会把「令」的字腔压到 32px)。
//
// ⚠ 与 C# 侧 ConfigReadDefaults.DefaultCommandFontWeight (`semibold`) **刻意不同口径**:
// 两侧职责不同。C# 的常量是"UI 层默认值"——用户在界面上看到并保存的就是它; Go 的回落
// 是"最后兜底"——只有在配置里出现**未知档位名**(正常情况下 UI 不可能写出这种值)时才生效。
// 即: 正常路径 = UI 写入 semibold ⇒ Go 读到已知档位, 直接透传, 两侧表现一致;
// 异常路径 = 配置被手改/降级 ⇒ Go 取中性 regular, 而不是跟随 UI 的偏好值。
// 这样"UI 默认"与"数据损坏兜底"两件事就不会互相绑死 (改 UI 默认不需要动 Go)。
func NormalizeFontWeight(w string) string {
	if _, ok := FontWeightVariants[w]; ok {
		return w
	}
	return "regular"
}

// VariantPath 推导某档位变体相对源字体的落点: <去扩展名>.<档位>.ttf。
//
// 约定放在源字体同目录, 理由: 源字体在用户自己的目录里 (可能被移动/删除), 变体跟着它
// 才能保证"源还在则变体也在"; 放程序目录反而会在换源字体后留下无人清理的孤儿。
// regular 无变体, 返回空串。
func VariantPath(src, weight string) string {
	v, ok := FontWeightVariants[NormalizeFontWeight(weight)]
	if !ok || v == "" {
		return ""
	}
	ext := filepath.Ext(src)
	return src[:len(src)-len(ext)] + "." + v + ext
}

// InstallCommandFont 按配置把用户选定的字体文件复制到命令框字体落点。
//
// 调用时机: 生成端 (GenerateScripts / GenerateAHK) 每轮生成时执行一次 —— 字体只在
// 命令框进程启动时被读取一次 (见 §3.11.1 硬约束 7), 故换字体后须重启命令框进程才生效。
//
// 失败**一律静默跳过** (返回 nil), 绝不阻断生成流程。理由: 字体是纯表现层资源,
// 用户可能已经把源文件删了/挪了/选了不支持的文件, 这些都不该让 AHK 脚本生成失败。
// 跳过时命令框沿用上一次成功落地的 font.ttf (即"回退到既有字体"), 行为可预期。
//
// 边界 (逐条):
//  1. SourcePath 为空      -> 未自定义, 不动部署目录的 font.ttf (沿用现状/人工放置)。
//  2. 源文件不存在/不可读   -> 跳过。
//  3. 源文件体积 > 上限     -> 跳过 (疑似误选非字体文件)。
//  4. 源轮廓格式不受支持 -> 跳过 (仅接受 glyf: TrueType 0x00010000 / 'true' /
//     集合 'ttcf' 且首 face 为 glyf; **CFF 'OTTO' 一律拒绝** —— exe 硬编码
//     TrueType face 类型, CFF 会在下游静默加载失败)。
//  5. 源已就位于目标路径     -> 跳过 (避免自复制把文件截断为 0 字节)。
//  6. 目标目录不存在         -> 先建目录再复制 (老部署树可能没有 bin/font/)。
//  7. 字重档位有预烘焙变体但变体文件缺失 -> **回落源字体本体** (见下"字重"段)。
//
// baseDir 同时用于解析相对路径与推导落点:
//   - 运行时 (GenerateScripts) cwd = bin ⇒ 传 "" (即 "."), 落点 "font/font.ttf" 正好
//     落在 bin/font/font.ttf;
//   - CLI (GenerateAHK) 落点应跟随**输出文件**所在目录 (部署树的 bin/), 故传其目录,
//     相对 SourcePath 也一并以该目录为基准解析。
//
// 字重 (opt.Weight): 取真正参与渲染的实际字形来源。
// 膨胀过的字重档位对应一个**预烘焙变体文件** (由 tools/font_weight_prebake.py 生成,
// 与源字体同目录, 命名 <源名>.<档位>.ttf), 本函数优先用它; 变体不存在则回落源字体。
//   - 为何"预烘焙"而非"运行时膨胀": 几何加粗要轮廓布尔运算, Go 侧无此能力; 调外部
//     Python 会让部署树依赖目标机器装有 Python+fontTools+pathops, 破坏可移植性。
//   - 为何缺变体只回落而**不报错**: 用户可能直接从别处拷来一份字体而未烘焙变体,
//     此时"能显示原样字体"远好于"整条链路失败"。回落口径让行为始终可预期。
//
// 返回错误仅供调试/未来门禁使用; 当前调用方刻意忽略 (见上"静默跳过")。
func InstallCommandFont(opt CommandFontOption, baseDir string) error {
	if opt.SourcePath == "" {
		return nil // 1. 未自定义
	}

	src := opt.SourcePath
	if !filepath.IsAbs(src) && baseDir != "" {
		src = filepath.Join(baseDir, src)
	}
	dst := FontTargetRel
	if baseDir != "" {
		dst = filepath.Join(baseDir, FontTargetRel)
	}

	srcInfo, err := os.Stat(src)
	if err != nil || srcInfo.IsDir() {
		return fmt.Errorf("命令框字体源不可用, 沿用现有字体: %s: %v", src, err) // 2.
	}
	if srcInfo.Size() > FontMaxBytes {
		return fmt.Errorf("命令框字体源超过 %d 字节上限 (%d), 疑似误选, 沿用现有字体: %s",
			FontMaxBytes, srcInfo.Size(), src) // 3.
	}

	// 字重: 先按档位试预烘焙变体, 缺失则回落源字体 (见函数头"字重"段)。
	// 变体必须**自身也通过格式校验**才采用 —— 用户可能手工塞进一个同名垃圾文件,
	// 此时若直接采用, 下面的格式闸门会拒绝它并让整次安装失败 (而正确行为是回落源字体)。
	weight := NormalizeFontWeight(opt.Weight)
	chosen := src
	if vp := VariantPath(src, weight); vp != "" {
		if vi, err := os.Stat(vp); err == nil && !vi.IsDir() && vi.Size() <= FontMaxBytes {
			if ok, _, err := classifyFontKinds(vp, 0); err == nil && ok {
				chosen = vp
			}
		}
	}

	// 5. 源已就位: 先于签名校验判定 —— 自复制会把目标截断为 0 字节。
	//    (按 chosen 判定: 若落点正是被选中的那份, 复制等于自截断。)
	if same, err := samePath(chosen, dst); err == nil && same {
		return nil
	}

	// 4. 字体格式校验: 必须是命令框 exe 能加载的 glyf 轮廓。
	//    (同时挡住复制自身这类边界。)
	ok, reason, err := classifyFontKinds(chosen, 0)
	if err != nil {
		return fmt.Errorf("命令框字体源读取失败, 沿用现有字体: %s: %v", chosen, err)
	}
	if !ok {
		return fmt.Errorf("命令框字体源不被接受, 沿用现有字体: %s: %s", chosen, reason)
	}

	if err := os.MkdirAll(filepath.Dir(dst), 0o755); err != nil { // 6.
		return fmt.Errorf("命令框字体落点目录创建失败: %v", err)
	}

	data, err := os.ReadFile(chosen)
	if err != nil {
		return fmt.Errorf("命令框字体读取失败: %v", err)
	}
	// 落点权限跟随可执行文件的常规权限口径 (0644); 非 0600 —— 部署目录为便携式安装,
	// 无多用户隔离需求, 且 0600 在跨用户复制部署树时易踩权限坑。
	if err := os.WriteFile(dst, data, 0o644); err != nil {
		return fmt.Errorf("命令框字体写入失败: %v", err)
	}
	return nil
}

// CommandBoxAppearance 命令框外观的两段配置 —— 字体 (options.commandFont) 与
// 皮肤 (options.commandInputSkin)。
//
// 为何把两者并成一个快照: 它们的**生效条件完全相同** —— 命令框 exe 只在进程启动时
// 读一次 `bin/font/font.ttf` 与 `bin/CommandInputSkin.txt` (DirectWrite 私有字体集合
// 与皮肤参数在进程内常驻; 皮肤读取时机已于 2026-09-21 用文件最后访问时间实证:
// 启动后 0.1s 读一次, 之后不再读)。故任一段变化后都必须结束命令框进程才能让新值可见
// (契约 §3.11.1 硬约束 7)。一次读取也保证两段来自同一文件快照, 不会撕裂。
type CommandBoxAppearance struct {
	Font CommandFontOption
	Skin CommandInputSkin
}

// CommandBoxAppearanceFromConfigFile 从**已落盘**的 config.json 读出命令框外观两段。
//
// 用途: 保存配置的处理器据此判断「外观是否真的变了」, 决定要不要结束命令框进程。
//
// 容错口径: 任何读取/解析失败一律返回**零值**。零值 (字体 SourcePath 为空 + 皮肤 18 键
// 全空) 与用户的实际选择必然不等 ⇒ 「读不到旧配置」会被判成「变了」, 走保守分支
// (结束命令框让其重建) —— 宁可多重建一次, 也不能漏掉生效。
//
// 与调用方约定: 必须在覆盖写 config.json **之前**调用, 否则读到的是新值。
func CommandBoxAppearanceFromConfigFile(configPath string) CommandBoxAppearance {
	data, err := os.ReadFile(configPath)
	if err != nil {
		return CommandBoxAppearance{}
	}
	var probe struct {
		Options struct {
			CommandFont      CommandFontOption `json:"commandFont"`
			CommandInputSkin CommandInputSkin  `json:"commandInputSkin"`
		} `json:"options"`
	}
	if err := json.Unmarshal(data, &probe); err != nil {
		return CommandBoxAppearance{}
	}
	return CommandBoxAppearance{Font: probe.Options.CommandFont, Skin: probe.Options.CommandInputSkin}
}

// samePath 判断两个路径是否指向同一个文件 (先做路径归一, 再比对 FileInfo 身份)。
func samePath(a, b string) (bool, error) {
	ai, err := os.Stat(a)
	if err != nil {
		return false, err
	}
	bi, err := os.Stat(b)
	if err != nil {
		return false, err // 目标不存在 ⇒ 必然不同
	}
	if filepath.Clean(a) == filepath.Clean(b) {
		return true, nil
	}
	return os.SameFile(ai, bi), nil
}

// classifyFontKinds 返回 (是否可用, 拒绝原因, IO 错误)。
//
// depth 用于 `ttcf` 集合的递归解包 (上限 1 层, 集合套集合无实际意义)。
func classifyFontKinds(path string, depth int) (bool, string, error) {
	f, err := os.Open(path)
	if err != nil {
		return false, "", err
	}
	defer f.Close()

	var head [4]byte
	if _, err := f.Read(head[:]); err != nil {
		return false, "", err
	}

	switch {
	case binary.BigEndian.Uint32(head[:]) == 0x00010000:
		return true, "", nil
	case string(head[:]) == "true":
		return true, "", nil
	case string(head[:]) == "OTTO":
		return false, "CFF/OpenType(OTTO) 轮廓不受支持, 命令框只接受 TrueType glyf " +
			"(可用 tools/font_otf2ttf.py 转换)", nil
	case string(head[:]) == "ttcf":
		if depth > 0 {
			return false, "字体集合嵌套过深, 无法判定轮廓格式", nil
		}
		// TTC 头: tag(4) + version(4) + numFonts(4) + offsetTable[numFonts](4 each)
		hdr := make([]byte, 12)
		if _, err := f.ReadAt(hdr, 0); err != nil {
			return false, "", err
		}
		numFonts := binary.BigEndian.Uint32(hdr[8:12])
		if numFonts == 0 {
			return false, "字体集合为空 (numFonts=0)", nil
		}
		if numFonts > fontCollectionFaceLimit {
			return false, "字体集合的 face 数超出上限, 文件疑似损坏", nil
		}
		var off [4]byte
		if _, err := f.ReadAt(off[:], 12); err != nil {
			return false, "", err
		}
		fi, err := f.Stat()
		if err != nil {
			return false, "", err
		}
		faceOff := int64(binary.BigEndian.Uint32(off[:]))
		// offset 必须落在文件内且**留得下** 4 字节 sfnt 签名 —— 与 C# 侧同口径。
		// 判据是 `off > size-4` (而非 `>=`): 恰好剩 4 字节时读取是合法的, 用 `>=` 会
		// 误拒 (C# 侧原先正是 `>=`, 2026-09-21 一并订正为 `>`)。缺此检查时越界 offset
		// 会以 I/O 错误冒泡, 被上层报成"读取失败"; 而正确语义是"格式不受支持"。
		if faceOff <= 0 || faceOff > fi.Size()-4 {
			return false, "字体集合的首 face 偏移越界, 文件疑似损坏", nil
		}
		var faceTag [4]byte
		if _, err := f.ReadAt(faceTag[:], faceOff); err != nil {
			return false, "", err
		}
		switch {
		case binary.BigEndian.Uint32(faceTag[:]) == 0x00010000, string(faceTag[:]) == "true":
			return true, "", nil
		case string(faceTag[:]) == "OTTO":
			return false, "字体集合的首个 face 是 CFF/OpenType(OTTO) 轮廓, 不受支持", nil
		}
		return false, "字体集合的 face 轮廓格式无法识别", nil
	}
	return false, "不是可识别的字体文件 (未知 sfnt 签名)", nil
}
