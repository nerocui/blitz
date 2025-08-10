# Blitz WinUI Shell – Progress Notes

Date: 2025-08-09

## Overview
WinUI / WinAppSDK host shell exposing a WinRT component (`Blitz.WinUI.Host`) that renders Blitz content into a `Microsoft.UI.Xaml.Controls.SwapChainPanel`. **Architectural pivot:** initial prototype targeted wgpu/Vello; constraints (no HWND ownership + composition swapchain reuse) made a wgpu surface impractical. We replaced that path with a native Direct2D backend (`anyrender_d2d`) implementing the `anyrender` traits. Rendering now records high‑level paint commands which the D2D backend replays directly into the DXGI swapchain backbuffer obtained from the panel.

## Key Artifacts
- IDL: `idl/Blitz.WinUI.idl` → midlrt → `.winmd` → `src/bindings.rs` (generated; do not edit).
- Runtimeclass: `Blitz.WinUI.Host` implemented in Rust (activation factory + host object).
- Renderer backend: `anyrender_d2d` (rects, paths, layers, transforms, solid/gradient/image brushes, text, placeholder shadows).
- Build script: `build.rs` handles WinMD generation (gracefully skips if `midlrt` not found, advising to use VS DevShell).
- Input bridge: pointer, wheel, keyboard events translated into Blitz DOM events.

## Architectural Pivot Rationale
Attempting to drive the panel through wgpu required creating a surface without a traditional HWND and leveraging the existing composition swapchain. wgpu currently assumes ownership / creation of the swapchain surface, making zero-copy integration brittle. Direct2D (with DirectWrite) can target the swapchain backbuffer directly through DXGI and fits the immediate feature set (vector paths, gradients, text) with lower integration risk. The Vello path remains experimental elsewhere but is decoupled from the WinUI shell.

## Implemented (Shell)
- WinRT activation: `DllGetActivationFactory` returns an activation factory implementing `IHostFactory` + `IActivationFactory`.
- `HostRuntime` implements required methods: `SetPanel(Object)`, `Resize(u32,u32,f32)`, `RenderOnce()`, `LoadHtml(HSTRING)`, `CreateInstance(Object,u32,u32,f32)`.
- SwapChainPanel association: accept a `IInspectable` panel reference (no HWND). (Native interop helper refined during D2D pivot; no reliance on raw window handles.)
- Event forwarding (mouse / wheel / keyboard) into Blitz.

## Implemented (Rendering – anyrender_d2d)
- Command set: rectangles (fill/stroke), arbitrary paths (kurbo → ID2D1PathGeometry), push/pop layer (clip), transforms per command.
- Brushes: solid, linear & radial gradients (sweep approximated via radial + angle fudge), image/bitmap with caching keyed by image hash.
- Text: DirectWrite glyph run rendering (replaced earlier placeholder). Stores glyph indices + per‑glyph advances derived from upstream layout (Parley) preserving shaping.
- Box shadow: placeholder inflated semi-transparent rect (blur effect to come).
- Resource caching: gradient & image brush caches; text format cache.

## Build & Tooling
- Must run checks in a VS 2022 Developer PowerShell (PowerShell 7). Use the VS Code task: “blitz-shell-winui: cargo check (VS DevShell)”.
- `build.rs` attempts `midlrt`; if absent, logs a warning and skips binding regeneration (previous panic removed for smoother CI / contributor onboarding).
- Future improvement: more robust quoting / invocation for midlrt when paths contain spaces (current implementation works in standard VS install layout).

## Current Limitations / Gaps
- Box shadow lacks real gaussian blur (need D2D effect / offscreen surface chain).
- Blend / composite modes from `peniko::BlendMode` not mapped to D2D yet.
- Gradient extend / spread modes (pad, repeat, reflect) not fully implemented.
- Sweep gradient is only approximated.
- Text font selection: single default Segoe UI face; weight/italic/style not yet mapped. Stroke style falls back to fill for glyph runs.
- Device lost / reset handling & cache eviction not implemented.
- No sample WinUI3 C# host app in repo for manual verification.
- `anyrender_vello` still present in workspace (not used by this shell) – eventual cleanup / optional feature flag.

## Next Steps (Prioritized)
1. Proper box shadow blur (`ID2D1Effect::GaussianBlur`).
2. Blend/composite mode mapping for `peniko::BlendMode`.
3. Gradient extend modes (Repeat / Reflect) + accurate Sweep gradient.
4. Font face selection & caching (family, weight, stretch, style) + fallback chain.
5. Glyph stroking (outline extraction) honoring stroke width & joins.
6. Device lost handling (recreate context + invalidate caches).
7. Cache eviction policy (LRU) & diagnostics (memory usage, counts).
8. WinUI3 sample host app (usage & manual testing).
9. Build script hardening + CI validation for WinMD generation path.
10. Feature-gate or slim unused backends (`anyrender_vello*`) for shell builds.
11. High DPI correctness (scale metrics, transforms, pixel snapping heuristics).

## Session Progress Summary (Aug 2025)

Focus: stabilize Direct2D backend integration + diagnostic clarity.

Key achievements (condensed):
- Reliable swapchain rendering after earlier blank-frame regression (lazy D2D init + cached backbuffer bitmap).
- Robust resize path (explicit release of D2D target & cached bitmap, retry on failure, no outstanding ref errors).
- Multi-strategy backbuffer bitmap creation (explicit props → inherit → 96dpi fallback) with logging of E_INVALIDARG failures.
- Runtime verbose logging toggle (`SetVerboseLogging` exposed via WinRT) to eliminate per‑frame overhead in normal runs.
- Command recording diagnostics (pre/post counts, box shadow enumeration) now gated behind verbose mode.
- DirectWrite glyph run path finalized for current scope; obsolete text path logic removed.

## Newly Identified Follow-ups (Snapshot)
- Outline-based glyph stroking.
- Transform stack refinement (avoid baked translations).
- Typography metrics visualization overlay.

---

## Decision Log (Highlights)
- Direct2D pivot (no HWND requirement) retained.
- Command recording abstraction preserved for backend interchangeability.
- Build remains tolerant of missing midlrt (no panic).
- Caching + lazy init preferred over upfront heavy initialization.

## Open Questions
- Best path for accurate sweep gradient (custom pixel shader vs. incremental stops transformation)?
- Whether to unify shadow pipeline with potential future filter effects (blur, drop-shadow variations) via a small effect graph abstraction.
- Strategy for high-DPI scaling and per-monitor awareness (current API passes scale; need dynamic updates on DPI change events).

## Notes
- `src/bindings.rs` is generated – never hand edit.
- Avoid introducing any HWND-based APIs into the shell; panel-only contract.
- Keep renderer backend plug-replaceable (avoid leaking Direct2D types outside backend crate).

## Glossary
- SwapChainPanel: WinUI XAML control hosting a DXGI swapchain for composition.
- midlrt: Microsoft IDL compiler producing .winmd metadata from .idl.
- anyrender: Abstraction layer defining renderer traits consumed by Blitz.
- Direct2D (D2D1): 2D rendering API used for current backend.
- DirectWrite: Text layout & glyph rendering API.

---
End of current progress snapshot.
