fn main() {
    // 自包含部署：把 pinned Windows App Runtime 落到 Cargo profile 输出目录，并嵌入
    // self-contained 标记清单。首次构建需要 NuGet 网络（缓存在 %LOCALAPPDATA%，
    // 本机通过 env.ps1 重定向到 D 盘以遵守「C 盘只读」）。
    windows_reactor_setup::as_self_contained();
}
