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
