/**
 * translation.ahk —— 界面文案多语言 (中文/英文)。
 * DefaultTranslation 为英文基类, ChineseTranslation 继承并覆写中文文案;
 * Translation() 按系统语言缓存单例 (首次调用后固定, 不随运行中变化)。
 * 新增文案: 先在 DefaultTranslation 加英文, 再在 ChineseTranslation 加中文。
 */
class DefaultTranslation {
  keyflux_on := "🚀  KeyFlux: On  "
  keyflux_off := "⏸️  KeyFlux: Off  "
  
  menu_pause := "Pause"
  menu_exit := "Exit"
  menu_reload := "Reload"
  menu_settings := "Settings"
  menu_window_spy := "Window Spy"

  no_items_selected := "no items selected"
  no_matching_type := "no matching type for the selection"
  always_on_top_on := "Always-on-top: On"
  always_on_top_off := "Always-on-top: Off"
  copy_failed := " Copy: fail "
  copy_ok := " Copy: ok "
  mute_on := "Mute: On"
  mute_off := "Mute: Off"
  mute_falied := "Cannot mute this app"
  app_running_in_background := "App is running in background, click the tray icon to show it"
  magnet_no_handler := "No handler registered for magnet: links. Install a BT client first"
  registry_open_failed := "Cannot locate the registry key"

  ; QuickSwitch 浮层相对时间 (FolderRanker 消费)
  qs_time_just_now := "just now"
  qs_time_min_ago := "{1} min ago"
  qs_time_hour_ago := "{1} h ago"
  qs_time_yesterday := "yesterday"
}

class ChineseTranslation extends DefaultTranslation {
  keyflux_on :=  "🚀  恢复 KeyFlux  "
  keyflux_off := "⏸️  暂停 KeyFlux  "

  menu_pause := "暂停"
  menu_exit := "退出"
  menu_reload := "重启程序"
  menu_settings := "打开设置"
  menu_window_spy := "查看窗口标识符"

  no_items_selected := "没有选中的文本或文件"
  no_matching_type := "未识别的类型"
  always_on_top_on := "置顶当前窗口"
  always_on_top_off := "取消置顶"
  copy_failed := "复制失败"
  copy_ok := "复制成功"
  mute_on := "静音当前应用"
  mute_off := "取消静音"
  mute_falied := "无法静音此应用"
  app_running_in_background := "程序在后台运行，请点击托盘图标唤出"
  magnet_no_handler := "未检测到磁力链接的默认下载程序 (magnet: 协议未关联), 请先安装 BT 下载工具"
  registry_open_failed := "无法定位注册表路径, 请检查选中内容是否为注册表键路径"

  ; QuickSwitch 浮层相对时间 (FolderRanker 消费)
  qs_time_just_now := "刚刚"
  qs_time_min_ago := "{1} 分钟前"
  qs_time_hour_ago := "{1} 小时前"
  qs_time_yesterday := "昨天"
}


; 按系统语言选择翻译单例; static 缓存, 全进程只判断一次。
Translation() {
  static t := SysLangIsChinese() ? ChineseTranslation() : DefaultTranslation()
  return t
}

; A_Language 为区域标识十六进制串, 凡是中文系区域都视为中文 (见注释中的官方语言表链接)。
SysLangIsChinese()
{
  ; https://www.autohotkey.com/docs/v2/misc/Languages.htm
  m := Map(
    "7804", "Chinese",  ; zh
    "0004", "Chinese (Simplified)",  ; zh-Hans
    "0804", "Chinese (Simplified, China)",  ; zh-CN
    "1004", "Chinese (Simplified, Singapore)",  ; zh-SG
    "7C04", "Chinese (Traditional)",  ; zh-Hant
    "0C04", "Chinese (Traditional, Hong Kong SAR)",  ; zh-HK
    "1404", "Chinese (Traditional, Macao SAR)",  ; zh-MO
    "0404", "Chinese (Traditional, Taiwan)",  ; zh-TW
  )
  return m.Get(A_Language, false)
}