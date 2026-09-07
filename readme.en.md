[![zh](https://img.shields.io/badge/lang-zh-blue.svg)](./readme.md)
[![Platform](https://img.shields.io/badge/Platform-Windows-0078D6?logo=windows)](https://github.com/Hermuc/KeyFlux)
[![License](https://img.shields.io/badge/License-GPL--3.0-blue)](./LICENSE)

# ⌨️ KeyFlux

> A keyboard tool for Windows that helps you get more done without leaving the home row.

## ✨ Features

- 🚀 **Quick app switching** — launch and switch any application with a hotkey
- 🖱️ **Mouse control** — drive the mouse from the keyboard, no more hand traveling
- ⌨️ **Key remapping** — cursor control, digit input and symbol input on the home row

## 📦 Quick start

1. Download and unzip from [Releases](https://github.com/Hermuc/KeyFlux/releases/latest) 📥
2. Run `KeyFlux.exe` ▶️
3. Press <kbd>CapsLock</kbd> + <kbd>S</kbd> + <kbd>E</kbd> to open the settings window ⚙️

## 🖼️ Screenshots

![settings](./doc/settings.en.png)

## 🔀 Differences from upstream

This fork is based on [xianyukang/MyKeymap](https://github.com/xianyukang/MyKeymap). Main differences:

- 🖥️ **Native settings window (Avalonia GUI)** — the old browser-based (Vue) settings page has been fully replaced by a native desktop app (`config-ui-avalonia/`); the GUI launches `settings.exe --headless` as a child process and talks to the Go backend over localhost HTTP, config writing stays in the backend
- ⚡ **Selected action system** — select text or files, then press a hotkey to trigger a preset action; rules are editable visually. Two match types: file extensions (with customizable group quick-fill) and text features (URL / path / magnet link / plain text), strictly paired with dedicated actions
- 🎨 **CommandInput skin** — background, border, gridlines, key colors, window position/width, shadow, hide animation and more, configurable via the `commandInputSkin` field in config.json
- 💊 **"Matrix" digital rain** — running `bin\settings.exe` directly still shows the digital rain in the console (disable via `options.hideMatrix`)
- 🚪 **Tray recall** — bring back apps (e.g. WeChat/QQ) minimized to the tray instantly, without re-launching a new instance
- 🛡️ **More robust** — invalid hotkey configs are skipped with a tip instead of crashing the whole program
- 🧰 **Registry-based autostart + one-click uninstall script**

---

🙌 Upstream: [xianyukang/MyKeymap](https://github.com/xianyukang/MyKeymap) · 📄 License: [GPL-3.0](./LICENSE)
