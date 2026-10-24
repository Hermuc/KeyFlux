; ============================================================
; EverythingPrewarm —— 会话开始前的预热 (2026-10-20 首搜延迟批)
;
; 要解决的问题: 「按空格进入搜索模式后第一次搜索明显延迟」。
; 实测 (本机 Everything 1.5.0.1418 + es.exe 1.1.0.37, 2026-10-20):
;   * `es.exe -version` (纯启动, 无查询) = 121~135ms;
;   * `es -n 1000 -export-txt` 全量查询 = 126~136ms; `-n 50` 仍 128~142ms
;     ⇒ **查询本身只有 5~15ms, 90% 是进程启动的固定成本**;
;   * es.exe 首次执行再叠 +70~100ms (镜像冷 / 安全软件首次扫描);
;   * 这些成本原先全部在「触发键之后的第一拍」才付。
;
; 做法: 把「首查才付」的固定成本提前到**会话开始** (命令框刚显示、用户还没按触发键)
;   付掉 —— 复用**生产同一条**查询路径 (EverythingSearch.WarmUp → provider.Search),
;   只把结果丢弃: 不新增机制、不新增参数、不新增文件格式, 因此「预热成功」==
;   「首查路径已就绪」(es 落点已解析 / 进程镜像已热 / DB 加载竞态已过)。
;
; 绿色 (无副作用) 的三条硬纪律:
;   ① **不拉起进程**: Everything 未运行时不预热 —— 是否启动 Everything 是用户的决定,
;      "预热"不得代为执行 (那会在用户只是路过命令框时凭空多一个常驻进程);
;   ② **不落盘**: 不写日志、不留缓存文件; 失败只记在内存 (LastError), 静默;
;   ③ **可随时移除**: 删掉此类与两处 Kick() 调用即回到旧时序, 生产语义零变化 ——
;      本模块不持有任何被生产路径读取的状态。
;
; 可移植: 只用 AHK 内建 (SetTimer / 反射式类调用) + 插件自身分层, 不依赖引擎符号,
;   不依赖 Everything 版本特性 (不走 IPC、不加新参数), 换 1.4/1.5 或换机器行为一致。
; ============================================================

class EverythingPrewarm {
  ; 同一预热的复用窗口 (ms)。会话开始 → 按触发键的人类间隔通常 150~400ms;
  ; 而用户连续开关命令框 (一个会话接一个会话) 可能只隔几百毫秒 —— TTL 让后者
  ; 不再重复热身 (每次热身 = 一个 es.exe 进程, 不该被连点放大)。
  static TTL_MS := 30000

  ; 上次「安排预热」的时刻 (A_TickCount); 0 = 本进程内从未预热
  static LastTick := 0

  ; 最近一次预热异常 (仅内存留痕, 不写日志文件 —— 见文件头纪律②)
  static LastError := ""

  /**
   * 请求预热 (幂等 + 节流)。**只挂一拍定时器就返回**, 任何调用方都不会被阻塞 ——
   * 预热本体要 RunWait 一个 es.exe (实测 ≈130ms), 绝不能发生在调用方的同步路径上
   * (插件入口 = 引擎启动期; 会话开始 = 命令框显示前)。
   *
   * @returns {Boolean} 是否真的安排了预热 (false = TTL 内已预热过, 本次跳过)
   */
  static Kick() {
    if (EverythingPrewarm.LastTick != 0
        && (A_TickCount - EverythingPrewarm.LastTick) < EverythingPrewarm.TTL_MS)
      return false
    EverythingPrewarm.LastTick := A_TickCount
    ; ObjBindMethod 而不是裸方法引用: 静态方法经类名取值在 AHK v2 里语义微妙
    ; (会被当成调用), 本仓库既有写法一律 ObjBindMethod (见 Session 的 _SyncQuery)。
    SetTimer(ObjBindMethod(EverythingPrewarm, "_Run"), -1)
    return true
  }

  /** 定时器体: 任何异常都就地吞掉 —— 预热失败绝不能影响会话 (纪律②)。 */
  static _Run() {
    try EverythingPrewarm.Warm()
    catch as e
      EverythingPrewarm.LastError := e.Message
  }

  /**
   * 同步预热 (立即执行, 供 Kick / 探针使用)。
   * @returns {Boolean} 是否真的热身了 es.exe 通道 (false = 未运行 / 通道不可用 / 出错)
   */
  static Warm() {
    return EverythingSearch.WarmUp()
  }
}
