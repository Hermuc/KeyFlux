# 提案: 插件动作完全解耦 (P7) —— `provides.actions[]` 与动作 ID 字符串化

> 状态: **待评审**（QuickSwitch 插件化提案 §6 P7 期的独立载体; 依 D4 裁定「P7 另起提案,
> 不与前 6 期混批」。前置: P0-P6 已全部落地, 见 `quickswitch-pluginization.md` §11）。
> 关联契约: `docs/CONTRACTS.md` §3.4 ActionRegistry、§3.7 PluginManager、§4 插件清单格式
> （2026-10-02 订正版）、§5 生成端契约; §0 总原则 2（快路径红线）。
> 分支: **独立分支 + 独立评审**（协议变更 + 基线全量重录, 不与其他工作混批）。

---

## 0. 结论先说

1. **这是终态, 不是必需品。** P0-P6 已让 QuickSwitch 满足「可启用/禁用/删除且不拖垮引擎」;
   核心仍残留的两处 QuickSwitch 知识（`generator/{actions,plugins}.rs` 的 type9 薄壳与
   晚初始化特判、`BUILTIN_PLUGIN_IDS` 保留 ID 集）都是**机制性的**, 不是功能性的。
   P7 的收益 = 第三方插件可注册**一等动作**（出现在动作下拉里）, 而非"每插件自己处理"。
2. **代价是协议变更 + 基线全量重录**, 且配置文件格式前进不兼容（动作 ID 字符串化后,
   旧版本读不懂新配置）—— 这正是 D4 拒绝混批的理由。
3. 推荐路径仍按 Strangler Fig 拆两期: **P7a 协议扩容**（manifest 加 `provides.actions[]`,
   未被消费时零产物漂移）→ **P7b 消费切换**（动作下拉动态化 + 配置字符串化 + 删保留 ID 集）。

---

## 1. 现状残余耦合（P6 后的精确测绘）

| # | 位置 | 知识内容 | 性质 |
|---|---|---|---|
| K1 | `generator/actions.rs`（Go `generators/actions.go` 同构） | `valueID 9` → `PluginAction("quick_switch", "goto")` 调用串 + `callMap[9]` 文本 | type9"快速切换"子项写死 |
| K2 | `generator/plugins.rs` / `generators/plugins.go` | `if id == "quick_switch"` 晚初始化特判（渲染 `\nInitQuickSwitch()`） | 插件缺「晚初始化函数」声明机制 |
| K3 | `generator/plugins.rs` / `services/plugins.rs`（`BUILTIN_PLUGIN_IDS`） | 保留 ID 集 = `["quick_switch"]`（导入冒名拦截 + 墓碑判定） | 「内置」是名单而非属性 |
| K4 | i18n / golden | `callMap[9]` 文本与「快速切换」标签 | 语料覆盖点写死 |

P7 = 拆 K1/K2（协议）+ K3（机制泛化）。K4 随 K1 自动消解。

## 2. P7a 协议扩容: manifest `provides.actions[]`

```json
{
  "provides": {
    "actions": [
      { "id": "goto", "label": "快速切换", "labelEn": "Quick Switch",
        "kind": "plugin", "lateInit": "InitQuickSwitch" }
    ]
  }
}
```

- `actions[].id`: 插件内唯一; 全局动作 ID = `"<pluginId>.<actionId>"`（与
  `ActionRegistry.RegisterAction` 现有键空间一致, 零迁移）。
- `actions[].lateInit`（可选）: 声明该插件的晚初始化函数名 ⇒ 生成端 `PLUGIN_LATE_INIT`
  渲染 `"<lateInit>()"`（无参, 配置自取 —— P5 已定式）。**未声明 = 不产出, 空块零字节**
  ⇒ P7a 落地时 quick_switch 若不同步改 manifest, 产物逐字节不变（双轨闸门同 P2）。
- 校验: id 词表同 settings key; 全局键唯一性由目录扫描聚合后判重（重名 = 后包告警隔离）。
- 影响面: 两端 manifest 模型 + 校验 + wire 投影（omitempty, 未声明不出场）⇒
  **GET /api/plugins 输出对存量插件零漂移; GET /config 不变** ⇒ parity / api-parity 基线
  **不动**（quick_switch 在 P7a 内不同步改 manifest 的前提下）。

## 3. P7b 消费切换

| 步骤 | 内容 | 基线影响 |
|---|---|---|
| ① 动作下拉动态化 | type9 的 valueID 子项列表 = 内置 1-8 快路径（**红线不动**）+ 目录聚合 `provides.actions[]`; 生成端 `valueID 9` → `PluginAction("<pluginId>", "<actionId>")` | keyflux 产物变（type9 键位行）⇒ parity 重录 |
| ② 配置字符串化 | keymap 配置存 `actionId: "quick_switch.goto"`（新增字段）, 生成端解析; 数字 `typeID/valueID` 继续输出（旧配置读兼容） | config 模型加字段 ⇒ GET /config 变 ⇒ api-parity 重录 |
| ③ 删 `BUILTIN_PLUGIN_IDS` | 「内置」改为随包分发标记（manifest 或分发渠道属性）; 冒名拦截改「真源目录名单」动态生成 | 面板/后端逻辑, 无产物影响 |
| ④ K2 移除 | 晚初始化特判删除, 全走 `lateInit` 声明 | 产物不变（P7a 已对齐） |

**机械验收**: `grep -rn "quickswitch\|quick_switch" bin/lib/ config-server/internal/ config-ui-reactor/src/`
中「核心层」（排除插件自身/测试/文档）**零命中** —— 核心零知识的可 grep 证明。

## 4. 风险与回滚

| 风险 | 等级 | 对策 | 回滚 |
|---|---|---|---|
| 动作 ID 字符串化后旧版本读不懂新配置（前向不兼容） | 高 | **双字段过渡**: 旧 `typeID/valueID` 继续写、新增 `actionId` 只读优先; 删旧字段留到 compat 期结束 | revert（旧字段一直在写） |
| type9 下拉动态化破坏 plan/golden 字节等价 | 中 | P7b 单独分支; 重录前定向 diff 审计（对照 P2 先例） | revert + 重录回滚 |
| 第三方动作注入任意 AHK 函数名（lateInit） | 中 | `lateInit` 仅允许 `[A-Za-z_][A-Za-z0-9_]{0,63}`; 渲染为无参调用; 生成端存在性校验照旧（入口缺失即跳过） | 校验拒绝, 不入目录 |
| ActionRegistry 键冲突（两插件同名 actionId） | 低 | 全局键 = `<pluginId>.<actionId>`, 命名空间天然隔离; 目录级判重兜底 | 加载告警隔离 |

## 5. 验收标准（§9 风格）

1. P7a 落地后 parity 12 基线 + api-parity 23 步**零重录**（双轨证明）。
2. P7b 落地后: 动作下拉含第三方插件动作; 删插件 ⇒ 下拉项消失 + 存量键位降级为
   「该动作不可用」提示（P4 既有路径）。
3. 全量重录后 `make parity` / `make api-parity` / `make check` 全绿。
4. 第 3 节「机械验收」grep 零命中。

## 6. 待用户裁定项

1. **动作 ID 存储形态**: 双字段过渡（本提案）vs 只存字符串（更激进, 回滚丢配置）。
2. **内置快路径 1-8 是否同样走 provides**: 本提案建议**不**（红线约束 #6, 编译期原生
   注册动作保持直连）。
3. **`lateInit` 放 manifest 顶层还是 provides.actions[] 内**: 本提案放 actions[] 内
   （与动作同生命周期）, 备选顶层 `init.late`。
