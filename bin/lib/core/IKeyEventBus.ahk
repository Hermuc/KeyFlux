; ============================================================
; IKeyEventBus —— 按键事件总线抽象(薄观察层)。
; ⚠️ 此接口后续可用 Rust FFI 实现:
;    Rust 守护进程接管键盘捕获与事件分发,通过本地 IPC / FFI 回调
;    向 AHK 层投递事件;AHK 侧订阅方代码无需任何改动即可完成引擎替换。
;    Rust 守护进程同时充当 L2 插件通道宿主(见 3.7)。
; 红线:总线仅承载慢事件,绝不参与快路径(重映射/发键/鼠标)决策链。
; ============================================================
/**
 * IKeyEventBus —— 接口声明 (docs/CONTRACTS.md §3.1, 接口已冻结)。
 * 模块化重构阶段 6 落地。实现: core/EventBus.ahk (进程内同步)。
 * 部署版 AHK 无接口语法, 以普通方法体表达 (与 IAction 同模式), 形状不变。
 */
class IKeyEventBus {
  ; eventType 词表(冻结):
  ;   "mode_enter" / "mode_exit"     模式进栈/出栈, eventData: {name}
  ;   "abbr_submit"                  缩写命令提交,  eventData: {source: "caps"|"semi", command, matched, fuzzy: bool}
  ;   "selection_action"             选中动作触发,  eventData: {behavior, name, selected} (方案 D 单键分发后不再有 schemeId/ruleIndex)
  ;   "plugin_loaded" / "plugin_error"  插件生命周期, eventData: {pluginId, message?}
  static EVENT_TYPES := ["mode_enter", "mode_exit", "abbr_submit", "selection_action", "plugin_loaded", "plugin_error"]

  Subscribe(eventType, callback) {
    return 0 ; 返回订阅 ID
  }

  Unsubscribe(subId) {
  }

  Publish(eventType, eventData) {
  }
}
