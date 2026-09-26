/**
 * LegacyLoader —— 遗留代码载荷消费入口 (占位, 模块化重构阶段 3)。
 *
 * 职责 (见 docs/CONTRACTS.md §5):
 *   Go 生成端继续编译遗留代码载荷 ——
 *     - TypeID 8 的 ahkCode 字段 (generators/type8_builtin.go)
 *     - ahk-expression: / ahk: 前缀行
 *     - conditionType 5 的自定义 HotIf 表达式 (model.GroupToWinTile 相关)
 *   这些载荷目前由生成脚本直接内联执行 (现状, 零行为变更)。
 *   本文件的最终职责是: 将上述载荷统一收口为受控执行
 *   (经 ActionRegistry.Execute 的 try/catch 错误隔离, 约束 4),
 *   直至用户把它们迁移为插件 (约束 7 数据/代码分离)。
 *
 * 当前状态: 仅定义, 不改变任何运行路径。
 * 接入点 (均按用户裁定推迟, 见 CONTRACTS.md §3.7 阶段 5 注记):
 *   1. 生成端把 TypeID 8 载荷输出为本文件可消费的注册数据,
 *      取代内联 km.Map(_ => <ahkCode>)。
 *   2. 执行异常经 EventBus 广播 plugin_error (阶段 6 总线已就绪, 发布能力已具备;
 *      待本收口落地时接入)。
 * 在接入点落地前, 本文件不得被生成脚本或模板引用, 保证零行为变更。
 */
class LegacyLoader {
  ; 已登记的遗留载荷条数 (接入后由生成数据填充)
  static Count := 0
}
