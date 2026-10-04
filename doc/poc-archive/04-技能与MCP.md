# 技能与 MCP 配置说明（前置工作项 2、3）

## 一、技能（Skill）

### 已安装

| 技能 | 位置 | 用途 | 状态 |
|---|---|---|---|
| `fluent-design` | `C:\Users\An1\.codebuddy\skills\fluent-design\` | **WinUI 3 / Fluent Design 2 实战参考**：Mica/Acrylic 材质、WinUI 控件规格（Button 32px/圆角 4px）、排版层级、阴影 elevation、动画缓动、NavigationView 导航模式。用于重建设计系统。 | ✅ 已装（SkillHub `fluent-design`，1033 下载） |
| `windows-reactor` | `C:\Users\An1\.codebuddy\skills\windows-reactor\` | **自制**：固化已核实的 `windows-reactor` v0.100 API 模型（Component/Input/Message/create/update/view、`App::run_component`、builder 控件）与「只用核实过的 API」纪律。 | ✅ 已装（本地自建） |

### 现有可复用技能

| 技能 | 用途 | 迁移期角色 |
|---|---|---|
| `ahk-v2-probe-harness` | AHK 引擎侧探针 | **保留**（引擎不变） |
| `avalonia-headless-probe` | Avalonia headless 渲染探针 | **保留至 Phase 3**，用于与旧版做行为对照；迁移完成后退役 |
| `code-review-and-quality` | 代码审查 | 保留 |
| `code-simplification` | 代码简化 | 保留 |

### 未找到（不臆造）
- SkillHub / 本地市场**均无** `windows-rs` / `winui` / `rust-desktop` 专属技能（已多轮检索）。
- ⇒ 因此**自建** `windows-reactor` 技能。后续如发现更好的社区技能再替换。

## 二、MCP（Model Context Protocol）

### 目的
「先查后写」：WinUI 3 / Windows App SDK / Windows API / Reactor 的细节**必须查官方文档**再落代码，直接压制幻觉（本次任务的核心诉求）。

### 推荐配置（只读检索）

| 名称 | 端点 | 类型 | 鉴权 | 范围 |
|---|---|---|---|---|
| `microsoft-learn` | `https://learn.microsoft.com/api/mcp` | 远程 Streamable HTTP | 免鉴权 | 微软官方文档检索 |
| （可选）`context7` | 第三方 | HTTP | 需 API Key | 库文档（Reactor 覆盖可能不足）**待评估** |
| （可选）`github` | GitHub 官方 MCP | HTTP | 需 Token | 读 `windows-rs` 源码/示例 |

### 预期效果
- 提问「WinUI 3 的 SystemBackdrop / Mica 如何用」等能命中官方页，而非靠模型记忆。
- 减少「凭印象写 Reactor API」的风险。

### 当前状态（2026-09-28 更新：已配置于 ZCode）
- ✅ 官方端点已核实：`https://learn.microsoft.com/api/mcp`（远程 streamable HTTP、免鉴权；本轮 `initialize` 探针 HTTP 200 复核）。
- ✅ **已写入 ZCode 用户级配置 `C:\Users\An1\.zcode\cli\config.json` → `mcp.servers`**（注意：不是旧计划的 `.codebuddy\mcp.json` —— 现客户端是 ZCode；`"type": "http"` 写法与官方 `image-search` 插件清单一致，附 `timeoutMs: 90000`）：

```json
{
  "mcp": {
    "servers": {
      "microsoft-learn": {
        "type": "http",
        "url": "https://learn.microsoft.com/api/mcp",
        "timeoutMs": 90000
      }
    }
  }
}
```

- ⏳ 生效验证（需重启客户端 / 新会话，MCP 在会话启动时连接）：Settings → MCP 应显示 `microsoft-learn` 已连接；提问「WinUI 3 SystemBackdrop / Mica」确认命中官方文档来源。若未连接 → 用 `diagnosing-mcp` 排查。
- 沿革：2026-09 上旬曾计划写入 `.codebuddy\mcp.json` 被用户取消；本条为 ZCode 下的正式落位。

### 验证方法（写入后）
1. 重启客户端使 MCP 生效。
2. 触发一次文档检索（如「WinUI 3 SystemBackdrop」），确认返回官方来源。
3. 若客户端不识别 `type: http`，回退用 `type: streamableHttp`（见本地 `qcc-company/mcp.json` 实例）或 `transportType: streamable-http`（见 `tencent-yunzhi/mcp.json`）。
