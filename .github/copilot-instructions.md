```instructions
WINUI SHELL CORE RULES
 - Rendering lives inside a Microsoft.UI.Xaml.Controls.SwapChainPanel. Do NOT require / assume a top-level HWND.
 - Accept only the panel's WinRT object and perform DXGI / SwapChainPanel interop internally.
 - Current backend is Direct2D (anyrender_d2d). Do not reintroduce wgpu/Vello here without an approved design change.

GENERATED CODE
 - src/bindings.rs is generated from idl/Blitz.WinUI.idl (midlrt + windows-bindgen). Never hand-edit.
 - If midlrt isn't available the build script may skip regeneration; committed bindings must remain valid.

BUILD ENVIRONMENT
 - Use the VS 2022 DevShell (PowerShell 7) so midlrt & Windows SDK tools are on PATH. Prefer the provided VS Code task.
 - When adding new WinRT APIs update the IDL, re-run build (with tools present), and commit regenerated bindings.

DIRECT2D BACKEND GUIDELINES
 - Keep backend-specific types encapsulated; expose only anyrender trait implementations outward.
 - Implement new paint features via command recording; avoid injecting Direct2D handles into higher layers.
 - Cache expensive resources (brushes, geometries, bitmaps, text formats) and document eviction strategy when added.

AVOID
 - Introducing HWND-based surfaces or raw-window-handle dependencies.
 - Panicking in build.rs for missing midlrt; prefer warn + skip.
 - Editing generated files or mixing backend-specific code into shell public APIs.

TO DO (HIGH LEVEL REMINDERS)
 - Proper gaussian blur for box shadows.
 - Blend/composite mapping and gradient spread modes.
 - Text shaping & font fallback via DirectWrite custom font collection.
 - Device lost handling & cache eviction policies.
 - WinUI3 sample app for manual verification.
```
