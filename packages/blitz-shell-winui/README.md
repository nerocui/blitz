# blitz-shell-winui

A WinUI/WinAppSDK host shell for Blitz that can be consumed from a C# app. It renders into a WinUI SwapChainPanel via a DXGI swapchain; no HWND is required or used.

## Design

- Expose a WinRT component that C# can new up and call methods on (SetPanel, Resize, RenderOnce, LoadHtml, input APIs).
- Create a DXGI swapchain for composition and attach it to the provided SwapChainPanel. Render content with Vello and upload into the backbuffer each frame.
- Translate host events (pointer/keyboard) to Blitz DOM events.

## Next steps

- Generate WinRT projection (via midlrt + windows-bindgen) and wire ABI signatures.
- Implement SwapChainPanel interop that accepts a panel-attacher and attaches a DXGI swapchain. 
- Complete keyboard input methods and more pointer events.
