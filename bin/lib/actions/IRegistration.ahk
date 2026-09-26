/**
 * IRegistration —— 声明型契约 (docs/CONTRACTS.md §3.3, 接口已冻结)。
 * 模块化重构阶段 3 新增。覆盖快路径: 重映射/模式结构必须声明为原生注册,
 * 由 AHK 钩子直接处理, 不得包装为回调 (性能红线, 约束 6)。
 */
class IRegistration {
  /**
   * 向指定 Keymap 声明绑定。
   * @param km Keymap 实例 (KeymapManager.ahk 的 Keymap 类)
   */
  Declare(km) {
  }

  ; 声明内容类别 (词表冻结): "hotkey-action" | "remap" | "submode" | "singlePress"
  ; 契约 §3.3 未定义读取方法, 类别由实现方自行持有; 如需纳入接口须经契约修订。
}
