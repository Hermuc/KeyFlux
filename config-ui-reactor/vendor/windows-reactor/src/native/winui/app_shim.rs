use super::*;
use windows_core::*;

implement_decl! {
    impl ReactorApplicationOverrides as pub ReactorApplicationOverrides_Impl:
        [IApplicationOverrides, IXamlMetadataProvider]
}

pub struct ReactorApplicationOverrides {
    controls_provider: RefCell<Option<XamlControlsXamlMetaDataProvider>>,
    on_launched: RefCell<Option<Box<dyn FnOnce() -> Result<()>>>>,
}

impl ReactorApplicationOverrides {
    fn new(on_launched: Box<dyn FnOnce() -> Result<()>>) -> Self {
        Self {
            controls_provider: RefCell::new(None),
            on_launched: RefCell::new(Some(on_launched)),
        }
    }

    fn provider(&self) -> Result<XamlControlsXamlMetaDataProvider> {
        if let Some(provider) = self.controls_provider.borrow().as_ref() {
            return Ok(provider.clone());
        }
        let provider = XamlControlsXamlMetaDataProvider::new()?;
        *self.controls_provider.borrow_mut() = Some(provider.clone());
        Ok(provider)
    }
}

impl IApplicationOverrides_Impl for ReactorApplicationOverrides_Impl {
    fn OnLaunched(&self, _args: Ref<LaunchActivatedEventArgs>) -> Result<()> {
        if let Some(on_launched) = self.on_launched.borrow_mut().take() {
            on_launched()?;
        }
        Ok(())
    }
}

impl IXamlMetadataProvider_Impl for ReactorApplicationOverrides_Impl {
    fn GetXamlType(&self, r#type: &TypeName) -> Result<IXamlType> {
        self.provider()?.GetXamlType(r#type)
    }

    fn GetXamlTypeByFullName(&self, full_name: &HSTRING) -> Result<IXamlType> {
        self.provider()?
            .GetXamlTypeByFullName(&full_name.to_string_lossy())
    }

    fn GetXmlnsDefinitions(&self) -> Result<Array<XmlnsDefinition>> {
        self.provider()?.GetXmlnsDefinitions()
    }
}

pub fn create_application(on_launched: Box<dyn FnOnce() -> Result<()>>) -> Result<Application> {
    Application::compose(ReactorApplicationOverrides::new(on_launched))
}

pub fn install_xaml_controls_resources(application: &Application) -> Result<()> {
    let controls = XamlControlsResources::new()?;
    let resources: ResourceDictionary = controls.cast()?;
    application
        .Resources()?
        .MergedDictionaries()?
        .Append(&resources)
}

/// 全局 UI 字体覆盖（vendor 补丁，2026-09-28）：与旧版 App.axaml 同键同值，
/// 覆盖 Fluent 模板内部的 `ContentControlThemeFontFamily` 主题资源键。
///
/// 实现走 `XamlReader::Load` 解析字典（reactor 绑定未含 FontFamily 工厂，
/// 用 XAML 字符串可完全绕开手工 IID/类型绑定）。后 Append 的合并字典优先级更高，
/// 因此能压过前一条 install_xaml_controls_resources 装入的 Fluent 字典。
/// 前置条件：宿主进程已把 MiSans ttf 私有加载（GDI AddFontResourceW）或系统可解析
/// `MiSans` 家族名；否则回落 Microsoft YaHei UI（字体链兜底，不致渲染失败）。
pub fn install_global_ui_font(application: &Application) -> Result<()> {
    // 2026-09-29 修正：普通合并字典条目**压不过** XamlControlsResources 自身主题字典里的
    // 同名键（TextBlock 默认样式的 {ThemeResource} 引用在库字典内部解析——真机截图实证
    // 全面板仍是雅黑回退）。文档化的主题资源覆盖姿势 = 在 App 级字典里放
    // **ThemeDictionaries**（末位合并优先，且按激活主题命中）。
    const FONT_DICT_XAML: &str = r#"<ResourceDictionary
    xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
    xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
    <ResourceDictionary.ThemeDictionaries>
        <ResourceDictionary x:Key="Light">
            <FontFamily x:Key="ContentControlThemeFontFamily">MiSans, Segoe UI Emoji</FontFamily>
        </ResourceDictionary>
        <ResourceDictionary x:Key="Default">
            <FontFamily x:Key="ContentControlThemeFontFamily">MiSans, Segoe UI Emoji</FontFamily>
        </ResourceDictionary>
    </ResourceDictionary.ThemeDictionaries>

</ResourceDictionary>"#;

    let value = XamlReader::Load(FONT_DICT_XAML)?;
    let dictionary: ResourceDictionary = value.cast()?;
    application
        .Resources()?
        .MergedDictionaries()?
        .Append(&dictionary)
}

/// 弹窗遮罩层覆盖（vendor 补丁，2026-10-01）：让 `ContentDialog` 独立浮出，
/// **不再把背后的页面/面板整体压暗**。
///
/// ## 为什么不能改弹窗自身
///
/// WinUI 的 `ContentDialog` 由 Popup 承载，smoke（遮罩）**不在弹窗模板里**，而是
/// Popup 根 `Canvas` 上的一个 `Rectangle x:Name="SmokeLayerBackground"`，与弹窗平级：
///
/// ```text
/// PopupRoot (Canvas)
///   ├─ Rectangle  x:Name="SmokeLayerBackground"   ← 变暗来源（铺满 XamlRoot）
///   └─ ContentDialog                              ← 真正的弹窗
/// ```
///
/// 因此给 dialog 设 `Background` / `BorderThickness` 都无效（社区同款结论：
/// <https://stackoverflow.com/a/79435072>）。它唯一的着色入口是主题资源键
/// `ContentDialogSmokeFill`（WinUI3 的 `XamlControlsResources` 字典内，
/// 键表实测见 `Microsoft.UI.Xaml.Controls.pri`：`ContentDialogSmokeFill` /
/// `ContentDialogTopOverlay` / `ContentDialogBackground` …；
/// 元素名 `SmokeLayerBackground` 在 `Microsoft.ui.xaml.dll` 中，**不是**资源键 ——
/// 只覆盖它会静默无效）。UWP 时代同义键名 `ContentDialogDimmingThemeBrush`
/// （见 `Microsoft.ui.xaml.resources.*.dll`）一并覆盖以防 downlevel 回落。
///
/// ## 修法
///
/// 沿用本文件既有通道：App 级资源字典末尾追加一条带 **ThemeDictionaries** 的覆盖字典
/// （末位合并优先、按激活主题命中），把遮罩填充刷成完全透明。
/// 弹窗仍由 Popup 承载 ⇒ **定位 / 层级 / 焦点与点击拦截（命中测试仍落在 Popup 层，
/// 透明画刷仍参与命中测试）全部不变**，只是背后不再变暗。
pub fn install_dialog_layer_overrides(application: &Application) -> Result<()> {
    const DICT_XAML: &str = r#"<ResourceDictionary
    xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
    xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
    <ResourceDictionary.ThemeDictionaries>
        <ResourceDictionary x:Key="Light">
            <SolidColorBrush x:Key="ContentDialogSmokeFill" Color="Transparent" />
            <SolidColorBrush x:Key="ContentDialogTopOverlay" Color="Transparent" />
            <SolidColorBrush x:Key="ContentDialogDimmingThemeBrush" Color="Transparent" />
        </ResourceDictionary>
        <ResourceDictionary x:Key="Dark">
            <SolidColorBrush x:Key="ContentDialogSmokeFill" Color="Transparent" />
            <SolidColorBrush x:Key="ContentDialogTopOverlay" Color="Transparent" />
            <SolidColorBrush x:Key="ContentDialogDimmingThemeBrush" Color="Transparent" />
        </ResourceDictionary>
        <ResourceDictionary x:Key="HighContrast">
            <SolidColorBrush x:Key="ContentDialogSmokeFill" Color="Transparent" />
            <SolidColorBrush x:Key="ContentDialogTopOverlay" Color="Transparent" />
            <SolidColorBrush x:Key="ContentDialogDimmingThemeBrush" Color="Transparent" />
        </ResourceDictionary>
        <ResourceDictionary x:Key="Default">
            <SolidColorBrush x:Key="ContentDialogSmokeFill" Color="Transparent" />
            <SolidColorBrush x:Key="ContentDialogTopOverlay" Color="Transparent" />
            <SolidColorBrush x:Key="ContentDialogDimmingThemeBrush" Color="Transparent" />
        </ResourceDictionary>
    </ResourceDictionary.ThemeDictionaries>

</ResourceDictionary>"#;

    let value = XamlReader::Load(DICT_XAML)?;
    let dictionary: ResourceDictionary = value.cast()?;
    application
        .Resources()?
        .MergedDictionaries()?
        .Append(&dictionary)
}
/// 滚动条响应计时覆盖（vendor 补丁，2026-10-06）：App 级隐式 `ScrollBar` 样式，
/// 让悬停展开 / 移出收起**即时**响应，去掉 Fluent 模板内置的起手延迟。
///
/// ## 根因
///
/// WinUI3 Fluent 的 `ScrollBar` 模板（`ScrollBar_themeresources.xaml`）把展开 / 收起
/// 两组动画的 `BeginTime` 都挂在了资源键上：
///
/// - `ScrollBarExpandBeginTime` = `00:00:00.40` —— 悬停后先等 **400ms** 才开始展开；
/// - `ScrollBarContractBeginTime` = `00:00:00.50` —— 移出后先等 **500ms** 才开始收起。
///
/// 两头都是「先延迟再动画」，宏观感受就是滚动条状态切换永远慢半拍（2026-10-06 用户
/// 报障：选项页滚动条 hover 不及时展开、移走不及时还原）。
///
/// ## 为什么只能整段重模板（两次尝试的结论）
///
/// 1. **App 级覆盖计时键无效**：模板 XAML 经 XBF 编译后，`{StaticResource ScrollBarExpandBeginTime}`
///    在**编译期**就绑定进了控件自身的资源字典，App 级合并字典（顶层条目或
///    ThemeDictionaries 双写）都压不进去——真机部署实测无效（2026-10-06）。
///    这与 P1 的教训同源：只有 `{ThemeResource}` 引用能被 App 级主题字典覆盖，
///    `{StaticResource}` 不能。而模板内**恰恰是** `{StaticResource}` 引用。
/// 2. **ThemeDictionaries 别名层无法保留**：模板里的刷子引用走别名键
///    （`ScrollBarThumbFill` → `ControlStrongFillColorDefaultBrush`），别名是
///    解析期冻结的 `<StaticResource>`，经 `XamlReader` 运行时装载时按启动主题
///    冻结取值，主题切换不跟随。
///
/// ⇒ 最终做法：把 Fluent ScrollBar 模板整体转为 **App 级隐式样式**（隐式样式
/// 按资源查找顺序压过 `XamlControlsResources` 的隐式样式），并在转换时：
///
/// - 全部标量（时长 / 尺寸 / 边距 / 圆角）**内联为字面量**，`BeginTime` 归零；
/// - 模板内的别名刷子引用**直接解析到底层主题刷子键**（如
///   `{ThemeResource ControlStrongFillColorDefaultBrush}`），运行时按激活主题解析，
///   浅色 / 深色跟随系统；HighContrast 的 ScrollBar 专用重定向丢失（回落标准
///   Fluent 刷子）——面板本身是固定浅色玻璃设计，可接受；
/// - 无任何指向字典外的 `{StaticResource}` 残留（转换自检通过）。
///
/// 模板来源：microsoft-ui-xaml `controls/dev/CommonStyles/ScrollBar_themeresources.xaml`
/// （main 分支，2026-10-06 抓取），与本进程 `Microsoft.UI.Xaml.Controls.pri` 键表核对一致。
/// 下游键若随 WinUI 升级改名，症状是滚动条整体不可见 / 无动画，届时用同法重转即可。
pub fn install_scroll_bar_overrides(application: &Application) -> Result<()> {
    const SCROLL_BAR_STYLE_XAML: &str = r#"<ResourceDictionary
    xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
    xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
    xmlns:primitives="using:Microsoft.UI.Xaml.Controls.Primitives">
    <primitives:CornerRadiusFilterConverter x:Key="TopLeftCornerRadiusDoubleValueConverter8x" Filter="TopLeftValue" Scale="8" />
    <primitives:CornerRadiusFilterConverter x:Key="BottomRightCornerRadiusDoubleValueConverter8x" Filter="BottomRightValue" Scale="8" />
    <primitives:CornerRadiusFilterConverter x:Key="TopLeftCornerRadiusDoubleValueConverter2x" Filter="TopLeftValue" Scale="2" />
    <primitives:CornerRadiusFilterConverter x:Key="BottomRightCornerRadiusDoubleValueConverter2x" Filter="BottomRightValue" Scale="2" />
    <Style TargetType="ScrollBar">
    <Setter Property="MinWidth" Value="12" />
    <Setter Property="MinHeight" Value="12" />
    <Setter Property="Background" Value="{ThemeResource SubtleFillColorTransparentBrush}" />
    <Setter Property="Foreground" Value="{ThemeResource SubtleFillColorTransparentBrush}" />
    <Setter Property="BorderBrush" Value="{ThemeResource SubtleFillColorTransparentBrush}" />
    <Setter Property="IsTabStop" Value="False" />
    <Setter Property="CornerRadius" Value="3" />
    <Setter Property="Template">
      <Setter.Value>
        <ControlTemplate TargetType="ScrollBar">
          <Grid x:Name="Root">
            <Grid.Resources>
              <ControlTemplate x:Key="RepeatButtonTemplate" TargetType="RepeatButton">
                <Grid x:Name="Root" Background="Transparent">
                  <VisualStateManager.VisualStateGroups>
                    <VisualStateGroup x:Name="CommonStates">
                      <VisualState x:Name="Normal" />
                    </VisualStateGroup>
                  </VisualStateManager.VisualStateGroups>
                </Grid>
              </ControlTemplate>
              <ControlTemplate x:Key="HorizontalIncrementTemplate" TargetType="RepeatButton">
                <Grid x:Name="Root" Background="{ThemeResource SubtleFillColorTransparentBrush}" BorderBrush="{ThemeResource SubtleFillColorTransparentBrush}" Padding="0,0,4,0">
                  <VisualStateManager.VisualStateGroups>
                    <VisualStateGroup x:Name="CommonStates">
                      <VisualState x:Name="Normal" />
                      <VisualState x:Name="PointerOver">
                        <Storyboard>
                          <ObjectAnimationUsingKeyFrames Storyboard.TargetName="Arrow" Storyboard.TargetProperty="Foreground">
                            <DiscreteObjectKeyFrame KeyTime="0" Value="{ThemeResource TextFillColorSecondaryBrush}" />
                          </ObjectAnimationUsingKeyFrames>
                        </Storyboard>
                      </VisualState>
                      <VisualState x:Name="Pressed">
                        <Storyboard>
                          <ObjectAnimationUsingKeyFrames Storyboard.TargetName="Arrow" Storyboard.TargetProperty="Foreground">
                            <DiscreteObjectKeyFrame KeyTime="0" Value="{ThemeResource TextFillColorSecondaryBrush}" />
                          </ObjectAnimationUsingKeyFrames>
                          <DoubleAnimationUsingKeyFrames Storyboard.TargetName="Arrow" Storyboard.TargetProperty="(UIElement.RenderTransform).(ScaleTransform.ScaleX)" RepeatBehavior="Forever">
                            <DiscreteDoubleKeyFrame KeyTime="0:0:0.016" Value="0.875" />
                            <DiscreteDoubleKeyFrame KeyTime="0:0:30" Value="0.875" />
                          </DoubleAnimationUsingKeyFrames>
                          <DoubleAnimationUsingKeyFrames Storyboard.TargetName="Arrow" Storyboard.TargetProperty="(UIElement.RenderTransform).(ScaleTransform.ScaleY)" RepeatBehavior="Forever">
                            <DiscreteDoubleKeyFrame KeyTime="0:0:0.016" Value="0.875" />
                            <DiscreteDoubleKeyFrame KeyTime="0:0:30" Value="0.875" />
                          </DoubleAnimationUsingKeyFrames>
                        </Storyboard>
                      </VisualState>
                      <VisualState x:Name="Disabled">
                        <Storyboard>
                          <ObjectAnimationUsingKeyFrames Storyboard.TargetName="Arrow" Storyboard.TargetProperty="Foreground">
                            <DiscreteObjectKeyFrame KeyTime="0" Value="{ThemeResource ControlStrongFillColorDisabledBrush}" />
                          </ObjectAnimationUsingKeyFrames>
                        </Storyboard>
                      </VisualState>
                    </VisualStateGroup>
                  </VisualStateManager.VisualStateGroups>
                  <FontIcon x:Name="Arrow" RenderTransformOrigin="0.5, 0.5" FontFamily="{ThemeResource SymbolThemeFontFamily}" Glyph="&#xEDDA;" Foreground="{ThemeResource ControlStrongFillColorDefaultBrush}" FontSize="8" MirroredWhenRightToLeft="True">
                    <FontIcon.RenderTransform>
                      <ScaleTransform x:Name="ScaleTransform" ScaleY="1" ScaleX="1" />
                    </FontIcon.RenderTransform>
                  </FontIcon>
                </Grid>
              </ControlTemplate>
              <ControlTemplate x:Key="HorizontalDecrementTemplate" TargetType="RepeatButton">
                <Grid x:Name="Root" Background="{ThemeResource SubtleFillColorTransparentBrush}" BorderBrush="{ThemeResource SubtleFillColorTransparentBrush}" Padding="4,0,0,0">
                  <VisualStateManager.VisualStateGroups>
                    <VisualStateGroup x:Name="CommonStates">
                      <VisualState x:Name="Normal" />
                      <VisualState x:Name="PointerOver">
                        <Storyboard>
                          <ObjectAnimationUsingKeyFrames Storyboard.TargetName="Arrow" Storyboard.TargetProperty="Foreground">
                            <DiscreteObjectKeyFrame KeyTime="0" Value="{ThemeResource TextFillColorSecondaryBrush}" />
                          </ObjectAnimationUsingKeyFrames>
                        </Storyboard>
                      </VisualState>
                      <VisualState x:Name="Pressed">
                        <Storyboard>
                          <ObjectAnimationUsingKeyFrames Storyboard.TargetName="Arrow" Storyboard.TargetProperty="Foreground">
                            <DiscreteObjectKeyFrame KeyTime="0" Value="{ThemeResource TextFillColorSecondaryBrush}" />
                          </ObjectAnimationUsingKeyFrames>
                          <DoubleAnimationUsingKeyFrames Storyboard.TargetName="Arrow" Storyboard.TargetProperty="(UIElement.RenderTransform).(ScaleTransform.ScaleX)" RepeatBehavior="Forever">
                            <DiscreteDoubleKeyFrame KeyTime="0:0:0.016" Value="0.875" />
                            <DiscreteDoubleKeyFrame KeyTime="0:0:30" Value="0.875" />
                          </DoubleAnimationUsingKeyFrames>
                          <DoubleAnimationUsingKeyFrames Storyboard.TargetName="Arrow" Storyboard.TargetProperty="(UIElement.RenderTransform).(ScaleTransform.ScaleY)" RepeatBehavior="Forever">
                            <DiscreteDoubleKeyFrame KeyTime="0:0:0.016" Value="0.875" />
                            <DiscreteDoubleKeyFrame KeyTime="0:0:30" Value="0.875" />
                          </DoubleAnimationUsingKeyFrames>
                        </Storyboard>
                      </VisualState>
                      <VisualState x:Name="Disabled">
                        <Storyboard>
                          <ObjectAnimationUsingKeyFrames Storyboard.TargetName="Arrow" Storyboard.TargetProperty="Foreground">
                            <DiscreteObjectKeyFrame KeyTime="0" Value="{ThemeResource ControlStrongFillColorDisabledBrush}" />
                          </ObjectAnimationUsingKeyFrames>
                        </Storyboard>
                      </VisualState>
                    </VisualStateGroup>
                  </VisualStateManager.VisualStateGroups>
                  <FontIcon x:Name="Arrow" RenderTransformOrigin="0.5, 0.5" FontFamily="{ThemeResource SymbolThemeFontFamily}" Glyph="&#xEDD9;" Foreground="{ThemeResource ControlStrongFillColorDefaultBrush}" FontSize="8" MirroredWhenRightToLeft="True">
                    <FontIcon.RenderTransform>
                      <ScaleTransform x:Name="ScaleTransform" ScaleY="1" ScaleX="1" />
                    </FontIcon.RenderTransform>
                  </FontIcon>
                </Grid>
              </ControlTemplate>
              <ControlTemplate x:Key="VerticalIncrementTemplate" TargetType="RepeatButton">
                <Grid x:Name="Root" Background="{ThemeResource SubtleFillColorTransparentBrush}" BorderBrush="{ThemeResource SubtleFillColorTransparentBrush}" Padding="0,0,0,4">
                  <VisualStateManager.VisualStateGroups>
                    <VisualStateGroup x:Name="CommonStates">
                      <VisualState x:Name="Normal" />
                      <VisualState x:Name="PointerOver">
                        <Storyboard>
                          <ObjectAnimationUsingKeyFrames Storyboard.TargetName="Arrow" Storyboard.TargetProperty="Foreground">
                            <DiscreteObjectKeyFrame KeyTime="0" Value="{ThemeResource TextFillColorSecondaryBrush}" />
                          </ObjectAnimationUsingKeyFrames>
                        </Storyboard>
                      </VisualState>
                      <VisualState x:Name="Pressed">
                        <Storyboard>
                          <ObjectAnimationUsingKeyFrames Storyboard.TargetName="Arrow" Storyboard.TargetProperty="Foreground">
                            <DiscreteObjectKeyFrame KeyTime="0" Value="{ThemeResource TextFillColorSecondaryBrush}" />
                          </ObjectAnimationUsingKeyFrames>
                          <DoubleAnimationUsingKeyFrames Storyboard.TargetName="Arrow" Storyboard.TargetProperty="(UIElement.RenderTransform).(ScaleTransform.ScaleX)" RepeatBehavior="Forever">
                            <DiscreteDoubleKeyFrame KeyTime="0:0:0.016" Value="0.875" />
                            <DiscreteDoubleKeyFrame KeyTime="0:0:30" Value="0.875" />
                          </DoubleAnimationUsingKeyFrames>
                          <DoubleAnimationUsingKeyFrames Storyboard.TargetName="Arrow" Storyboard.TargetProperty="(UIElement.RenderTransform).(ScaleTransform.ScaleY)" RepeatBehavior="Forever">
                            <DiscreteDoubleKeyFrame KeyTime="0:0:0.016" Value="0.875" />
                            <DiscreteDoubleKeyFrame KeyTime="0:0:30" Value="0.875" />
                          </DoubleAnimationUsingKeyFrames>
                        </Storyboard>
                      </VisualState>
                      <VisualState x:Name="Disabled">
                        <Storyboard>
                          <ObjectAnimationUsingKeyFrames Storyboard.TargetName="Arrow" Storyboard.TargetProperty="Foreground">
                            <DiscreteObjectKeyFrame KeyTime="0" Value="{ThemeResource ControlStrongFillColorDisabledBrush}" />
                          </ObjectAnimationUsingKeyFrames>
                        </Storyboard>
                      </VisualState>
                    </VisualStateGroup>
                  </VisualStateManager.VisualStateGroups>
                  <FontIcon x:Name="Arrow" RenderTransformOrigin="0.5, 0.5" FontFamily="{ThemeResource SymbolThemeFontFamily}" Glyph="&#xEDDC;" Foreground="{ThemeResource ControlStrongFillColorDefaultBrush}" FontSize="8">
                    <FontIcon.RenderTransform>
                      <ScaleTransform x:Name="ScaleTransform" ScaleY="1" ScaleX="1" />
                    </FontIcon.RenderTransform>
                  </FontIcon>
                </Grid>
              </ControlTemplate>
              <ControlTemplate x:Key="VerticalDecrementTemplate" TargetType="RepeatButton">
                <Grid x:Name="Root" Background="{ThemeResource SubtleFillColorTransparentBrush}" BorderBrush="{ThemeResource SubtleFillColorTransparentBrush}" Padding="0,4,0,0">
                  <VisualStateManager.VisualStateGroups>
                    <VisualStateGroup x:Name="CommonStates">
                      <VisualState x:Name="Normal" />
                      <VisualState x:Name="PointerOver">
                        <Storyboard>
                          <ObjectAnimationUsingKeyFrames Storyboard.TargetName="Arrow" Storyboard.TargetProperty="Foreground">
                            <DiscreteObjectKeyFrame KeyTime="0" Value="{ThemeResource TextFillColorSecondaryBrush}" />
                          </ObjectAnimationUsingKeyFrames>
                        </Storyboard>
                      </VisualState>
                      <VisualState x:Name="Pressed">
                        <Storyboard>
                          <ObjectAnimationUsingKeyFrames Storyboard.TargetName="Arrow" Storyboard.TargetProperty="Foreground">
                            <DiscreteObjectKeyFrame KeyTime="0" Value="{ThemeResource TextFillColorSecondaryBrush}" />
                          </ObjectAnimationUsingKeyFrames>
                          <DoubleAnimationUsingKeyFrames Storyboard.TargetName="Arrow" Storyboard.TargetProperty="(UIElement.RenderTransform).(ScaleTransform.ScaleX)" RepeatBehavior="Forever">
                            <DiscreteDoubleKeyFrame KeyTime="0:0:0.016" Value="0.875" />
                            <DiscreteDoubleKeyFrame KeyTime="0:0:30" Value="0.875" />
                          </DoubleAnimationUsingKeyFrames>
                          <DoubleAnimationUsingKeyFrames Storyboard.TargetName="Arrow" Storyboard.TargetProperty="(UIElement.RenderTransform).(ScaleTransform.ScaleY)" RepeatBehavior="Forever">
                            <DiscreteDoubleKeyFrame KeyTime="0:0:0.016" Value="0.875" />
                            <DiscreteDoubleKeyFrame KeyTime="0:0:30" Value="0.875" />
                          </DoubleAnimationUsingKeyFrames>
                        </Storyboard>
                      </VisualState>
                      <VisualState x:Name="Disabled">
                        <Storyboard>
                          <ObjectAnimationUsingKeyFrames Storyboard.TargetName="Arrow" Storyboard.TargetProperty="Foreground">
                            <DiscreteObjectKeyFrame KeyTime="0" Value="{ThemeResource ControlStrongFillColorDisabledBrush}" />
                          </ObjectAnimationUsingKeyFrames>
                        </Storyboard>
                      </VisualState>
                    </VisualStateGroup>
                  </VisualStateManager.VisualStateGroups>
                  <FontIcon x:Name="Arrow" RenderTransformOrigin="0.5, 0.5" FontFamily="{ThemeResource SymbolThemeFontFamily}" Glyph="&#xEDDB;" Foreground="{ThemeResource ControlStrongFillColorDefaultBrush}" FontSize="8">
                    <FontIcon.RenderTransform>
                      <ScaleTransform x:Name="ScaleTransform" ScaleY="1" ScaleX="1" />
                    </FontIcon.RenderTransform>
                  </FontIcon>
                </Grid>
              </ControlTemplate>
              <ControlTemplate x:Key="VerticalThumbTemplate" TargetType="Thumb">
                <Rectangle x:Name="ThumbVisual" Fill="{TemplateBinding Background}" Stroke="{TemplateBinding BorderBrush}" StrokeThickness="6" RadiusX="{Binding CornerRadius, RelativeSource={RelativeSource TemplatedParent}, Converter={StaticResource TopLeftCornerRadiusDoubleValueConverter}}" RadiusY="{Binding CornerRadius, RelativeSource={RelativeSource TemplatedParent}, Converter={StaticResource BottomRightCornerRadiusDoubleValueConverter}}">
                  <VisualStateManager.VisualStateGroups>
                    <VisualStateGroup x:Name="CommonStates">
                      <VisualState x:Name="Normal" />
                      <VisualState x:Name="Disabled">
                        <Storyboard>
                          <ObjectAnimationUsingKeyFrames Storyboard.TargetName="ThumbVisual" Storyboard.TargetProperty="Fill">
                            <DiscreteObjectKeyFrame KeyTime="00:00:00.083" Value="{ThemeResource ControlStrongFillColorDisabledBrush}" />
                          </ObjectAnimationUsingKeyFrames>
                          <DoubleAnimation Duration="00:00:00.083" To="0" Storyboard.TargetProperty="Opacity" Storyboard.TargetName="ThumbVisual" />
                        </Storyboard>
                      </VisualState>
                    </VisualStateGroup>
                  </VisualStateManager.VisualStateGroups>
                </Rectangle>
              </ControlTemplate>
              <ControlTemplate x:Key="HorizontalThumbTemplate" TargetType="Thumb">
                <Rectangle x:Name="ThumbVisual" Fill="{TemplateBinding Background}" Stroke="{TemplateBinding BorderBrush}" StrokeThickness="6" RadiusX="{Binding CornerRadius, RelativeSource={RelativeSource TemplatedParent}, Converter={StaticResource TopLeftCornerRadiusDoubleValueConverter}}" RadiusY="{Binding CornerRadius, RelativeSource={RelativeSource TemplatedParent}, Converter={StaticResource BottomRightCornerRadiusDoubleValueConverter}}">
                  <VisualStateManager.VisualStateGroups>
                    <VisualStateGroup x:Name="CommonStates">
                      <VisualState x:Name="Normal" />
                      <VisualState x:Name="Disabled">
                        <Storyboard>
                          <ObjectAnimationUsingKeyFrames Storyboard.TargetName="ThumbVisual" Storyboard.TargetProperty="Fill">
                            <DiscreteObjectKeyFrame KeyTime="00:00:00.083" Value="{ThemeResource ControlStrongFillColorDisabledBrush}" />
                          </ObjectAnimationUsingKeyFrames>
                          <DoubleAnimation Duration="00:00:00.083" To="0" Storyboard.TargetProperty="Opacity" Storyboard.TargetName="ThumbVisual" />
                        </Storyboard>
                      </VisualState>
                    </VisualStateGroup>
                  </VisualStateManager.VisualStateGroups>
                </Rectangle>
              </ControlTemplate>
            </Grid.Resources>
            <VisualStateManager.VisualStateGroups>
              <VisualStateGroup x:Name="CommonStates">
                <VisualState x:Name="Normal" />
                <VisualState x:Name="Disabled">
                  <VisualState.Setters>
                    <Setter Target="Root.Background" Value="{ThemeResource SubtleFillColorTransparentBrush}" />
                    <Setter Target="Root.BorderBrush" Value="{ThemeResource SubtleFillColorTransparentBrush}" />
                    <Setter Target="Root.Opacity" Value="0.5" />
                    <Setter Target="HorizontalTrackRect.Stroke" Value="{ThemeResource AcrylicInAppFillColorDefaultBrush}" />
                    <Setter Target="VerticalTrackRect.Stroke" Value="{ThemeResource AcrylicInAppFillColorDefaultBrush}" />
                    <Setter Target="HorizontalTrackRect.Fill" Value="{ThemeResource AcrylicInAppFillColorDefaultBrush}" />
                    <Setter Target="VerticalTrackRect.Fill" Value="{ThemeResource AcrylicInAppFillColorDefaultBrush}" />
                    <Setter Target="HorizontalPanningThumb.Background" Value="{ThemeResource ControlStrongFillColorDisabledBrush}" />
                    <Setter Target="VerticalPanningThumb.Background" Value="{ThemeResource ControlStrongFillColorDisabledBrush}" />
                  </VisualState.Setters>
                </VisualState>
              </VisualStateGroup>
              <VisualStateGroup x:Name="ScrollingIndicatorStates">
                <VisualState x:Name="TouchIndicator">
                  <VisualState.Setters>
                    <Setter Target="HorizontalRoot.Visibility" Value="Collapsed" />
                    <Setter Target="VerticalRoot.Visibility" Value="Collapsed" />
                    <Setter Target="HorizontalPanningRoot.Opacity" Value="1" />
                    <Setter Target="VerticalPanningRoot.Opacity" Value="1" />
                  </VisualState.Setters>
                </VisualState>
                <VisualState x:Name="MouseIndicator">
                  <VisualState.Setters>
                    <Setter Target="HorizontalPanningRoot.Visibility" Value="Collapsed" />
                    <Setter Target="VerticalPanningRoot.Visibility" Value="Collapsed" />
                    <Setter Target="HorizontalRoot.IsHitTestVisible" Value="True" />
                    <Setter Target="VerticalRoot.IsHitTestVisible" Value="True" />
                    <Setter Target="HorizontalThumb.Opacity" Value="1" />
                    <Setter Target="VerticalThumb.Opacity" Value="1" />
                  </VisualState.Setters>
                </VisualState>
                <VisualState x:Name="NoIndicator">
                  <VisualState.Setters>
                    <Setter Target="HorizontalThumb.Background" Value="{ThemeResource ControlStrongFillColorDefaultBrush}" />
                    <Setter Target="VerticalThumb.Background" Value="{ThemeResource ControlStrongFillColorDefaultBrush}" />
                  </VisualState.Setters>
                  <Storyboard>
                    <DoubleAnimation Duration="00:00:00.083" Storyboard.TargetName="VerticalSmallIncrease" Storyboard.TargetProperty="Opacity" To="0" />
                    <DoubleAnimation Duration="00:00:00.083" Storyboard.TargetName="VerticalLargeIncrease" Storyboard.TargetProperty="Opacity" To="0" />
                    <DoubleAnimation Duration="00:00:00.083" Storyboard.TargetName="VerticalLargeDecrease" Storyboard.TargetProperty="Opacity" To="0" />
                    <DoubleAnimation Duration="00:00:00.083" Storyboard.TargetName="VerticalThumb" Storyboard.TargetProperty="Opacity" To="0" />
                    <DoubleAnimation Duration="00:00:00.083" Storyboard.TargetName="VerticalSmallDecrease" Storyboard.TargetProperty="Opacity" To="0" />
                    <DoubleAnimation Duration="00:00:00.083" Storyboard.TargetName="VerticalTrackRect" Storyboard.TargetProperty="Opacity" To="0" />
                    <DoubleAnimation Duration="00:00:00.083" Storyboard.TargetName="HorizontalSmallIncrease" Storyboard.TargetProperty="Opacity" To="0" />
                    <DoubleAnimation Duration="00:00:00.083" Storyboard.TargetName="HorizontalLargeIncrease" Storyboard.TargetProperty="Opacity" To="0" />
                    <DoubleAnimation Duration="00:00:00.083" Storyboard.TargetName="HorizontalLargeDecrease" Storyboard.TargetProperty="Opacity" To="0" />
                    <DoubleAnimation Duration="00:00:00.083" Storyboard.TargetName="HorizontalThumb" Storyboard.TargetProperty="Opacity" To="0" />
                    <DoubleAnimation Duration="00:00:00.083" Storyboard.TargetName="HorizontalSmallDecrease" Storyboard.TargetProperty="Opacity" To="0" />
                    <DoubleAnimation Duration="00:00:00.083" Storyboard.TargetName="HorizontalTrackRect" Storyboard.TargetProperty="Opacity" To="0" />
                    <DoubleAnimationUsingKeyFrames Storyboard.TargetName="VerticalThumb" Storyboard.TargetProperty="Width" EnableDependentAnimation="True">
                      <SplineDoubleKeyFrame KeyTime="00:00:00.167" Value="8" KeySpline="0,0,0,1" />
                    </DoubleAnimationUsingKeyFrames>
                    <DoubleAnimationUsingKeyFrames Storyboard.TargetName="VerticalThumbTransform" Storyboard.TargetProperty="X" EnableDependentAnimation="True">
                      <SplineDoubleKeyFrame KeyTime="00:00:00.167" Value="2" KeySpline="0,0,0,1" />
                    </DoubleAnimationUsingKeyFrames>
                    <DoubleAnimationUsingKeyFrames Storyboard.TargetName="HorizontalThumb" Storyboard.TargetProperty="Height" EnableDependentAnimation="True">
                      <SplineDoubleKeyFrame KeyTime="00:00:00.167" Value="8" KeySpline="0,0,0,1" />
                    </DoubleAnimationUsingKeyFrames>
                    <DoubleAnimationUsingKeyFrames Storyboard.TargetName="HorizontalThumbTransform" Storyboard.TargetProperty="Y" EnableDependentAnimation="True">
                      <SplineDoubleKeyFrame KeyTime="00:00:00.167" Value="2" KeySpline="0,0,0,1" />
                    </DoubleAnimationUsingKeyFrames>
                    <ObjectAnimationUsingKeyFrames Storyboard.TargetName="HorizontalRoot" Storyboard.TargetProperty="Visibility">
                      <DiscreteObjectKeyFrame KeyTime="00:00:00.083">
                        <DiscreteObjectKeyFrame.Value>
                          <Visibility>Collapsed</Visibility>
                        </DiscreteObjectKeyFrame.Value>
                      </DiscreteObjectKeyFrame>
                    </ObjectAnimationUsingKeyFrames>
                    <ObjectAnimationUsingKeyFrames Storyboard.TargetName="VerticalRoot" Storyboard.TargetProperty="Visibility">
                      <DiscreteObjectKeyFrame KeyTime="00:00:00.083">
                        <DiscreteObjectKeyFrame.Value>
                          <Visibility>Collapsed</Visibility>
                        </DiscreteObjectKeyFrame.Value>
                      </DiscreteObjectKeyFrame>
                    </ObjectAnimationUsingKeyFrames>
                    <DoubleAnimation Duration="00:00:00.083" Storyboard.TargetName="HorizontalPanningRoot" Storyboard.TargetProperty="Opacity" To="0" />
                    <DoubleAnimation Duration="00:00:00.083" Storyboard.TargetName="VerticalPanningRoot" Storyboard.TargetProperty="Opacity" To="0" />
                    <ObjectAnimationUsingKeyFrames Storyboard.TargetName="HorizontalPanningRoot" Storyboard.TargetProperty="Visibility">
                      <DiscreteObjectKeyFrame KeyTime="00:00:00.167">
                        <DiscreteObjectKeyFrame.Value>
                          <Visibility>Collapsed</Visibility>
                        </DiscreteObjectKeyFrame.Value>
                      </DiscreteObjectKeyFrame>
                    </ObjectAnimationUsingKeyFrames>
                    <ObjectAnimationUsingKeyFrames Storyboard.TargetName="VerticalPanningRoot" Storyboard.TargetProperty="Visibility">
                      <DiscreteObjectKeyFrame KeyTime="00:00:00.167">
                        <DiscreteObjectKeyFrame.Value>
                          <Visibility>Collapsed</Visibility>
                        </DiscreteObjectKeyFrame.Value>
                      </DiscreteObjectKeyFrame>
                    </ObjectAnimationUsingKeyFrames>
                  </Storyboard>
                </VisualState>
              </VisualStateGroup>
              <VisualStateGroup x:Name="ConsciousStates">
                <VisualStateGroup.Transitions>
                  <VisualTransition From="Expanded" To="Collapsed">
                    <Storyboard>
                      <DoubleAnimation Duration="00:00:00.083" BeginTime="00:00:00" Storyboard.TargetName="VerticalSmallIncrease" Storyboard.TargetProperty="Opacity" To="0" />
                      <DoubleAnimation Duration="00:00:00.083" BeginTime="00:00:00" Storyboard.TargetName="VerticalLargeIncrease" Storyboard.TargetProperty="Opacity" To="0" />
                      <DoubleAnimation Duration="00:00:00.083" BeginTime="00:00:00" Storyboard.TargetName="VerticalLargeDecrease" Storyboard.TargetProperty="Opacity" To="0" />
                      <DoubleAnimation Duration="00:00:00.083" BeginTime="00:00:00" Storyboard.TargetName="VerticalSmallDecrease" Storyboard.TargetProperty="Opacity" To="0" />
                      <DoubleAnimation Duration="00:00:00.083" BeginTime="00:00:00" Storyboard.TargetName="VerticalTrackRect" Storyboard.TargetProperty="Opacity" To="0" />
                      <DoubleAnimation Duration="00:00:00.083" BeginTime="00:00:00" Storyboard.TargetName="HorizontalSmallIncrease" Storyboard.TargetProperty="Opacity" To="0" />
                      <DoubleAnimation Duration="00:00:00.083" BeginTime="00:00:00" Storyboard.TargetName="HorizontalLargeIncrease" Storyboard.TargetProperty="Opacity" To="0" />
                      <DoubleAnimation Duration="00:00:00.083" BeginTime="00:00:00" Storyboard.TargetName="HorizontalLargeDecrease" Storyboard.TargetProperty="Opacity" To="0" />
                      <DoubleAnimation Duration="00:00:00.083" BeginTime="00:00:00" Storyboard.TargetName="HorizontalSmallDecrease" Storyboard.TargetProperty="Opacity" To="0" />
                      <DoubleAnimation Duration="00:00:00.083" BeginTime="00:00:00" Storyboard.TargetName="HorizontalTrackRect" Storyboard.TargetProperty="Opacity" To="0" />
                      <DoubleAnimationUsingKeyFrames Storyboard.TargetName="VerticalThumb" Storyboard.TargetProperty="Width" BeginTime="00:00:00" EnableDependentAnimation="True">
                        <SplineDoubleKeyFrame KeyTime="00:00:00.167" Value="8" KeySpline="0,0,0,1" />
                      </DoubleAnimationUsingKeyFrames>
                      <DoubleAnimationUsingKeyFrames Storyboard.TargetName="VerticalThumbTransform" Storyboard.TargetProperty="X" BeginTime="00:00:00">
                        <SplineDoubleKeyFrame KeyTime="00:00:00.167" Value="2" KeySpline="0,0,0,1" />
                      </DoubleAnimationUsingKeyFrames>
                      <DoubleAnimationUsingKeyFrames Storyboard.TargetName="HorizontalThumb" Storyboard.TargetProperty="Height" BeginTime="00:00:00" EnableDependentAnimation="True">
                        <SplineDoubleKeyFrame KeyTime="00:00:00.167" Value="8" KeySpline="0,0,0,1" />
                      </DoubleAnimationUsingKeyFrames>
                      <DoubleAnimationUsingKeyFrames Storyboard.TargetName="HorizontalThumbTransform" Storyboard.TargetProperty="Y" BeginTime="00:00:00">
                        <SplineDoubleKeyFrame KeyTime="00:00:00.167" Value="2" KeySpline="0,0,0,1" />
                      </DoubleAnimationUsingKeyFrames>
                    </Storyboard>
                  </VisualTransition>
                </VisualStateGroup.Transitions>
                <VisualState x:Name="Collapsed">
                  <VisualState.Setters>
                    <Setter Target="HorizontalThumb.Background" Value="{ThemeResource ControlStrongFillColorDefaultBrush}" />
                    <Setter Target="VerticalThumb.Background" Value="{ThemeResource ControlStrongFillColorDefaultBrush}" />
                  </VisualState.Setters>
                </VisualState>
                <VisualState x:Name="Expanded">
                  <VisualState.Setters>
                    <Setter Target="Root.Background" Value="{ThemeResource SubtleFillColorTransparentBrush}" />
                    <Setter Target="Root.BorderBrush" Value="{ThemeResource SubtleFillColorTransparentBrush}" />
                    <Setter Target="HorizontalTrackRect.Stroke" Value="{ThemeResource AcrylicInAppFillColorDefaultBrush}" />
                    <Setter Target="VerticalTrackRect.Stroke" Value="{ThemeResource AcrylicInAppFillColorDefaultBrush}" />
                    <Setter Target="HorizontalTrackRect.Fill" Value="{ThemeResource AcrylicInAppFillColorDefaultBrush}" />
                    <Setter Target="VerticalTrackRect.Fill" Value="{ThemeResource AcrylicInAppFillColorDefaultBrush}" />
                    <Setter Target="HorizontalThumb.Background" Value="{ThemeResource ControlStrongFillColorDefaultBrush}" />
                    <Setter Target="VerticalThumb.Background" Value="{ThemeResource ControlStrongFillColorDefaultBrush}" />
                  </VisualState.Setters>
                  <Storyboard>
                    <DoubleAnimation Duration="00:00:00.083" BeginTime="00:00:00" Storyboard.TargetName="VerticalSmallIncrease" Storyboard.TargetProperty="Opacity" To="1" />
                    <DoubleAnimation Duration="00:00:00.083" BeginTime="00:00:00" Storyboard.TargetName="VerticalLargeIncrease" Storyboard.TargetProperty="Opacity" To="1" />
                    <DoubleAnimation Duration="00:00:00.083" BeginTime="00:00:00" Storyboard.TargetName="VerticalLargeDecrease" Storyboard.TargetProperty="Opacity" To="1" />
                    <DoubleAnimation Duration="00:00:00.083" BeginTime="00:00:00" Storyboard.TargetName="VerticalSmallDecrease" Storyboard.TargetProperty="Opacity" To="1" />
                    <DoubleAnimation Duration="00:00:00.083" BeginTime="00:00:00" Storyboard.TargetName="VerticalTrackRect" Storyboard.TargetProperty="Opacity" To="1" />
                    <DoubleAnimation Duration="00:00:00.083" BeginTime="00:00:00" Storyboard.TargetName="HorizontalSmallIncrease" Storyboard.TargetProperty="Opacity" To="1" />
                    <DoubleAnimation Duration="00:00:00.083" BeginTime="00:00:00" Storyboard.TargetName="HorizontalLargeIncrease" Storyboard.TargetProperty="Opacity" To="1" />
                    <DoubleAnimation Duration="00:00:00.083" BeginTime="00:00:00" Storyboard.TargetName="HorizontalLargeDecrease" Storyboard.TargetProperty="Opacity" To="1" />
                    <DoubleAnimation Duration="00:00:00.083" BeginTime="00:00:00" Storyboard.TargetName="HorizontalSmallDecrease" Storyboard.TargetProperty="Opacity" To="1" />
                    <DoubleAnimation Duration="00:00:00.083" BeginTime="00:00:00" Storyboard.TargetName="HorizontalTrackRect" Storyboard.TargetProperty="Opacity" To="1" />
                    <!-- Because of the blurriness caused by SCALE animation performed on the object with rounded corners, we have to use dependent animation on width to rerasterize the mask on every tick of the animation.-->
                    <DoubleAnimationUsingKeyFrames Storyboard.TargetName="VerticalThumb" Storyboard.TargetProperty="Width" BeginTime="00:00:00" EnableDependentAnimation="True">
                      <SplineDoubleKeyFrame KeyTime="00:00:00.167" Value="12" KeySpline="0,0,0,1" />
                    </DoubleAnimationUsingKeyFrames>
                    <DoubleAnimationUsingKeyFrames Storyboard.TargetName="VerticalThumbTransform" Storyboard.TargetProperty="X" BeginTime="00:00:00">
                      <SplineDoubleKeyFrame KeyTime="00:00:00.167" Value="0" KeySpline="0,0,0,1" />
                    </DoubleAnimationUsingKeyFrames>
                    <DoubleAnimationUsingKeyFrames Storyboard.TargetName="HorizontalThumb" Storyboard.TargetProperty="Height" BeginTime="00:00:00" EnableDependentAnimation="True">
                      <SplineDoubleKeyFrame KeyTime="00:00:00.167" Value="12" KeySpline="0,0,0,1" />
                    </DoubleAnimationUsingKeyFrames>
                    <DoubleAnimationUsingKeyFrames Storyboard.TargetName="HorizontalThumbTransform" Storyboard.TargetProperty="Y" BeginTime="00:00:00">
                      <SplineDoubleKeyFrame KeyTime="00:00:00.167" Value="0" KeySpline="0,0,0,1" />
                    </DoubleAnimationUsingKeyFrames>
                  </Storyboard>
                </VisualState>
                <VisualState x:Name="ExpandedWithoutAnimation">
                  <VisualState.Setters>
                    <Setter Target="Root.Background" Value="{ThemeResource SubtleFillColorTransparentBrush}" />
                    <Setter Target="Root.BorderBrush" Value="{ThemeResource SubtleFillColorTransparentBrush}" />
                    <Setter Target="HorizontalTrackRect.Stroke" Value="{ThemeResource AcrylicInAppFillColorDefaultBrush}" />
                    <Setter Target="VerticalTrackRect.Stroke" Value="{ThemeResource AcrylicInAppFillColorDefaultBrush}" />
                    <Setter Target="HorizontalTrackRect.Fill" Value="{ThemeResource AcrylicInAppFillColorDefaultBrush}" />
                    <Setter Target="VerticalTrackRect.Fill" Value="{ThemeResource AcrylicInAppFillColorDefaultBrush}" />
                    <Setter Target="HorizontalThumb.Background" Value="{ThemeResource ControlStrongFillColorDefaultBrush}" />
                    <Setter Target="VerticalThumb.Background" Value="{ThemeResource ControlStrongFillColorDefaultBrush}" />
                  </VisualState.Setters>
                  <!-- The storyboard below cannot be moved to a transition since transitions
                                             will not be run by the framework when animations are disabled in the system -->
                  <Storyboard>
                    <DoubleAnimation Duration="0" BeginTime="00:00:00" Storyboard.TargetName="VerticalSmallIncrease" Storyboard.TargetProperty="Opacity" To="1" />
                    <DoubleAnimation Duration="0" BeginTime="00:00:00" Storyboard.TargetName="VerticalLargeIncrease" Storyboard.TargetProperty="Opacity" To="1" />
                    <DoubleAnimation Duration="0" BeginTime="00:00:00" Storyboard.TargetName="VerticalLargeDecrease" Storyboard.TargetProperty="Opacity" To="1" />
                    <DoubleAnimation Duration="0" BeginTime="00:00:00" Storyboard.TargetName="VerticalSmallDecrease" Storyboard.TargetProperty="Opacity" To="1" />
                    <DoubleAnimation Duration="0" BeginTime="00:00:00" Storyboard.TargetName="VerticalTrackRect" Storyboard.TargetProperty="Opacity" To="1" />
                    <DoubleAnimation Duration="0" BeginTime="00:00:00" Storyboard.TargetName="HorizontalSmallIncrease" Storyboard.TargetProperty="Opacity" To="1" />
                    <DoubleAnimation Duration="0" BeginTime="00:00:00" Storyboard.TargetName="HorizontalLargeIncrease" Storyboard.TargetProperty="Opacity" To="1" />
                    <DoubleAnimation Duration="0" BeginTime="00:00:00" Storyboard.TargetName="HorizontalLargeDecrease" Storyboard.TargetProperty="Opacity" To="1" />
                    <DoubleAnimation Duration="0" BeginTime="00:00:00" Storyboard.TargetName="HorizontalSmallDecrease" Storyboard.TargetProperty="Opacity" To="1" />
                    <DoubleAnimation Duration="0" BeginTime="00:00:00" Storyboard.TargetName="HorizontalTrackRect" Storyboard.TargetProperty="Opacity" To="1" />
                    <DoubleAnimationUsingKeyFrames Storyboard.TargetName="VerticalThumb" Storyboard.TargetProperty="Width" BeginTime="00:00:00" EnableDependentAnimation="True">
                      <SplineDoubleKeyFrame KeyTime="0" Value="12" KeySpline="0,0,0,1" />
                    </DoubleAnimationUsingKeyFrames>
                    <DoubleAnimationUsingKeyFrames Storyboard.TargetName="VerticalThumbTransform" Storyboard.TargetProperty="X" BeginTime="00:00:00">
                      <SplineDoubleKeyFrame KeyTime="0" Value="0" KeySpline="0,0,0,1" />
                    </DoubleAnimationUsingKeyFrames>
                    <DoubleAnimationUsingKeyFrames Storyboard.TargetName="HorizontalThumb" Storyboard.TargetProperty="Height" BeginTime="00:00:00" EnableDependentAnimation="True">
                      <SplineDoubleKeyFrame KeyTime="0" Value="12" KeySpline="0,0,0,1" />
                    </DoubleAnimationUsingKeyFrames>
                    <DoubleAnimationUsingKeyFrames Storyboard.TargetName="HorizontalThumbTransform" Storyboard.TargetProperty="Y" BeginTime="00:00:00">
                      <SplineDoubleKeyFrame KeyTime="0" Value="0" KeySpline="0,0,0,1" />
                    </DoubleAnimationUsingKeyFrames>
                  </Storyboard>
                </VisualState>
                <VisualState x:Name="CollapsedWithoutAnimation">
                  <VisualState.Setters>
                    <Setter Target="HorizontalThumb.Background" Value="{ThemeResource ControlStrongFillColorDefaultBrush}" />
                    <Setter Target="VerticalThumb.Background" Value="{ThemeResource ControlStrongFillColorDefaultBrush}" />
                  </VisualState.Setters>
                  <!-- The storyboard below cannot be moved to a transition since transitions
                                             will not be run by the framework when animations are disabled in the system -->
                  <Storyboard>
                    <DoubleAnimation Duration="0" BeginTime="00:00:00" Storyboard.TargetName="VerticalSmallIncrease" Storyboard.TargetProperty="Opacity" To="0" />
                    <DoubleAnimation Duration="0" BeginTime="00:00:00" Storyboard.TargetName="VerticalLargeIncrease" Storyboard.TargetProperty="Opacity" To="0" />
                    <DoubleAnimation Duration="0" BeginTime="00:00:00" Storyboard.TargetName="VerticalLargeDecrease" Storyboard.TargetProperty="Opacity" To="0" />
                    <DoubleAnimation Duration="0" BeginTime="00:00:00" Storyboard.TargetName="VerticalSmallDecrease" Storyboard.TargetProperty="Opacity" To="0" />
                    <DoubleAnimation Duration="0" BeginTime="00:00:00" Storyboard.TargetName="VerticalTrackRect" Storyboard.TargetProperty="Opacity" To="0" />
                    <DoubleAnimation Duration="0" BeginTime="00:00:00" Storyboard.TargetName="HorizontalSmallIncrease" Storyboard.TargetProperty="Opacity" To="0" />
                    <DoubleAnimation Duration="0" BeginTime="00:00:00" Storyboard.TargetName="HorizontalLargeIncrease" Storyboard.TargetProperty="Opacity" To="0" />
                    <DoubleAnimation Duration="0" BeginTime="00:00:00" Storyboard.TargetName="HorizontalLargeDecrease" Storyboard.TargetProperty="Opacity" To="0" />
                    <DoubleAnimation Duration="0" BeginTime="00:00:00" Storyboard.TargetName="HorizontalSmallDecrease" Storyboard.TargetProperty="Opacity" To="0" />
                    <DoubleAnimation Duration="0" BeginTime="00:00:00" Storyboard.TargetName="HorizontalTrackRect" Storyboard.TargetProperty="Opacity" To="0" />
                    <DoubleAnimationUsingKeyFrames Storyboard.TargetName="VerticalThumb" Storyboard.TargetProperty="Width" BeginTime="00:00:00" EnableDependentAnimation="True">
                      <SplineDoubleKeyFrame KeyTime="0" Value="8" KeySpline="0,0,0,1" />
                    </DoubleAnimationUsingKeyFrames>
                    <DoubleAnimationUsingKeyFrames Storyboard.TargetName="VerticalThumbTransform" Storyboard.TargetProperty="X" BeginTime="00:00:00">
                      <SplineDoubleKeyFrame KeyTime="0" Value="2" KeySpline="0,0,0,1" />
                    </DoubleAnimationUsingKeyFrames>
                    <DoubleAnimationUsingKeyFrames Storyboard.TargetName="HorizontalThumb" Storyboard.TargetProperty="Height" BeginTime="00:00:00" EnableDependentAnimation="True">
                      <SplineDoubleKeyFrame KeyTime="0" Value="8" KeySpline="0,0,0,1" />
                    </DoubleAnimationUsingKeyFrames>
                    <DoubleAnimationUsingKeyFrames Storyboard.TargetName="HorizontalThumbTransform" Storyboard.TargetProperty="Y" BeginTime="00:00:00">
                      <SplineDoubleKeyFrame KeyTime="0" Value="2" KeySpline="0,0,0,1" />
                    </DoubleAnimationUsingKeyFrames>
                  </Storyboard>
                </VisualState>
              </VisualStateGroup>
            </VisualStateManager.VisualStateGroups>
            <Grid x:Name="HorizontalRoot" Background="{TemplateBinding Background}" BorderBrush="{TemplateBinding BorderBrush}" IsHitTestVisible="False" CornerRadius="{TemplateBinding CornerRadius}">
              <Grid.ColumnDefinitions>
                <ColumnDefinition Width="Auto" />
                <ColumnDefinition Width="Auto" />
                <ColumnDefinition Width="Auto" />
                <ColumnDefinition Width="*" />
                <ColumnDefinition Width="Auto" />
              </Grid.ColumnDefinitions>
              <Rectangle x:Name="HorizontalTrackRect" RadiusX="{Binding CornerRadius, RelativeSource={RelativeSource TemplatedParent}, Converter={StaticResource TopLeftCornerRadiusDoubleValueConverter2x}}" RadiusY="{Binding CornerRadius, RelativeSource={RelativeSource TemplatedParent}, Converter={StaticResource BottomRightCornerRadiusDoubleValueConverter2x}}" Opacity="0" Grid.ColumnSpan="5" Margin="0" StrokeThickness="0" Fill="{ThemeResource AcrylicInAppFillColorDefaultBrush}" Stroke="{ThemeResource AcrylicInAppFillColorDefaultBrush}" />
              <RepeatButton x:Name="HorizontalSmallDecrease" Grid.Column="0" Opacity="0" MinHeight="12" IsTabStop="False" Interval="50" Padding="4,0,0,0" Template="{StaticResource HorizontalDecrementTemplate}" Width="12" AllowFocusOnInteraction="False" VerticalAlignment="Center" />
              <RepeatButton x:Name="HorizontalLargeDecrease" Opacity="0" Grid.Column="1" HorizontalAlignment="Stretch" VerticalAlignment="Stretch" IsTabStop="False" Interval="50" Template="{StaticResource RepeatButtonTemplate}" Width="0" AllowFocusOnInteraction="False" />
              <Thumb x:Name="HorizontalThumb" Opacity="0" Grid.Column="2" Background="{ThemeResource ControlStrongFillColorDefaultBrush}" BorderBrush="{ThemeResource ControlFillColorTransparentBrush}" Template="{StaticResource HorizontalThumbTemplate}" Height="8" MinWidth="30" AutomationProperties.AccessibilityView="Raw" RenderTransformOrigin="0.5,1" CornerRadius="{TemplateBinding CornerRadius}">
                <Thumb.RenderTransform>
                  <TranslateTransform x:Name="HorizontalThumbTransform" Y="2" />
                </Thumb.RenderTransform>
              </Thumb>
              <RepeatButton x:Name="HorizontalLargeIncrease" Opacity="0" Grid.Column="3" HorizontalAlignment="Stretch" VerticalAlignment="Stretch" IsTabStop="False" Interval="50" AllowFocusOnInteraction="False" Template="{StaticResource RepeatButtonTemplate}" />
              <RepeatButton x:Name="HorizontalSmallIncrease" Grid.Column="4" Opacity="0" MinHeight="12" IsTabStop="False" Interval="50" Padding="0,0,4,0" Template="{StaticResource HorizontalIncrementTemplate}" Width="12" AllowFocusOnInteraction="False" VerticalAlignment="Center" />
            </Grid>
            <Grid x:Name="HorizontalPanningRoot" MinWidth="24" Visibility="Collapsed" Opacity="0" CornerRadius="{TemplateBinding CornerRadius}">
              <Border x:Name="HorizontalPanningThumb" VerticalAlignment="Bottom" HorizontalAlignment="Left" Background="{ThemeResource ControlStrongFillColorDefaultBrush}" BorderThickness="0" Height="2" MinWidth="32" Margin="0,2,0,2" />
            </Grid>
            <Grid x:Name="VerticalRoot" Background="{TemplateBinding Background}" BorderBrush="{TemplateBinding BorderBrush}" IsHitTestVisible="False">
              <Grid.RowDefinitions>
                <RowDefinition Height="Auto" />
                <RowDefinition Height="Auto" />
                <RowDefinition Height="Auto" />
                <RowDefinition Height="*" />
                <RowDefinition Height="Auto" />
              </Grid.RowDefinitions>
              <Rectangle x:Name="VerticalTrackRect" RadiusX="{Binding CornerRadius, RelativeSource={RelativeSource TemplatedParent}, Converter={StaticResource TopLeftCornerRadiusDoubleValueConverter2x}}" RadiusY="{Binding CornerRadius, RelativeSource={RelativeSource TemplatedParent}, Converter={StaticResource BottomRightCornerRadiusDoubleValueConverter2x}}" Opacity="0" Grid.RowSpan="5" Margin="0" StrokeThickness="0" Fill="{ThemeResource AcrylicInAppFillColorDefaultBrush}" Stroke="{ThemeResource AcrylicInAppFillColorDefaultBrush}" />
              <RepeatButton x:Name="VerticalSmallDecrease" Grid.Row="0" Opacity="0" Height="12" MinWidth="12" IsTabStop="False" Interval="50" Padding="0,4,0,0" Template="{StaticResource VerticalDecrementTemplate}" HorizontalAlignment="Center" />
              <RepeatButton x:Name="VerticalLargeDecrease" Opacity="0" HorizontalAlignment="Stretch" VerticalAlignment="Stretch" Height="0" IsTabStop="False" Interval="50" Grid.Row="1" AllowFocusOnInteraction="False" Template="{StaticResource RepeatButtonTemplate}" />
              <Thumb x:Name="VerticalThumb" Opacity="0" Grid.Row="2" Background="{ThemeResource ControlStrongFillColorDefaultBrush}" BorderBrush="{ThemeResource ControlFillColorTransparentBrush}" Template="{StaticResource VerticalThumbTemplate}" Width="8" MinHeight="30" AutomationProperties.AccessibilityView="Raw" RenderTransformOrigin="1,0.5" CornerRadius="{TemplateBinding CornerRadius}">
                <Thumb.RenderTransform>
                  <TranslateTransform x:Name="VerticalThumbTransform" X="2" />
                </Thumb.RenderTransform>
              </Thumb>
              <RepeatButton x:Name="VerticalLargeIncrease" Opacity="0" HorizontalAlignment="Stretch" VerticalAlignment="Stretch" IsTabStop="False" Interval="50" Grid.Row="3" AllowFocusOnInteraction="False" Template="{StaticResource RepeatButtonTemplate}" />
              <RepeatButton x:Name="VerticalSmallIncrease" Grid.Row="4" Opacity="0" Height="12" MinWidth="12" IsTabStop="False" Interval="50" Padding="0,0,0,4" Template="{StaticResource VerticalIncrementTemplate}" HorizontalAlignment="Center" />
            </Grid>
            <Grid x:Name="VerticalPanningRoot" MinHeight="24" Visibility="Collapsed" Opacity="0" CornerRadius="{TemplateBinding CornerRadius}">
              <Border x:Name="VerticalPanningThumb" VerticalAlignment="Top" HorizontalAlignment="Right" Background="{ThemeResource ControlStrongFillColorDefaultBrush}" BorderThickness="0" Width="2" MinHeight="32" Margin="2,0,2,0" />
            </Grid>
          </Grid>
        </ControlTemplate>
      </Setter.Value>
    </Setter>
  </Style>
</ResourceDictionary>"#;

    let value = XamlReader::Load(SCROLL_BAR_STYLE_XAML)?;
    let dictionary: ResourceDictionary = value.cast()?;
    application
        .Resources()?
        .MergedDictionaries()?
        .Append(&dictionary)
}
