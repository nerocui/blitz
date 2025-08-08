# Blitz WinUI Shell – Progress Notes

Date: 2025-08-08

## Overview
A WinUI/WinAppSDK shell to host Blitz rendering inside a Microsoft.UI.Xaml.Controls.SwapChainPanel, exposed as a WinRT component for C# apps. Rendering is powered by anyrender_vello (wgpu + vello). Input is bridged to Blitz DOM events.

## Key Artifacts
- IDL: `idl/Blitz.WinUI.idl` → built with midlrt → `.winmd` → Rust bindings via windows-bindgen → `src/bindings.rs` (generated; do not edit).
- Runtimeclass: `Blitz.WinUI.Host` implemented in Rust.
- Host object: `winrt_component::BlitzHost` manages the document, renderer, and input.
- Raw handle wrapper: `raw_handle::DxgiInteropHandle` for HWND/RawWindowHandle integration (temporary path; panel interop is the target).
- C ABI (optional): `blitz_winui_*` functions for early P/Invoke testing.

## What’s Implemented
- Crate scaffold `blitz-shell-winui` and workspace wiring.
- IDL-driven WinRT generation (midlrt + windows-bindgen) in build.rs.
- WinRT runtimeclass implementation via windows-rs:
  - `#[implement(IHost, IHostFactory)] pub struct HostRuntime`.
  - Explicit `impl IHost_Impl for HostRuntime_Impl` and `impl IHostFactory_Impl for HostRuntime_Impl`, forwarding to the inner `HostRuntime` using `get_impl()`.
  - Methods: `SetPanel(Object)`, `Resize(u32,u32,f32)`, `RenderOnce()`, `LoadHtml(HSTRING)`, `CreateInstance(Object,u32,u32,f32)`.
- Input bridging from C# (mouse, wheel, keyboard) into Blitz DOM events.
- DllGetActivationFactory: exported as a stub returning `CLASS_E_CLASSNOTAVAILABLE` (activation wiring pending).

## Constraints & Guidelines
- SwapChainPanel is the embedding surface. Do not require a top-level HWND from C#.
- Create the wgpu surface via DXGI/SwapChainPanel interop internally (to be implemented in `BlitzHost::set_panel`).
- `src/bindings.rs` is generated from `idl/Blitz.WinUI.idl`; do not edit it by hand.
- Builds must run from a Visual Studio Developer PowerShell (PowerShell 7) so midlrt/Windows SDK tools are on PATH.

## Build/Check (VS DevShell required)
- Use the VS Code task “blitz-shell-winui: cargo check (VS DevShell)” that opens:
  - PowerShell 7 + VS 2022 DevShell with `Enter-VsDevShell 19c26628 -DevCmdArguments "-arch=x64 -host_arch=x64"`.
  - Executes `cargo check -p blitz-shell-winui` within that shell.

## Current Gaps
- SwapChainPanel native interop (ISwapChainPanelNative) not wired yet; `SetPanel` forwards a raw IInspectable for now.
- Real activation factory not implemented; DllGetActivationFactory is a temporary stub.
- No sample WinUI3 C# app in-repo yet.

## Next Steps
- Implement `BlitzHost::set_panel` using SwapChainPanel native interop to create/recreate the wgpu surface.
- Replace activation stub with a real factory returning `IHostFactory` for `Blitz.WinUI.Host`.
- Add a minimal WinUI3 C# sample that activates `Blitz.WinUI.Host`, calls `CreateInstance(panel, w, h, scale)`, forwards events, and renders.
- Scale/viewport updates on panel DPI/size changes.

## Notes
- Keyboard mapping uses `windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY` to translate VK codes into `keyboard-types` keys.
- Raw HWND path exists only for early testing via P/Invoke; production should rely on SwapChainPanel.
