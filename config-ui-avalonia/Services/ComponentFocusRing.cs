using System.Collections.Generic;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Threading;
using Avalonia.VisualTree;
using KeyFlux.Settings.Controls;

namespace KeyFlux.Settings.Services;

/// <summary>
/// 组件框焦点环「最内层唯一」转移器 (2026-09-18 用户裁定)。
///
/// 背景: 原先各页组件框用纯 XAML 的 <c>:focus-within</c> 点亮橙色焦点环, 但该伪类会
/// 同时命中焦点元素的**所有**祖先卡 ⇒ 点击卡内自带描边的小框 (快捷键捕获框 / 文本框 /
/// 下拉框) 时, 父卡橙环与小框自己的描边**同时出现**, 双重描边。用户裁定:
/// 「子框被点击时, 描边转移到子框上, 父框上的消失」。
///
/// Avalonia 没有 <c>:has()</c>, 无法用选择器表达「后代里没有已自饰的焦点框」, 故改为
/// 代码侧管理 <c>.ring</c> 类: 页面根挂 GotFocus/LostFocus (11.3 中二者均为 Bubble,
/// 已用反射核实), 焦点变化后经 Dispatcher 防抖重算一次 (等焦点迁移完全落定再读
/// FocusManager, 规避事件与 FocusedElement 的更新时序), 并把 <c>.ring</c> 只挂到
/// **唯一的**目标卡 Border 上; 各页样式选择器由 <c>:focus-within</c> 换成 <c>.ring</c>。
///
/// 抑制规则: 获焦控件本身 (或其祖先) 是自饰框 (TextBox / ComboBox / AutoCompleteBox /
/// HotkeyCapture —— 它们的模板/代码在聚焦时自己画边框) ⇒ 任何卡都不亮, 环"转移"到子框。
/// ToggleSwitch/Button/CheckBox 不自饰 ⇒ 环留在最内层组件框上 (与既有交互一致)。
/// 遍历止步: 命中豁免框 (行卡 .row-card / 行内编辑器 .rowEditor) 时整体放弃而非跳过,
/// 防止环逃到更外层。
/// </summary>
public static class ComponentFocusRing
{
    /// <summary>把焦点环管理挂到页面 (或窗口) 根; 各承载组件框的视图构造器调用一次。</summary>
    public static void Attach(Control scope) => new RingScope(scope);

    private sealed class RingScope
    {
        /// <summary>有资格承载焦点环的组件框类 (与四页 .ring 样式一一对应)。</summary>
        private static readonly string[] CardClasses =
            ["actionCard", "pluginCard", "settingsCard", "marketCard", "leftPanel"];

        /// <summary>豁免框: 列表条目/嵌套面板, 命中即放弃 (不得把环让渡给更外层)。</summary>
        private static readonly string[] BlockedClasses = ["row-card", "rowEditor"];

        private readonly Control _scope;
        private Border? _marked;
        private bool _queued;

        public RingScope(Control scope)
        {
            _scope = scope;
            scope.AddHandler(InputElement.GotFocusEvent, (_, _) => QueueRecompute());
            scope.AddHandler(InputElement.LostFocusEvent, (_, _) => QueueRecompute());
        }

        private void QueueRecompute()
        {
            if (_queued) return;
            _queued = true;
            Dispatcher.UIThread.Post(() =>
            {
                _queued = false;
                Recompute();
            });
        }

        private void Recompute()
        {
            var focused = TopLevel.GetTopLevel(_scope)?.FocusManager?.GetFocusedElement() as Visual;
            var target = focused is null || IsSelfDecorating(focused) ? null : FindCard(focused);
            if (ReferenceEquals(_marked, target)) return;
            if (_marked is not null) _marked.Classes.Remove("ring");
            _marked = target;
            if (target is not null) target.Classes.Add("ring");
        }

        /// <summary>焦点元素自身或祖先为自饰输入框 ⇒ 由子框自己画边框, 卡环熄灭。</summary>
        private bool IsSelfDecorating(Visual focused)
        {
            foreach (var v in EnumerateSelfAndAncestors(focused))
            {
                if (v is TextBox or ComboBox or AutoCompleteBox or HotkeyCapture) return true;
                if (ReferenceEquals(v, _scope)) break;
            }
            return false;
        }

        /// <summary>
        /// 自焦点元素 (含其自身 —— 卡片 Border 已 Focusable, 点非交互区时 FocusManager
        /// 上溯直接把焦点落在卡上) 向上找**最近的**组件框卡; 命中豁免框则直接放弃。
        /// </summary>
        private Border? FindCard(Visual focused)
        {
            foreach (var v in focused.GetSelfAndVisualAncestors())
            {
                if (v is Border b)
                {
                    if (HasAny(b.Classes, BlockedClasses)) return null;
                    if (HasAny(b.Classes, CardClasses)) return b;
                }
                if (ReferenceEquals(v, _scope)) return null;
            }
            return null;
        }

        private static bool HasAny(Classes classes, string[] targets)
        {
            foreach (var t in targets)
            {
                if (classes.Contains(t)) return true;
            }
            return false;
        }

        private static IEnumerable<Visual?> EnumerateSelfAndAncestors(Visual v)
        {
            for (Visual? cur = v; cur is not null; cur = cur.GetVisualParent())
            {
                yield return cur;
            }
        }
    }
}
