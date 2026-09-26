# 托盘图标 UIA 直连 v10（v9 优化版：轮询替代固定等待 + C# 延迟编译，唤起提速）
# v8 修复：keybd_event 注入 Win+B 非原子导致开始菜单弹出、面板无法展开（用户实测 PANEL NOT FOUND）
#   —— 展开面板改纯 UIA：chevron（Name=显示隐藏的图标）直接 Invoke（PoC 实证）
# v9 新增：图标可能在托盘可见区（不在溢出面板，如 QQ Electron 关窗后 uid=3 在可见区，UIA 树不可见）
#   —— 面板匹配失败后兜底：Shell_NotifyIconGetRect 即时定位图标坐标 → 单击 → 验证窗口 → 双击 → 验证
# v10 优化（2026-08-21 基线实测：场景 A 约 3.0s = 冷启动 ~970ms + 固定等待 1700ms + 其余 ~330ms）：
#   —— 所有固定等待改 50ms 轮询（面板展开/窗口可见，窗口一出现立即返回，实测省 ~900ms）
#   —— TrayNav C# 类延迟编译（面板路径不需要，省 ~600ms 编译；仅可见区兜底首次编译）
#   —— Get-PanelWindow 改 FindFirst+ClassName 条件、Get-ChevronButton 改 ControlType 条件（减少 UIA 全树枚举）
# 行为不变：激活后绝不发 Esc、绝不键盘模拟组合键、面板残留由下次运行复用、日志结构不变
# 用法: powershell -File tray_nav.ps1 -Target 微信 -Process Weixin.exe -LogFile mk_traynav.txt
param([string]$Target = '', [string]$Process = '', [string]$LogFile = 'mk_traynav.txt')
$ErrorActionPreference = 'SilentlyContinue'
$log = Join-Path $env:TEMP $LogFile
function Log([string]$msg) {
  try { "$(Get-Date -Format 'HH:mm:ss.fff') $msg" | Out-File $log -Append -Encoding utf8 } catch {}
}
Log "=== run target=[$Target] process=[$Process] ==="

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

function Wait-Until([scriptblock]$cond, [int]$timeoutMs, [int]$intervalMs = 50) {
  # 轮询等待条件成立（窗口/面板出现即返回，替代固定 Sleep 缩短延迟）
  $sw = [System.Diagnostics.Stopwatch]::StartNew()
  while ($sw.ElapsedMilliseconds -lt $timeoutMs) {
    try { if (& $cond) { return $true } } catch {}
    Start-Sleep -Milliseconds $intervalMs
  }
  try { if (& $cond) { return $true } } catch {}
  return $false
}

function Get-PanelWindow {
  # 溢出面板窗口是独立顶层窗口（TopLevelWindowForOverflowXamlIsland），FindFirst 条件直查（比全枚举快）
  try {
    $root = [System.Windows.Automation.AutomationElement]::RootElement
    $cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ClassNameProperty, 'TopLevelWindowForOverflowXamlIsland')
    return $root.FindFirst([System.Windows.Automation.TreeScope]::Children, $cond)
  } catch {}
  return $null
}

function Get-ChevronButton {
  # 任务栏"显示隐藏的图标"chevron 按钮：Shell_TrayWnd 子树按 ControlType.Button 条件过滤查找
  try {
    $root = [System.Windows.Automation.AutomationElement]::RootElement
    $tcond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ClassNameProperty, 'Shell_TrayWnd')
    $tray = $root.FindFirst([System.Windows.Automation.TreeScope]::Children, $tcond)
    if (-not $tray) { return $null }
    $bcond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button)
    $all = $tray.FindAll([System.Windows.Automation.TreeScope]::Descendants, $bcond)
    foreach ($el in $all) {
      try {
        $n = ([string]$el.Current.Name).Trim()
        if ($n -eq '显示隐藏的图标') { return $el }
      } catch {}
    }
  } catch {}
  return $null
}

function Expand-Panel {
  # 面板已开直接复用；未开则 Invoke chevron 展开（轮询等待面板出现，重试一次防抖）
  $panel = Get-PanelWindow
  if ($panel) { Log 'panel already open (reuse)'; return $panel }
  $chev = Get-ChevronButton
  if (-not $chev) { Log 'chevron NOT FOUND'; return $null }
  foreach ($attempt in 1..2) {
    try {
      $pat = $chev.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
      if ($pat) { $pat.Invoke(); Log "chevron invoked (attempt $attempt)" }
    } catch {}
    $tmo = 650; if ($attempt -eq 2) { $tmo = 950 }
    if (Wait-Until { Get-PanelWindow } $tmo) { Log 'panel appeared'; return (Get-PanelWindow) }
  }
  return $null
}

function Test-Match([string]$n, [string]$target, [string]$process, [bool]$exactOnly) {
  if (-not $n) { return $false }
  if ($target -and $n -eq $target) { return $true }
  if ($process -and $n -eq $process) { return $true }
  if ($exactOnly) { return $false }
  if ($target -and $n -like "*$target*") {
    if ($n -match '输入法|指示器|网络|音频|音量|电源|时钟|通知|隐藏') { return $false }
    return $true
  }
  if ($process -and $n -like "*$process*") {
    if ($n -match '输入法|指示器|网络|音频|音量|电源|时钟|通知|隐藏') { return $false }
    return $true
  }
  return $false
}

function Test-WindowVisible([string]$procName) {
  # 目标进程任一实例出现主窗口即视为已唤出
  foreach ($p in (Get-Process $procName -ErrorAction SilentlyContinue)) {
    try { if ($p.MainWindowHandle -ne 0) { return $true } } catch {}
  }
  return $false
}

function Find-AndClickIcon([string]$procName) {
  # 可见区图标兜底（仅此路径需要 C# 互操作，首次进入才编译，面板路径零编译开销）
  Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
using System.Collections.Generic;
public class TrayNav {
  [StructLayout(LayoutKind.Sequential)] public struct NOTIFYICONIDENTIFIER { public int cbSize; public IntPtr hWnd; public uint uID; public Guid guidItem; }
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int left; public int top; public int right; public int bottom; }
  [DllImport("shell32.dll")] public static extern int Shell_NotifyIconGetRect(ref NOTIFYICONIDENTIFIER id, out RECT rect);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT pt);
  public struct POINT { public int x; public int y; }
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
  public static IntPtr[] FindWindows(uint targetPid) {
    List<IntPtr> list = new List<IntPtr>();
    EnumWindows(delegate(IntPtr h, IntPtr l) {
      uint wpid; GetWindowThreadProcessId(h, out wpid);
      if (wpid == targetPid) list.Add(h);
      return true;
    }, IntPtr.Zero);
    return list.ToArray();
  }
  public static int GetIconRect(IntPtr hwnd, uint uid, out RECT rc) {
    NOTIFYICONIDENTIFIER id = new NOTIFYICONIDENTIFIER();
    id.cbSize = Marshal.SizeOf(typeof(NOTIFYICONIDENTIFIER));
    id.hWnd = hwnd; id.uID = uid;
    return Shell_NotifyIconGetRect(ref id, out rc);
  }
  public static void Click() { mouse_event(2, 0, 0, 0, UIntPtr.Zero); mouse_event(4, 0, 0, 0, UIntPtr.Zero); }
  public static POINT Capture() { POINT p; GetCursorPos(out p); return p; }
  public static void Restore(POINT p) { SetCursorPos(p.x, p.y); }
}
'@
  $null = [TrayNav]::SetProcessDPIAware()
  # Shell_NotifyIconGetRect 即时定位（拿完立刻点，防布局漂移）→ 单击轮询验证 → 双击轮询验证
  foreach ($p in (Get-Process $procName -ErrorAction SilentlyContinue)) {
    $wins = [TrayNav]::FindWindows([uint32]$p.Id)
    foreach ($hwnd in $wins) {
      foreach ($uid in 0..9) {
        $rc = New-Object TrayNav+RECT
        $r = [TrayNav]::GetIconRect([IntPtr]$hwnd, [uint32]$uid, [ref]$rc)
        if ($r -eq 0) {
          $cx = [int](($rc.left + $rc.right) / 2)
          $cy = [int](($rc.top + $rc.bottom) / 2)
          Log "icon found hwnd=$hwnd uid=$uid center=$cx,$cy"
          $orig = [TrayNav]::Capture()
          [TrayNav]::SetCursorPos($cx, $cy) | Out-Null
          Start-Sleep -Milliseconds 150
          [TrayNav]::Click()
          if (Wait-Until { Test-WindowVisible $procName } 800) { [TrayNav]::Restore($orig); Log 'ACTIVATE single click'; return $true }
          [TrayNav]::Click()
          Start-Sleep -Milliseconds 120
          [TrayNav]::Click()
          if (Wait-Until { Test-WindowVisible $procName } 1600) { [TrayNav]::Restore($orig); Log 'ACTIVATE double click'; return $true }
          [TrayNav]::Restore($orig)
          return $false
        }
      }
    }
  }
  return $false
}

# 1. 展开溢出面板（纯 UIA，无键盘模拟）
$panel = Expand-Panel
if ($panel) {
  # 2. 枚举面板内所有 Button 图标（Name 去首尾空白）
  $icons = New-Object System.Collections.ArrayList
  try {
    $bcond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button)
    $all = $panel.FindAll([System.Windows.Automation.TreeScope]::Descendants, $bcond)
    foreach ($el in $all) {
      try {
        $n = ([string]$el.Current.Name).Trim()
        if ($n) { $null = $icons.Add(@($el, $n)) }
      } catch {}
    }
  } catch {}
  Log "icons count: $($icons.Count)"

  # 3. 匹配 + 激活：第一轮精确，第二轮包含（排除系统指示器）
  #    激活优先 UIA InvokePattern；失败 fallback SetFocus + Enter（仅单键注入，无 Win 键干扰）
  #    激活后绝不发 Esc（微信将 Esc 视为最小化回托盘）；面板残留由下次运行复用
  foreach ($exactOnly in @($true, $false)) {
    foreach ($ic in $icons) {
      $el = $ic[0]
      $n = $ic[1]
      if (Test-Match $n $Target $Process $exactOnly) {
        Log "MATCH exactOnly=$exactOnly : [$n]"
        $activated = $false
        try {
          $inv = $el.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
          if ($inv) {
            $inv.Invoke()
            $activated = $true
            Log "ACTIVATE invoke: [$n]"
          }
        } catch {}
        if (-not $activated) {
          try {
            $sf = $el.GetCurrentPattern([System.Windows.Automation.SetFocusPattern]::Pattern)
            if ($sf) {
              $sf.SetFocus()
              Start-Sleep -Milliseconds 200
              Add-Type -TypeDefinition 'using System;using System.Runtime.InteropServices;public class K1{ [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra); public static void Enter(){ keybd_event(0x0D,0,0,UIntPtr.Zero); keybd_event(0x0D,0,2,UIntPtr.Zero); }}'
              [K1]::Enter()
              $activated = $true
              Log "ACTIVATE setfocus+enter: [$n]"
            }
          } catch {}
        }
        # 窗口一出现立即 DONE（轮询，替代固定 1000ms 等待）
        if (Wait-Until { Test-WindowVisible $Process.Replace('.exe', '') } 1500) { Log 'DONE (window visible)'; exit 0 }
        Log 'invoke done but window not visible, fallback to visible-area click'
        if (Find-AndClickIcon $Process.Replace('.exe', '')) { Log 'DONE (visible-area click)'; exit 0 }
        Log 'NOTFOUND'
        exit 1
      }
    }
  }
}

# 4. 面板路径未命中 → 可见区图标点击兜底
Log 'panel path no match, try visible-area click'
if (Find-AndClickIcon $Process.Replace('.exe', '')) {
  Log 'DONE (visible-area click)'
  exit 0
}
Log 'NOTFOUND'
exit 1
