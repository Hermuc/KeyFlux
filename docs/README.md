# KeyFlux 文档索引

> **唯一入口**。共 25 篇，按「你要干什么」组织，不按目录。
> 状态标记：**权威** 随代码同步维护 ｜ **决策** 已落地、只读留档 ｜ **就地** 位置由构建/门禁固定，不可移动

---

## 1. 改代码前必读

| 文档 | 状态 | 一句话 |
|---|---|---|
| [CONTRACTS.md](./CONTRACTS.md) | **权威** | 行为契约：§0 总原则、约束 #1–#7、各模块接口与消息族（0x401–0x40C）。改动前先查它有没有被冻结 |
| [pitfalls.md](./pitfalls.md) | **权威** | 踩过的坑与反直觉行为（AHK v2 语义、GDI 细节、竞态），改相关代码前扫一遍 |
| [CHANGELOG.md](./CHANGELOG.md) | 流水 | 逐条变更记录（174KB）。查「某天为什么改了这个」时用关键词搜，不要通读 |

## 2. 为什么这样设计（决策记录）

已落地，只读。用途：回答「当初为什么不那样做」。

| 文档 | 一句话 |
|---|---|
| [quickswitch-pluginization.md](./contracts-proposals/quickswitch-pluginization.md) | QuickSwitch 从内建模块改为可启停插件：P0–P7b 七期裁定与回退路径 |
| [plugin-action-decoupling.md](./contracts-proposals/plugin-action-decoupling.md) | 插件动作完全解耦（`provides.actions[]` + 动作 ID 字符串化） |
| [fuzzy-strategy.md](./contracts-proposals/fuzzy-strategy.md) | 缩写命令的编辑距离容错与候选提示 |
| [plan-rust-migration.md](./plan-rust-migration.md) | Go 后端 → Rust 的终版方案（2026-10-06 完成退役） |
| [prd-custom-match-types.md](./prd-custom-match-types.md) | 用户自定义匹配类型（文本特征 + 后缀分组），已落地 |

## 3. 设计细节

| 文档 | 一句话 |
|---|---|
| [design-ime-guard.md](./design-ime-guard.md) | 命令框的输入法守卫：何时锁 IME、何时放开 |
| [design-text-feature-registry.md](./design-text-feature-registry.md) | 内置文本特征注册表，「plain 排除集」为何这样派生 |

## 4. 各模块就地面文档

与代码同目录，改那个模块时顺手可见。**不要搬走**。

| 文档 | 覆盖内容 |
|---|---|
| [command-input/README.md](../command-input/README.md) | 命令框：构建、协议接线、渲染分层、部署注意 |
| [command-input/doc/reference/README.md](../command-input/doc/reference/README.md) | 参考实现收编说明（`EverythingQueryEdit.ahk` 禁改约束） |
| [config-ui-reactor/README.md](../config-ui-reactor/README.md) | 设置面板：分层、生成端、构建与部署双腿 |
| [tools/parity/README.md](../tools/parity/README.md) | 生成端差分对账闸门：语料、基线、重录方法 |
| [tools/api-parity/README.md](../tools/api-parity/README.md) | API 逐位对账：23 条路由的基线口径 |
| [vendor/README.md](../vendor/README.md) | 第三方随包内容「勿改」清单与理由 |
| [config-ui-reactor/vendor/windows-reactor/readme.md](../config-ui-reactor/vendor/windows-reactor/readme.md) | 上游 fork 的用途与同步方式 |
| [config-ui-reactor/vendor/windows-reactor/PATCHES.md](../config-ui-reactor/vendor/windows-reactor/PATCHES.md) | 对上游打的全部补丁清单（升级时逐条复查） |

## 5. 随包分发（镜像）

`plugins/examples/` 与 `data/plugins/` 必须逐字节一致（`make check-plugins-mirror` 强制），
且会随安装包分发到用户机器。**两份都不是冗余**。

| 文档 | 说明 |
|---|---|
| [plugins/examples/everything_search/README.md](../plugins/examples/everything_search/README.md) | Everything 搜索插件唯一开发文档（含通道选型、消息契约、踩坑） |
| [plugins/examples/quick_switch/README.md](../plugins/examples/quick_switch/README.md) | QuickSwitch 插件说明 |
| `data/plugins/*/README.md` | 上述两份的镜像副本，内容完全相同 |

## 6. 站点资源

| 文档 | 说明 |
|---|---|
| [site-assets/config_doc.md](../site-assets/config_doc.md) | 配置说明页正文，被 `i18n.json` 与面板 guide 页读取；**运行时消费，不可移动** |

## 7. 面向使用者

| 文档 | 说明 |
|---|---|
| [readme.md](../readme.md) | 中文主页：功能、快捷键、安装 |
| [readme.en.md](../readme.en.md) | 英文版 |

---

## 为什么有些文档不在 `docs/`

「统一到一起」不是把文件挪进来，而是**入口统一**。下面这些位置由构建或门禁决定，移动会直接破坏产物：

| 文档 | 移动后会发生什么 |
|---|---|
| `docs/design-ime-guard.md` 之外的 `config-ui-reactor/src/generator/template.rs:456` | 该行注释会被**生成进 `keyflux.ahk`**，改路径 ⇒ 生成物字节变化 ⇒ parity 4/4 基线漂移 |
| `vendor/**` | `check-vendor` 校验逐文件 hash，改动或移动即红 |
| `plugins/examples/*/README.md` | 与 `data/plugins/*` 镜像比对；且随安装包分发到用户机器 |
| `site-assets/config_doc.md` | 被 `i18n.json` / `guide.rs` 按路径读取 |
| 各模块 `README.md` | 就近维护；多处代码注释以相对路径引用 |

需要搬迁时，先跑 `make check` 与 `make parity` 确认没有连锁反应。

---

## 已清理（2026-10-30）

| 内容 | 理由 |
|---|---|
| `doc/poc-archive/`（22 篇 + 3 个 PoC 源码） | 迁移 2026-10-06 完成退役；进度流水已被 CONTRACTS/CHANGELOG 覆盖 |
| `doc/与原版的差异.md` | 停更于 2026-09-10 |
| `docs/baseline/BEHAVIOR_CHECKLIST.md` | 重构期人工核对清单，已完成使命 |
| `command-input/doc/REWRITE.md` | 作者自标 v1 口径，描述已失真 |

恢复：`git revert 568dc0f`，或 `git checkout c44ddb1 -- doc/poc-archive/`。
