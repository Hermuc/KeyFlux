//! `app::handlers` —— `Shell::update` 的**按域消息处理臂**（2026-10-07 自 app.rs 拆出）。
//!
//! 拆分动机（见 docs 里的代码审查报告 #1）: 迁移前 `update` 是**单个 1334 行 / 108 臂**
//! 的总分发函数, 改任一业务都要在同一个函数里定位, 且视图层无测试网。现在:
//!
//! * `update` 只做**路由**（仍 match 全部 108 变体 ⇒ 穷尽性由 rustc 保证）;
//! * 业务臂按域落到本目录 9 个文件, 每个变体仍是独立 match 臂, **臂体逐字未改**
//!   （迁移由脚本执行, 并以「臂集合 + 逐臂 SHA256」双重对账, 失败即中止不写盘）;
//! * 加新消息 = 在对应域文件加臂 + 在 `update` 加一行转发, 两处即可。
//!
//! 域划分（`Message` 变体前缀 ⇒ 文件）:
//! | 文件 | 域 | 臂数 |
//! | --- | --- | --- |
//! | `keymap.rs` | 键位网格 / 导航 / 全局开关 / 保存流水线 / 后端生命周期 | 24 |
//! | `sa.rs` | SelectedAction 规则编辑（选中文本动作） | 24 |
//! | `bh.rs` | 动作库（behavior）编辑 | 18 |
//! | `mt.rs` | 文本特征 / 匹配类型注册表编辑 | 17 |
//! | `plugin.rs` | 插件卡：开关 / 配置 / 导入 / 删除 | 6 |
//! | `ps.rs` | 单插件设置对话框 | 5 |
//! | `misc.rs` | 提示条 / 自定义热键 / 字体浏览 | 5 |
//! | `guide.rs` | 使用指南就地编辑 | 5 |
//! | `market.rs` | 插件市场对话框 | 4 |

pub(super) mod bh;
pub(super) mod guide;
pub(super) mod keymap;
pub(super) mod market;
pub(super) mod misc;
pub(super) mod mt;
pub(super) mod plugin;
pub(super) mod ps;
pub(super) mod sa;
