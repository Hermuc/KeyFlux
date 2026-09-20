package script

import (
	"encoding/binary"
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
//  4. 源文件头不是字体签名   -> 跳过 (TrueType 0x00010000 / 'true' / 'OTTO' / 'ttcf')。
//  5. 源已就位于目标路径     -> 跳过 (避免自复制把文件截断为 0 字节)。
//  6. 目标目录不存在         -> 先建目录再复制 (老部署树可能没有 bin/font/)。
//
// baseDir 同时用于解析相对路径与推导落点:
//   - 运行时 (GenerateScripts) cwd = bin ⇒ 传 "" (即 "."), 落点 "font/font.ttf" 正好
//     落在 bin/font/font.ttf;
//   - CLI (GenerateAHK) 落点应跟随**输出文件**所在目录 (部署树的 bin/), 故传其目录,
//     相对 SourcePath 也一并以该目录为基准解析。
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

	// 5. 源已就位: 先于签名校验判定 —— 自复制会把目标截断为 0 字节。
	if same, err := samePath(src, dst); err == nil && same {
		return nil
	}

	// 4. 字体签名嗅探 (同时挡住复制自身这类边界)。
	ok, err := looksLikeFont(src)
	if err != nil {
		return fmt.Errorf("命令框字体源读取失败, 沿用现有字体: %s: %v", src, err)
	}
	if !ok {
		return fmt.Errorf("命令框字体源不是字体文件 (未知 sfnt 签名), 沿用现有字体: %s", src)
	}

	if err := os.MkdirAll(filepath.Dir(dst), 0o755); err != nil { // 6.
		return fmt.Errorf("命令框字体落点目录创建失败: %v", err)
	}

	data, err := os.ReadFile(src)
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

// looksLikeFont 读文件头 4 字节判定是否为可识别的字体容器。
// 接受: TrueType (0x00010000)、Apple TrueType ('true')、CFF/OTF ('OTTO')、
// 字体集合 ('ttcf')。这四种覆盖了系统文件弹窗里可能选到的全部字体格式。
func looksLikeFont(path string) (bool, error) {
	f, err := os.Open(path)
	if err != nil {
		return false, err
	}
	defer f.Close()

	var head [4]byte
	if _, err := f.Read(head[:]); err != nil {
		return false, err
	}
	switch {
	case binary.BigEndian.Uint32(head[:]) == 0x00010000:
		return true, nil
	case string(head[:]) == "true", string(head[:]) == "OTTO", string(head[:]) == "ttcf":
		return true, nil
	}
	return false, nil
}
