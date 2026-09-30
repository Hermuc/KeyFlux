package proc

import (
	"os"
	"path/filepath"
	"testing"
)

// TestStopProcessByName_MissingIsIdempotent 覆盖 `StopProcessByName` 的「进程不存在」分支。
//
// 该分支为**正常情形**而非异常: 命令框 (`KeyFlux-CommandInput.exe`) 是懒加载的,
// 用户可能从未唤起过它, 此时保存字体设置不应被当成失败 (否则会误导日志与调用方)。
// 实现依赖 `taskkill` 在找不到镜像时返回退出码 128, 此处即守住这一约定 ——
// 若某天 taskkill 的退出码语义变化, 本用例会先红。
func TestStopProcessByName_MissingIsIdempotent(t *testing.T) {
	// 故意用一个几乎不可能存在的镜像名, 避免误杀真实进程。
	const ghost = "KeyFlux-NoSuchProcessXYZ-9f3a.exe"
	if !StopProcessByName(ghost) {
		t.Fatal("结束不存在的进程应视作成功 (幂等), 实得失败")
	}
}

// TestRelayTarget_RequiresExistingFile 覆盖 relayTarget 的「目标存在才可中转」决策。
//
// 这是「保存设置后成批弹出『文档』资源管理器窗口」的守门人: 目标不存在时必须返回
// ok=false, 让 FallbackExecCmd 不调用 explorer.exe —— 否则 explorer 会把不存在的路径
// 当文件夹打开, 弹出默认目录「文档」(每调用一次弹一个窗口)。目标存在 → 可中转。
func TestRelayTarget_RequiresExistingFile(t *testing.T) {
	base := t.TempDir()
	target := filepath.Join(base, "KeyFlux.exe")
	if err := os.WriteFile(target, []byte("stub"), 0o644); err != nil {
		t.Fatalf("写目标失败: %v", err)
	}

	// 目标存在 → 可中转, 返回 clean 后的绝对路径 (与 Rust relay_target 同构)
	got, ok := relayTarget(base, "KeyFlux.exe")
	if !ok {
		t.Fatal("目标存在时应可中转 (ok=true), 实得 false")
	}
	if got != target {
		t.Fatalf("中转路径 = %q, 期望 %q", got, target)
	}

	// 目标缺失 → 不中转 (绝不调用 explorer)
	if _, ok := relayTarget(base, "Missing.exe"); ok {
		t.Fatal("目标缺失时不应中转 (ok=false), 实得 true")
	}

	// 同名目录不是可执行目标 → 同样不中转
	if err := os.Mkdir(filepath.Join(base, "adir"), 0o755); err != nil {
		t.Fatalf("建子目录失败: %v", err)
	}
	if _, ok := relayTarget(base, "adir"); ok {
		t.Fatal("目标是目录时不应中转 (ok=false), 实得 true")
	}
}
