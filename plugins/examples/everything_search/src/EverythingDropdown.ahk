; ============================================================
; EverythingDropdown —— 结果下拉浮层 (渲染层)。
;
; 职责边界 (与 QuickSwitchUI.ahk 同款分层约定):
;   * 入参 = items:Array<{path,name,isFolder}> + index + 锚点;
;   * 唯一出参 = 回调 onPick(item);
;   * 不查 Everything、不读配置、不发键、不持有命令框句柄。
;
; 为什么自建浮层而不是「塞进命令框」: 命令框本体是上游预编译二进制
; (bin/KeyFlux-CommandInput.exe, 无源码), 只有 WM_CHAR 单向通道, 没有任何
; 「投递候选列表」接口 (实测其内部无 ListBox/ListView 资源)。故下拉列表只能由
; 引擎侧自绘, 锚定在命令框正下方 —— 视觉上仍是「command 下方的下拉列表」。
;
; 「不激活」实现: -Caption + WS_EX_NOACTIVATE(0x08000000) 且 Show("NA"),
; 显示后前台仍是用户原来的窗口, 命令框输入不受影响。
; 错误隔离: 全部 GUI 调用 try/catch, 失败静默 (最坏情况 = 没有下拉, 功能降级但不崩)。
; ============================================================
#Warn All, Off

global ED_GUI := 0
global ED_LV := 0
global ED_BUILT := false
global ED_ONPICK := 0
global ED_ROWS := 10            ; 可见行数上限 (不滚动, 与锚点高度共同决定)
global ED_ROW_H := 21
global ED_GAP := 6              ; 与命令框底边的贴合间距 (命令框窗口含阴影内边距)

class EverythingDropdown {
  /** 注入点选回调 (由会话层提供)。 */
  static SetCallback(onPick) {
    global ED_ONPICK
    ED_ONPICK := onPick
  }

  /** 首次创建 Gui + ListView; 之后复用同一窗口。 */
  static Ensure() {
    global ED_GUI, ED_LV, ED_BUILT
    if (ED_BUILT)
      return
    g := Gui("+AlwaysOnTop -Caption +ToolWindow +E0x08000000 -DPIScale", EverythingMessages.T("title"))
    g.MarginX := 0
    g.MarginY := 0
    try g.SetFont("s9", "Segoe UI")
    lv := g.Add("ListView", "x0 y0 w420 h200 -Multi -Hdr", ["", ""])
    try lv.OnEvent("Click", EverythingDropdown._OnClick)
    try lv.OnEvent("DoubleClick", EverythingDropdown._OnClick)
    ED_GUI := g
    ED_LV := lv
    ED_BUILT := true
  }

  /**
   * 显示结果列表。
   * @param items 结果数组
   * @param index 初始高亮行 (1 基)
   */
  static Show(items, index) {
    global ED_LV, ED_ROWS
    this.Ensure()
    rect := this._AnchorRect(items.Length)
    try ED_LV.Move(0, 0, rect.w, rect.h)
    try ED_LV.ModifyCol(1, 26)
    try ED_LV.ModifyCol(2, rect.w - 34)

    try ED_LV.Delete()
    n := 0
    for it in items {
      ED_LV.Add(, it.isFolder ? "📁" : "📄", it.path)
      n += 1
    }
    this.Select(index)
    this._ShowAt(rect)
  }

  /** 显示一行提示文本 (无结果 / 通道不可用 / 引导继续输入)。 */
  static ShowHint(text) {
    global ED_LV
    this.Ensure()
    rect := this._AnchorRect(1)
    try ED_LV.Move(0, 0, rect.w, rect.h)
    try ED_LV.ModifyCol(1, 26)
    try ED_LV.ModifyCol(2, rect.w - 34)
    try ED_LV.Delete()
    try ED_LV.Add(, "…", text)
    this._ShowAt(rect)
  }

  /** 高亮第 index 行 (越界则收敛到范围内)。 */
  static Select(index) {
    global ED_LV, ED_BUILT
    if (!ED_BUILT)
      return
    n := 0
    try n := ED_LV.GetCount()
    if (n < 1)
      return
    if (index < 1)
      index := 1
    if (index > n)
      index := n
    try ED_LV.Modify(index, "Select Focus")
  }

  /** 隐藏浮层 (不销毁, 供复用)。 */
  static Hide() {
    global ED_GUI, ED_BUILT
    if (!ED_BUILT)
      return
    try ED_GUI.Hide()
  }

  ; ---- 内部 ----

  /**
   * 锚点矩形: 命令框正下方, 宽度跟随命令框, 高度按行数收敛到屏幕工作区内。
   * 命令框几何取自 WinExist 实例 (其 X 由二进制内部计算, 仓库不可假定居中)。
   */
  static _AnchorRect(rows) {
    global ED_ROWS, ED_ROW_H, ED_GAP
    if (rows < 1)
      rows := 1
    if (rows > ED_ROWS)
      rows := ED_ROWS

    bx := 0, by := 0, bw := 700, bh := 0, found := false
    try {
      hwnd := WinExist("ahk_class MyKeymap_Command_Input ahk_exe KeyFlux-CommandInput.exe")
      if (hwnd) {
        WinGetPos(&bx, &by, &bw, &bh, hwnd)
        found := true
      }
    }
    if (!found) {
      ; 命令框窗口不可用 (刚启动/被隐藏): 居中兜底, 仍保持可见
      bx := (A_ScreenWidth - 700) // 2
      by := A_ScreenHeight // 3
      bw := 700
      bh := 0
    }

    inset := 10
    x := bx + inset
    w := bw - inset * 2
    if (w < 200)
      w := 200
    y := by + bh - ED_GAP
    h := rows * ED_ROW_H + 4

    ; 不越出屏幕工作区 (多显示器下取主屏工作区做保守收敛)
    if (y + h > A_ScreenHeight)
      h := A_ScreenHeight - y - 4
    if (h < ED_ROW_H)
      h := ED_ROW_H
    return {x: x, y: y, w: w, h: h}
  }

  static _ShowAt(rect) {
    global ED_GUI
    try ED_GUI.Show("NA x" rect.x " y" rect.y " w" rect.w " h" rect.h)
  }

  static _OnClick(lv, item, *) {
    global ED_ONPICK
    if (item < 1)
      return
    path := ""
    try path := lv.GetText(item, 2)
    if (path != "" && ED_ONPICK != 0)
      ED_ONPICK(path)
  }
}
