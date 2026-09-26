package proc

import "testing"

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
