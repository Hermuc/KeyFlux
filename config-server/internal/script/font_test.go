package script

import (
	"encoding/binary"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// 构造一个最小可识别的"字体"文件 (只需 4 字节 sfnt 签名, 嗅探器不解析表)。
func writeFakeFont(t *testing.T, path string, sig []byte, size int) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("建测试目录失败: %v", err)
	}
	if size < len(sig) {
		size = len(sig)
	}
	data := make([]byte, size)
	copy(data, sig)
	if err := os.WriteFile(path, data, 0o644); err != nil {
		t.Fatalf("写入测试字体失败: %v", err)
	}
}

// TestInstallCommandFont 覆盖安装器的全部边界:
// 未配置 / 源缺失 / 超限 / 非字体 / 正常复制 / 源即目标 (自复制) / 目标目录不存在。
func TestInstallCommandFont(t *testing.T) {
	srcDir := t.TempDir()
	sig := make([]byte, 4)
	binary.BigEndian.PutUint32(sig, 0x00010000) // 合法 TrueType 签名

	// 每个子测试有独立的 base, 故 readTarget 必须接收 base 参数 (不可闭包捕获外层变量)。
	readTarget := func(base string) ([]byte, bool) {
		b, err := os.ReadFile(filepath.Join(base, FontTargetRel))
		return b, err == nil
	}

	t.Run("未配置路径时不触碰目标", func(t *testing.T) {
		base := t.TempDir()
		writeFakeFont(t, filepath.Join(base, FontTargetRel), sig, 64) // 预置既有字体
		before, _ := os.ReadFile(filepath.Join(base, FontTargetRel))

		if err := InstallCommandFont(CommandFontOption{SourcePath: ""}, base); err != nil {
			t.Fatalf("空路径应静默跳过, 实际 err=%v", err)
		}
		after, err := os.ReadFile(filepath.Join(base, FontTargetRel))
		if err != nil || string(before) != string(after) {
			t.Fatalf("空路径不应改动既有字体")
		}
	})

	t.Run("源不存在时静默跳过且不建目标", func(t *testing.T) {
		base := t.TempDir()
		err := InstallCommandFont(CommandFontOption{SourcePath: filepath.Join(srcDir, "nope.ttf")}, base)
		if err == nil {
			t.Fatal("源缺失应返回说明性 error (供调试), 实际 nil")
		}
		if _, ok := readTarget(base); ok {
			t.Fatal("源缺失时不应产生目标文件")
		}
	})

	t.Run("超过体积上限时跳过", func(t *testing.T) {
		base := t.TempDir()
		big := filepath.Join(srcDir, "big.ttf")
		writeFakeFont(t, big, sig, FontMaxBytes+1)
		if err := InstallCommandFont(CommandFontOption{SourcePath: big}, base); err == nil {
			t.Fatal("超限应返回 error")
		}
		if _, ok := readTarget(base); ok {
			t.Fatal("超限时不应产生目标文件")
		}
	})

	t.Run("非字体文件时跳过", func(t *testing.T) {
		base := t.TempDir()
		notFont := filepath.Join(srcDir, "notfont.bin")
		writeFakeFont(t, notFont, []byte("PK\x03\x04"), 128) // zip 签名
		if err := InstallCommandFont(CommandFontOption{SourcePath: notFont}, base); err == nil {
			t.Fatal("非字体应返回 error")
		}
		if _, ok := readTarget(base); ok {
			t.Fatal("非字体时不应产生目标文件")
		}
	})

	t.Run("正常复制并自动建目录", func(t *testing.T) {
		base := t.TempDir() // 目标 bin/font/ 尚不存在
		src := filepath.Join(srcDir, "ok.ttf")
		writeFakeFont(t, src, sig, 4096)
		if err := InstallCommandFont(CommandFontOption{SourcePath: src}, base); err != nil {
			t.Fatalf("正常复制不应失败: %v", err)
		}
		got, ok := readTarget(base)
		if !ok {
			t.Fatal("目标文件应存在")
		}
		if len(got) != 4096 {
			t.Fatalf("目标内容长度 = %d, 期望 4096", len(got))
		}
	})

	t.Run("源即目标时不得自复制截断", func(t *testing.T) {
		base := t.TempDir()
		target := filepath.Join(base, FontTargetRel)
		if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
			t.Fatal(err)
		}
		writeFakeFont(t, target, sig, 8192) // 已就位 (模拟用户手工放置/上一轮复制结果)

		if err := InstallCommandFont(CommandFontOption{SourcePath: target}, base); err != nil {
			t.Fatalf("源即目标应静默跳过: %v", err)
		}
		got, err := os.ReadFile(target)
		if err != nil || len(got) != 8192 {
			t.Fatalf("自复制把文件截断了: len=%d err=%v", len(got), err)
		}
	})

	t.Run("相对源路径以 baseDir 为基准解析", func(t *testing.T) {
		base := t.TempDir()
		writeFakeFont(t, filepath.Join(base, "lib", "rel.ttf"), sig, 2048)
		if err := InstallCommandFont(CommandFontOption{SourcePath: filepath.Join("lib", "rel.ttf")}, base); err != nil {
			t.Fatalf("相对路径解析失败: %v", err)
		}
		if got, ok := readTarget(base); !ok || len(got) != 2048 {
			t.Fatalf("相对路径复制结果不符: len=%d ok=%v", len(got), ok)
		}
	})
}

// writeFakeTTC 构造一个最小字体集合 (ttcf) 文件: 头部 12 字节 + 偏移表,
// 首个 face 的表头写在 offsetTable[0] 指向的位置。
func writeFakeTTC(t *testing.T, path string, faceTag string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("建测试目录失败: %v", err)
	}
	const faceOff = 28 // 12 字节头 + 4 字节偏移表 + 留白, 保证不重叠
	data := make([]byte, faceOff+4)
	copy(data[0:4], "ttcf")
	binary.BigEndian.PutUint32(data[4:8], 0x00010000) // version 1.0
	binary.BigEndian.PutUint32(data[8:12], 1)         // numFonts
	binary.BigEndian.PutUint32(data[12:16], faceOff)  // offsetTable[0]
	copy(data[faceOff:faceOff+4], faceTag)
	if err := os.WriteFile(path, data, 0o644); err != nil {
		t.Fatalf("写入测试字体集合失败: %v", err)
	}
}

// TestInstallCommandFont_FormatGate 覆盖轮廓格式闸门:
// 命令框 exe 硬编码 TrueType face 类型, 故只接受 glyf, 必须拒绝 CFF(OTTO)。
// 回归背景 (2026-09-21): 原实现把 'OTTO' 当合法签名放行 ⇒ 用户选 .otf 后文件被原样
// 复制为 font.ttf ⇒ 下游加载失败且**静默** (用户只看到"选了没效果")。
func TestInstallCommandFont_FormatGate(t *testing.T) {
	// 与 TestInstallCommandFont 同款局部读取器 (该处是同函数内闭包, 不跨函数可见)。
	readTarget := func(base string) ([]byte, bool) {
		b, err := os.ReadFile(filepath.Join(base, FontTargetRel))
		if err != nil {
			return nil, false
		}
		return b, true
	}

	t.Run("CFF/OpenType(OTTO) 必须被拒绝且不落盘", func(t *testing.T) {
		base := t.TempDir()
		writeFakeFont(t, filepath.Join(base, "src.otf"), []byte("OTTO"), 4096)

		err := InstallCommandFont(CommandFontOption{SourcePath: filepath.Join(base, "src.otf")}, base)
		if err == nil {
			t.Fatal("CFF/OTTO 应被拒绝, 实得 nil")
		}
		if _, ok := readTarget(base); ok {
			t.Fatal("被拒绝的源不应产生目标文件")
		}
		if !strings.Contains(err.Error(), "OTTO") {
			t.Fatalf("错误信息应点明 OTTO 轮廓: %v", err)
		}
	})

	t.Run("字体集合首 face 为 glyf 时接受", func(t *testing.T) {
		base := t.TempDir()
		writeFakeTTC(t, filepath.Join(base, "src.ttc"), "\x00\x01\x00\x00")

		if err := InstallCommandFont(CommandFontOption{SourcePath: filepath.Join(base, "src.ttc")}, base); err != nil {
			t.Fatalf("glyf 集合应被接受: %v", err)
		}
		if _, ok := readTarget(base); !ok {
			t.Fatal("glyf 集合应产生目标文件")
		}
	})

	t.Run("字体集合首 face 为 CFF 时拒绝", func(t *testing.T) {
		base := t.TempDir()
		writeFakeTTC(t, filepath.Join(base, "src.ttc"), "OTTO")

		err := InstallCommandFont(CommandFontOption{SourcePath: filepath.Join(base, "src.ttc")}, base)
		if err == nil {
			t.Fatal("CFF face 的集合应被拒绝, 实得 nil")
		}
		if _, ok := readTarget(base); ok {
			t.Fatal("被拒绝的集合不应产生目标文件")
		}
	})

	t.Run("Apple TrueType('true') 接受", func(t *testing.T) {
		base := t.TempDir()
		writeFakeFont(t, filepath.Join(base, "src.ttf"), []byte("true"), 4096)

		if err := InstallCommandFont(CommandFontOption{SourcePath: filepath.Join(base, "src.ttf")}, base); err != nil {
			t.Fatalf("'true' 签名应被接受: %v", err)
		}
		if _, ok := readTarget(base); !ok {
			t.Fatal("'true' 签名应产生目标文件")
		}
	})
}
