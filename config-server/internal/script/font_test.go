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

// TestCommandBoxAppearanceFromConfigFile 覆盖「读取旧配置的命令框外观两段 (字体+皮肤)」
// 的容错口径 —— 该函数供保存处理器判定「外观是否真的变了」(决定是否结束命令框进程让
// 新值生效), 故任何异常都必须安全退化为零值 (零值 ≠ 用户的实际选择 ⇒ 走保守分支,
// 不会漏生效)。
func TestCommandBoxAppearanceFromConfigFile(t *testing.T) {
	write := func(t *testing.T, content string) string {
		t.Helper()
		p := filepath.Join(t.TempDir(), "config.json")
		if err := os.WriteFile(p, []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
		return p
	}

	t.Run("正常读出字体与皮肤两段", func(t *testing.T) {
		p := write(t, `{"options":{`+
			`"commandFont":{"sourcePath":"D:\\fonts\\A.ttf","weight":"bold"},`+
			`"commandInputSkin":{"backgroundColor":"#123456","windowWidth":"480"}}}`)
		got := CommandBoxAppearanceFromConfigFile(p)
		if got.Font != (CommandFontOption{SourcePath: `D:\fonts\A.ttf`, Weight: "bold"}) {
			t.Fatalf("字体段不符: %+v", got.Font)
		}
		if got.Skin.BackgroundColor != "#123456" || got.Skin.WindowWidth != "480" {
			t.Fatalf("皮肤段不符: %+v", got.Skin)
		}
	})

	t.Run("皮肤改动可被检出", func(t *testing.T) {
		// 这条是「皮肤也保存即生效」的核心依据: 若两快照相等, 皮肤改动将永远
		// 不会触发结束命令框进程 ⇒ 用户改皮肤永远看不到效果。
		old := CommandBoxAppearanceFromConfigFile(
			write(t, `{"options":{"commandInputSkin":{"borderRadius":"10"}}}`))
		neu := CommandBoxAppearanceFromConfigFile(
			write(t, `{"options":{"commandInputSkin":{"borderRadius":"20"}}}`))
		if old.Skin == neu.Skin {
			t.Fatal("皮肤变化后两快照不应相等")
		}
		if old.Font != neu.Font {
			t.Fatal("仅皮肤变化时字体段应保持不变")
		}
	})

	t.Run("文件不存在时退化为零值", func(t *testing.T) {
		got := CommandBoxAppearanceFromConfigFile(filepath.Join(t.TempDir(), "nope.json"))
		if got != (CommandBoxAppearance{}) {
			t.Fatalf("应为零值, 实得 %+v", got)
		}
	})

	t.Run("JSON 非法时退化为零值", func(t *testing.T) {
		if got := CommandBoxAppearanceFromConfigFile(write(t, "{ not json")); got != (CommandBoxAppearance{}) {
			t.Fatalf("应为零值, 实得 %+v", got)
		}
	})

	t.Run("缺 options 段时退化为零值", func(t *testing.T) {
		if got := CommandBoxAppearanceFromConfigFile(write(t, `{"behaviors":[]}`)); got != (CommandBoxAppearance{}) {
			t.Fatalf("应为零值, 实得 %+v", got)
		}
	})

	t.Run("零值与用户选择必然不等 (保守分支的依据)", func(t *testing.T) {
		got := CommandBoxAppearanceFromConfigFile(write(t, `{"options":{}}`))
		if got.Font == (CommandFontOption{SourcePath: `D:\fonts\A.ttf`}) {
			t.Fatal("零值不应等于用户的非空字体选择")
		}
		if got.Skin == (CommandInputSkin{WindowWidth: "700"}) {
			t.Fatal("零值不应等于用户改过的皮肤")
		}
	})
}

// TestNormalizeFontWeight 覆盖字重档位规范化: 已知档位直通, 未知/空值回落 regular。
//
// 回落口径必须是**中性档**而非更粗的档位 —— 猜粗会在部分字体上把 CJK 字腔填死
// (不可逆), 而 regular 只是"不膨胀", 永远安全。
func TestNormalizeFontWeight(t *testing.T) {
	for _, w := range []string{"thin", "light", "regular", "semibold", "bold"} {
		if got := NormalizeFontWeight(w); got != w {
			t.Fatalf("已知档位应直通: %q -> %q", w, got)
		}
	}
	// 未知值 / 空串 / 大小写不符 / 历史遗留值一律回落 regular
	// ⚠ "medium" 是已移除的旧档名, 必须回落而不是直通 (否则会去找不存在的变体)
	for _, bad := range []string{"", "Bold", "BOLD", "medium", "heavy", "black", "700", "超粗"} {
		if got := NormalizeFontWeight(bad); got != "regular" {
			t.Fatalf("未知档位 %q 应回落 regular, 实得 %q", bad, got)
		}
	}
}

// TestVariantPath 覆盖变体路径推导: 命名约定, 去扩展名, regular 无变体。
func TestVariantPath(t *testing.T) {
	cases := []struct {
		src, weight, want string
	}{
		{`D:\f\A.ttf`, "regular", ""},                               // regular 无变体
		{`D:\f\A.ttf`, "thin", `D:\f\A.thin.ttf`},                   // 腐蚀细档
		{`D:\f\A.ttf`, "light", `D:\f\A.light.ttf`},                 //
		{`D:\f\A.ttf`, "semibold", `D:\f\A.semibold.ttf`},           //
		{`D:\f\A.ttf`, "bold", `D:\f\A.bold.ttf`},                   //
		{`D:\f\A.ttf`, "bogus", ""},                                 // 未知档位 -> regular -> 无变体
		{`D:\f\A.ttf`, "medium", ""},                                // 旧档名 -> regular -> 无变体
		{`/home/u/My Font.ttf`, "bold", `/home/u/My Font.bold.ttf`}, // 含空格
		{`D:\f\noext`, "bold", `D:\f\noext.bold`},                   // 无扩展名
	}
	for _, c := range cases {
		if got := VariantPath(c.src, c.weight); got != c.want {
			t.Fatalf("VariantPath(%q,%q) = %q, 期望 %q", c.src, c.weight, got, c.want)
		}
	}
}

// TestInstallCommandFont_WeightVariant 覆盖字重档位对实际落地字体的选择:
// 有变体用变体 / 变体缺失回落源字体 / regular 用源字体本体。
func TestInstallCommandFont_WeightVariant(t *testing.T) {
	sig := make([]byte, 4)
	binary.BigEndian.PutUint32(sig, 0x00010000)

	readTarget := func(base string) []byte {
		b, err := os.ReadFile(filepath.Join(base, FontTargetRel))
		if err != nil {
			t.Fatalf("读取落点失败: %v", err)
		}
		return b
	}
	// 造一份内容可区分的"字体": 首 4 字节合法签名 + 填充字节标识来源
	mk := func(t *testing.T, path string, tag byte) {
		t.Helper()
		writeFakeFont(t, path, sig, 64)
		b, err := os.ReadFile(path)
		if err != nil {
			t.Fatal(err)
		}
		for i := 4; i < len(b); i++ {
			b[i] = tag
		}
		if err := os.WriteFile(path, b, 0o644); err != nil {
			t.Fatal(err)
		}
	}

	t.Run("有预烘焙变体时用变体", func(t *testing.T) {
		base := t.TempDir()
		src := filepath.Join(base, "A.ttf")
		mk(t, src, 0xAA)                               // 源字体
		mk(t, filepath.Join(base, "A.bold.ttf"), 0xBB) // 预烘焙的粗体变体

		if err := InstallCommandFont(CommandFontOption{SourcePath: src, Weight: "bold"}, base); err != nil {
			t.Fatalf("应成功, err=%v", err)
		}
		got := readTarget(base)
		if got[4] != 0xBB {
			t.Fatalf("应落地粗体变体 (tag 0xBB), 实得 0x%02X", got[4])
		}
	})

	t.Run("变体缺失时回落源字体本体", func(t *testing.T) {
		base := t.TempDir()
		src := filepath.Join(base, "A.ttf")
		mk(t, src, 0xAA) // 只有源字体, 没有 A.bold.ttf

		if err := InstallCommandFont(CommandFontOption{SourcePath: src, Weight: "bold"}, base); err != nil {
			t.Fatalf("应回落成功, err=%v", err)
		}
		got := readTarget(base)
		if got[4] != 0xAA {
			t.Fatalf("变体缺失应回落源字体 (tag 0xAA), 实得 0x%02X", got[4])
		}
	})

	t.Run("regular 档用源字体本体", func(t *testing.T) {
		base := t.TempDir()
		src := filepath.Join(base, "A.ttf")
		mk(t, src, 0xAA)
		mk(t, filepath.Join(base, "A.semibold.ttf"), 0xCC) // 存在其它档变体, 但 regular 不该用它

		if err := InstallCommandFont(CommandFontOption{SourcePath: src, Weight: "regular"}, base); err != nil {
			t.Fatalf("应成功, err=%v", err)
		}
		got := readTarget(base)
		if got[4] != 0xAA {
			t.Fatalf("regular 应落地源字体 (tag 0xAA), 实得 0x%02X", got[4])
		}
	})

	t.Run("腐蚀细档 thin/light 也走变体选择", func(t *testing.T) {
		base := t.TempDir()
		src := filepath.Join(base, "A.ttf")
		mk(t, src, 0xAA)
		mk(t, filepath.Join(base, "A.thin.ttf"), 0xDD)  // 腐蚀产出的极细变体
		mk(t, filepath.Join(base, "A.light.ttf"), 0xEE) // 腐蚀产出的细变体

		if err := InstallCommandFont(CommandFontOption{SourcePath: src, Weight: "thin"}, base); err != nil {
			t.Fatalf("thin 应成功, err=%v", err)
		}
		if got := readTarget(base); got[4] != 0xDD {
			t.Fatalf("thin 应落地 A.thin.ttf (tag 0xDD), 实得 0x%02X", got[4])
		}

		if err := InstallCommandFont(CommandFontOption{SourcePath: src, Weight: "light"}, base); err != nil {
			t.Fatalf("light 应成功, err=%v", err)
		}
		if got := readTarget(base); got[4] != 0xEE {
			t.Fatalf("light 应落地 A.light.ttf (tag 0xEE), 实得 0x%02X", got[4])
		}
	})

	t.Run("未知字重回落 regular (源字体)", func(t *testing.T) {
		base := t.TempDir()
		src := filepath.Join(base, "A.ttf")
		mk(t, src, 0xAA)
		mk(t, filepath.Join(base, "A.bold.ttf"), 0xBB)

		if err := InstallCommandFont(CommandFontOption{SourcePath: src, Weight: "超粗"}, base); err != nil {
			t.Fatalf("应成功, err=%v", err)
		}
		got := readTarget(base)
		if got[4] != 0xAA {
			t.Fatalf("未知档位应回落 regular/源字体 (tag 0xAA), 实得 0x%02X", got[4])
		}
	})

	t.Run("变体不是有效字体时回落源字体", func(t *testing.T) {
		base := t.TempDir()
		src := filepath.Join(base, "A.ttf")
		mk(t, src, 0xAA)
		// 变体存在但是垃圾内容 (无 sfnt 签名) —— 极易出现在用户手工替换文件后
		if err := os.WriteFile(filepath.Join(base, "A.bold.ttf"),
			[]byte("not a font at all"), 0o644); err != nil {
			t.Fatal(err)
		}

		if err := InstallCommandFont(CommandFontOption{SourcePath: src, Weight: "bold"}, base); err != nil {
			t.Fatalf("应成功, err=%v", err)
		}
		got := readTarget(base)
		if got[4] != 0xAA {
			t.Fatalf("变体非法时应回落源字体 (tag 0xAA), 实得 0x%02X", got[4])
		}
	})
}

// TestInstallCommandFont_CollectionGuards 覆盖 .ttc 集合的两条"防损坏"判据。
//
// 为什么必须有: 这两条判据在 C# 侧 CommandFontValidator 里也有一份, 两边**必须同口径** ——
// UI 说"可用"而生成端拒绝 (或反之) 会给出自相矛盾的反馈, 是最难排查的一类问题。
// 回归背景 (2026-09-21): Go 侧原先只拒绝 numFonts==0, 且越界 offset 依赖 ReadAt 报错
// 冒泡 (会被上层报成"读取失败"而非"格式不受支持"); C# 侧的 offset 判据又比必要值严
// 1 字节。现两侧统一为: numFonts ∈ [1, 4096], offset ∈ (0, size-4]。
func TestInstallCommandFont_CollectionGuards(t *testing.T) {
	// 局部构造器: 声明偏移与实际文件尺寸**解耦** —— 既有 writeFakeTTC 把两者绑死,
	// 造不出"声明越界"的样本。
	write := func(t *testing.T, path string, numFonts uint32, declaredOff int) {
		t.Helper()
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			t.Fatalf("建测试目录失败: %v", err)
		}
		const realFaceOff = 28 // 12 字节头 + 4 字节偏移表 + 留白 (共 32 字节文件)
		data := make([]byte, realFaceOff+4)
		copy(data[0:4], "ttcf")
		binary.BigEndian.PutUint32(data[4:8], 0x00010000) // version 1.0
		binary.BigEndian.PutUint32(data[8:12], numFonts)
		binary.BigEndian.PutUint32(data[12:16], uint32(declaredOff)) // 只改"声明", 不动文件尺寸
		copy(data[realFaceOff:realFaceOff+4], "\x00\x01\x00\x00")
		if err := os.WriteFile(path, data, 0o644); err != nil {
			t.Fatalf("写入测试字体集合失败: %v", err)
		}
	}

	target := func(base string) string { return filepath.Join(base, FontTargetRel) }

	reject := []struct {
		name     string
		numFonts uint32
		off      int
		wantMsg  string
	}{
		{"face 数超上限 (4097) 拒绝", 4097, 28, "face 数"},
		{"声明偏移远超文件末尾拒绝", 1, 1 << 20, "越界"},
		{"声明偏移为 0 拒绝", 1, 0, "越界"},
	}
	for _, c := range reject {
		t.Run(c.name, func(t *testing.T) {
			base := t.TempDir()
			src := filepath.Join(base, "src.ttc")
			write(t, src, c.numFonts, c.off)

			err := InstallCommandFont(CommandFontOption{SourcePath: src}, base)
			if err == nil {
				t.Fatal("应被拒绝, 实得 nil")
			}
			if !strings.Contains(err.Error(), c.wantMsg) {
				t.Fatalf("错误信息应含 %q: %v", c.wantMsg, err)
			}
			if _, err := os.Stat(target(base)); err == nil {
				t.Fatal("被拒绝的集合不应产生目标文件")
			}
		})
	}

	accept := []struct {
		name     string
		numFonts uint32
		off      int
	}{
		{"face 数上限边界 (4096) 接受", 4096, 28},
		{"偏移恰好留 4 字节接受 (判据用 > 而非 >=)", 1, 28},
	}
	for _, c := range accept {
		t.Run(c.name, func(t *testing.T) {
			base := t.TempDir()
			src := filepath.Join(base, "src.ttc")
			write(t, src, c.numFonts, c.off)

			if err := InstallCommandFont(CommandFontOption{SourcePath: src}, base); err != nil {
				t.Fatalf("应被接受, err=%v", err)
			}
			if _, err := os.Stat(target(base)); err != nil {
				t.Fatalf("应产生目标文件: %v", err)
			}
		})
	}
}
