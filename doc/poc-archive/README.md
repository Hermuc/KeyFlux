# KeyFlux 设置面板迁移：Avalonia → Rust + windows-reactor（WinUI 3）

> 本目录 = 迁移前置工作与方案的**交付文档**（D 盘工作区，非仓库内）。
> 决策已确认：**Rust + windows-reactor**（全面迁移；范围含测试与 CI/部署）。

## 文档索引

| 文件 | 内容 |
|---|---|
| [01-项目规范.md](./01-项目规范.md) | 代码风格、目录结构、命名约定、版本管理策略、契约衔接 |
| [02-迁移总方案.md](./02-迁移总方案.md) | 总体策略、关键改造点、依赖调整、风险登记、回退 |
| [03-分步实施计划.md](./03-分步实施计划.md) | Phase 0~6 可执行步骤与验收门禁 |
| [04-技能与MCP.md](./04-技能与MCP.md) | 已装技能清单/用途、MCP 目的·范围·预期 |
| [scaffold/](./scaffold/) | 规范的可执行落点（rustfmt/clippy/toolchain/editorconfig/Cargo.toml） |

## 已完成的前置工作

- ✅ 项目规范文档 + 可执行配置文件
- ✅ 技能 `fluent-design`（SkillHub 安装）
- ✅ 技能 `windows-reactor`（自建，固化已核实 API）
- ⏸ MCP `microsoft-learn` 配置（**写入被取消，待确认**）

## 关键事实（已核实，非推测）

| 项 | 值 |
|---|---|
| 现有 UI 栈 | .NET 10 + Avalonia 11.3.20 + CommunityToolkit.Mvvm 8.4.2 |
| `windows-reactor` | v0.100.0，声明式 WinUI 3 库，2026-04 首发，约 1.2k 下载（极早期） |
| 本机 Rust | **未安装**（`cargo`/`rustc` 不存在） |
| 迁移边界 | Go 后端 / AHK 引擎 / `config.json` / HTTP 协议 **零改动** |

## 下一步

建议先执行 **Phase 1（安装 Rust + 最小 PoC）**，把方案中所有「待核实」项验证掉，再进入逐页迁移。
