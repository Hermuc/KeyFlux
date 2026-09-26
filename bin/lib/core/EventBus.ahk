/**
 * EventBus —— IKeyEventBus 的进程内同步实现 (docs/CONTRACTS.md §3.1)。
 * 模块化重构阶段 6 落地。
 *
 * 实现约定 (契约 §3.1):
 *  - Publish 对每个订阅回调独立 try/catch, 单个回调异常不影响其他订阅者 (约束 4);
 *  - 总线自身任何异常都不得向调用点抛出 (观察层不能反过来破坏运行);
 *  - 红线: 仅承载慢事件, 绝不参与快路径 (重映射/发键/鼠标) 决策链。
 *
 * 零订阅者时 Publish 为一次空 Map 遍历, 开销可忽略。
 * 未来引擎替换 (Rust 守护进程) 时只需换掉本实现, 订阅方代码不动。
 */
class EventBus {
  static Subs := Map()      ; subId -> Map{eventType, callback}
  static NextId := 1

  /**
   * 订阅事件。
   * @param eventType IKeyEventBus.EVENT_TYPES 词表内的类型; 未知类型拒绝 (返回 0)
   * @param callback Funcref, 签名 (eventData)
   * @return 订阅 ID (>0); 拒绝时返回 0
   */
  static Subscribe(eventType, callback) {
    if (!this._validType(eventType) || !IsObject(callback)) {
      this._log("Subscribe rejected: invalid eventType or callback")
      return 0
    }
    id := this.NextId
    this.NextId += 1
    this.Subs[id] := Map("eventType", eventType, "callback", callback)
    return id
  }

  /**
   * 取消订阅。未知订阅 ID 静默忽略。
   */
  static Unsubscribe(subId) {
    if (this.Subs.Has(subId))
      this.Subs.Delete(subId)
  }

  /**
   * 发布事件 (同步)。每个回调独立隔离; 总线自身异常也不向调用点抛出。
   * @param eventType 事件类型 (词表外直接忽略)
   * @param eventData 事件数据 (对象)
   */
  static Publish(eventType, eventData) {
    try {
      if (!this._validType(eventType))
        return
      for id, sub in this.Subs {
        if (sub["eventType"] == eventType) {
          try {
            sub["callback"].Call(eventData)
          } catch as e {
            this._log("Subscriber error on '" eventType "': " e.Message)
          }
        }
      }
    } catch {
      ; 观察层静默兜底
    }
  }

  static _validType(eventType) {
    for t in IKeyEventBus.EVENT_TYPES
      if (t == eventType)
        return true
    return false
  }

  static _log(msg) {
    try {
      FileAppend(FormatTime(, "yyyy-MM-dd HH:mm:ss") " " msg "`n", "logs\event_bus.log")
    }
  }
}
