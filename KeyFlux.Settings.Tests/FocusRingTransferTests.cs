using System.Linq;
using Avalonia;
using Avalonia.Animation;
using Avalonia.Controls;
using Avalonia.Headless;
using Avalonia.Headless.XUnit;
using Avalonia.Input;
using Avalonia.Media;
using Avalonia.Threading;
using Avalonia.VisualTree;
using KeyFlux.Settings.Controls;
using KeyFlux.Settings.Models;
using KeyFlux.Settings.Services;
using KeyFlux.Settings.ViewModels;
using KeyFlux.Settings.Views;
using Xunit;

namespace KeyFlux.Settings.Tests;

/// <summary>
/// 焦点环「最内层唯一」守护 (2026-09-18 用户裁定: 子框获焦时, 描边转移到子框上,
/// 父组件框上的橙环必须消失 —— 此前 :focus-within 会让两者同时亮, 双重描边)。
/// 机制: ComponentFocusRing 取代纯 XAML :focus-within (Avalonia 无 :has(), 选择器
/// 表达不了「后代里没有自饰焦点框」), 把 .ring 类只挂到唯一目标卡上;
/// 获焦控件是/在自饰框 (TextBox/ComboBox/AutoCompleteBox/HotkeyCapture) 内 ⇒ 全卡不亮。
/// </summary>
[Collection("I18nSerial")]
public sealed class FocusRingTransferTests
{
    private static (SelectedActionPageView View, Window Win) CreateHost()
    {
        BehaviorCatalog.SeedForTests(BehaviorFixtures.Builtin(), []);
        var main = new MainViewModel(new BackendSessionOptions());
        main.Config = new Config
        {
            FileGroups = [new FileGroup { Name = "image", Label = "图片", Exts = ["jpg", "png"] }],
            SelectedAction = new SelectedAction
            {
                Mappings =
                [
                    new SelectedMapping
                    {
                        MatchType = "fileExt", MatchValue = "jpg, png",
                        Entries = [new SelectedEntry { Behavior = "open", Options = new RuleOptions() }],
                    },
                ],
            },
        };
        var page = new SelectedActionPageViewModel(main); // 构造即建两张卡 + 默认展开已配置类型编辑器

        var view = new SelectedActionPageView { DataContext = page };
        var win = new Window { Width = 1200, Height = 1000, Content = view };
        win.Show();
        Dispatcher.UIThread.RunJobs();
        return (view, win);
    }

    /// <summary>焦点元素的**有环资格**祖先卡 (页面级 actionCard, 排除行卡/编辑器卡)。</summary>
    private static Border? EligibleCard(Visual v)
    {
        foreach (var a in v.GetVisualAncestors())
        {
            if (a is Border b)
            {
                if (b.Classes.Contains("actionCard"))
                {
                    return b.Classes.Contains("row-card") || b.Classes.Contains("rowEditor")
                        ? null // 豁免框阻断外逃 (与 ComponentFocusRing 同规则)
                        : b;
                }
            }
        }
        return null;
    }

    /// <summary>① 快捷键捕获框获焦 ⇒ 父卡 (主快捷键卡) 不亮环: 环由子框自己的描边承担。</summary>
    [AvaloniaFact]
    public void HotkeyCapture_Focus_Transfers_Ring_Off_Parent_Card()
    {
        var (view, win) = CreateHost();
        try
        {
            var cap = view.GetVisualDescendants().OfType<HotkeyCapture>().First();
            var card = Assert.IsType<Border>(EligibleCard(cap));
            card.Transitions = new Transitions(); // headless 不推动画时钟, 摘掉后读终点态
            var rest = (BoxShadows)view.FindResource("SelectedActionCardShadow")!;

            Assert.True(cap.Focus());
            Dispatcher.UIThread.RunJobs();
            Assert.True(cap.IsFocused);
            Assert.DoesNotContain("ring", card.Classes);
            Assert.Equal(rest.ToString(), card.BoxShadow.ToString());
        }
        finally
        {
            win.Close();
        }
    }

    /// <summary>
    /// ② 环的唯一性与迁移: 普通按钮 (不自饰) 获焦 ⇒ 所在卡亮环 (正例, 与插件/选项页一致);
    /// 随后快捷键框获焦 ⇒ 该环**消失而非留在原地**, 且没有任何卡持有 .ring (单例)。
    /// </summary>
    [AvaloniaFact]
    public void Ring_Lights_On_Button_Focus_And_Clears_When_SelfDecorating_Box_Focuses()
    {
        var (view, win) = CreateHost();
        try
        {
            var buttons = view.GetVisualDescendants().OfType<Button>()
                .Select(b => (Btn: b, Card: EligibleCard(b)))
                .Where(x => x.Card is not null)
                .ToList();
            Assert.NotEmpty(buttons);
            var (btn, btnCard) = buttons[0];
            var cap = view.GetVisualDescendants().OfType<HotkeyCapture>().First();
            foreach (var c in new[] { btnCard, EligibleCard(cap)! }.Distinct())
            {
                c.Transitions = new Transitions();
            }
            var focusRing = (BoxShadows)view.FindResource("SelectedActionCardShadowFocus")!;
            var rest = (BoxShadows)view.FindResource("SelectedActionCardShadow")!;

            Assert.True(btn.Focus());
            Dispatcher.UIThread.RunJobs();
            Assert.Contains("ring", btnCard.Classes);
            Assert.Equal(focusRing.ToString(), btnCard.BoxShadow.ToString());

            Assert.True(cap.Focus());
            Dispatcher.UIThread.RunJobs();
            Assert.DoesNotContain("ring", btnCard.Classes);
            Assert.Equal(rest.ToString(), btnCard.BoxShadow.ToString());
            Assert.Empty(view.GetVisualDescendants().OfType<Border>()
                .Where(b => b.Classes.Contains("ring")));
        }
        finally
        {
            win.Close();
        }
    }

    /// <summary>③ 行内编辑器卡的下拉框获焦 ⇒ 全页无任何卡亮环 (豁免框阻断 + 自饰抑制双保险)。</summary>
    [AvaloniaFact]
    public void RowEditor_ComboBox_Focus_Lights_No_Card()
    {
        var (view, win) = CreateHost();
        try
        {
            var editor = view.GetVisualDescendants().OfType<Border>()
                .First(b => b.Classes.Contains("rowEditor"));
            var combo = editor.GetVisualDescendants().OfType<ComboBox>().First();

            Assert.True(combo.Focus());
            Dispatcher.UIThread.RunJobs();
            Assert.Empty(view.GetVisualDescendants().OfType<Border>()
                .Where(b => b.Classes.Contains("ring")));
        }
        finally
        {
            win.Close();
        }
    }

    /// <summary>
    /// ④ 真实指针点击链路 (2026-09-18 用户补充裁定: 点组件框本身也必须亮环):
    /// 点卡的非交互内衬 → FocusManager 上溯把焦点落在 (Focusable=True 的) 卡上 ⇒ 卡亮橙环;
    /// 再点卡内子框 (快捷键捕获框) → 焦点转移, 橙环让位给子框自己的描边 ⇒ 卡环熄灭回静止档。
    /// </summary>
    [AvaloniaFact]
    public void Click_Card_Body_Lights_Ring_Then_Click_Child_Box_Transfers_It()
    {
        var (view, win) = CreateHost();
        try
        {
            var cap = view.GetVisualDescendants().OfType<HotkeyCapture>().First();
            var card = EligibleCard(cap)!;
            card.Transitions = new Transitions();
            var focusRing = (BoxShadows)view.FindResource("SelectedActionCardShadowFocus")!;
            var rest = (BoxShadows)view.FindResource("SelectedActionCardShadow")!;

            // headless 输入管线的首个事件可能被吞, 先在窗口角落空点一下预热
            win.MouseDown(new Point(1, 1), MouseButton.Left, RawInputModifiers.None);
            win.MouseUp(new Point(1, 1), MouseButton.Left, RawInputModifiers.None);
            Dispatcher.UIThread.RunJobs();

            // ① 点卡内衬 (Padding 16 内的左上角, 无任何子控件) → 焦点落在卡上 → 橙环
            var pad = card.TranslatePoint(new Point(8, 8), win)!.Value;
            win.MouseDown(pad, MouseButton.Left, RawInputModifiers.None);
            win.MouseUp(pad, MouseButton.Left, RawInputModifiers.None);
            Dispatcher.UIThread.RunJobs();
            Assert.True(card.IsFocused, "点组件框非交互区应把焦点落在卡片上");
            Assert.Contains("ring", card.Classes);
            Assert.Equal(focusRing.ToString(), card.BoxShadow.ToString());

            // ② 再点子框 (快捷键捕获框) → 焦点转移 → 卡环熄灭 (环由子框自己的描边承担)
            var cp = cap.TranslatePoint(
                new Point(cap.Bounds.Width / 2, cap.Bounds.Height / 2), win)!.Value;
            win.MouseDown(cp, MouseButton.Left, RawInputModifiers.None);
            win.MouseUp(cp, MouseButton.Left, RawInputModifiers.None);
            Dispatcher.UIThread.RunJobs();
            Assert.True(cap.IsFocused, "点子框应聚焦快捷键捕获框");
            Assert.DoesNotContain("ring", card.Classes);

            // 移开指针消除 :pointerover 档, 阴影回到静止档再断言 (焦点环声明在悬停档之后)
            win.MouseMove(new Point(1, 1));
            Dispatcher.UIThread.RunJobs();
            Assert.Equal(rest.ToString(), card.BoxShadow.ToString());
        }
        finally
        {
            win.Close();
        }
    }
}
