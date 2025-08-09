# blitz-shell-winui

WinUI / WinAppSDK host shell for Blitz consumable from C#. Renders into a `Microsoft.UI.Xaml.Controls.SwapChainPanel` using a DXGI swapchain and a Direct2D backend (no HWND dependency).

## Design

- Expose a WinRT component (`Blitz.WinUI.Host`) with methods: `SetPanel`, `CreateInstance`, `Resize`, `RenderOnce`, `LoadHtml`, and input forwarding.
- Create / manage a DXGI swapchain targeted at the provided `SwapChainPanel`; acquire backbuffer for Direct2D drawing.
- Use the `anyrender_d2d` backend to replay recorded Blitz scene commands (paths, gradients, images, text) straight into the swapchain.
- Translate host pointer / keyboard events to Blitz DOM events.

## Implementation Status (Highlights)

- WinRT IDL -> midlrt -> winmd -> Rust bindings (generated `bindings.rs`).
- Activation factory + runtime class implemented in Rust.
- Direct2D renderer integrated (rects, paths, layers, transforms, solid/gradient/image brushes, basic text, placeholder shadows).
- Build script gracefully skips WinMD regeneration if `midlrt` missing (advise using VS DevShell).

## Roadmap (Abbreviated)

- Gaussian blur box shadows via D2D effect.
- Blend/composite mode mapping.
- Gradient spread modes & accurate sweep gradient.
- Proper text shaping (DirectWrite font collection from embedded fonts) & fallback.
- Device lost handling & cache eviction.
- Sample WinUI3 C# application demonstrating usage.

See `docs/PROGRESS.md` and `../anyrender_d2d/README.md` for full details and current gaps.

## Contributing Notes

- Always run cargo checks from a VS 2022 DevShell PowerShell so `midlrt` is on PATH (use provided VS Code task).
- Never edit `src/bindings.rs` by hand.
- Avoid introducing any HWND-based interfaces; keep the contract purely in terms of the `SwapChainPanel` WinRT object.
