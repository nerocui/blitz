# blitz-shell-winui

A WinUI/WinAppSDK host shell for Blitz that can be consumed from a C# app. It renders into a native window/canvas via wgpu/Vello.

Status: scaffold. WGPU surface creation expects an HWND for now; hooking a WinUI SwapChainPanel will require passing its hosting HWND to Rust.

## Design

- Expose a WinRT component that C# can new up and call methods on (SetHwnd, Resize, RenderOnce, LoadHtml, input APIs).
- Create a wgpu Surface from the provided HWND (via raw-window-handle) and drive anyrender_vello.
- Translate host events (pointer/keyboard) to Blitz DOM events.

## Next steps

- Generate WinRT projection (via windows-rs build macro or MSBuild IDL tool) and wire ABI signatures.
- Implement SwapChainPanel interop to extract HWND (or use Win32 window hosting pattern). 
- Complete keyboard input methods and more pointer events.
