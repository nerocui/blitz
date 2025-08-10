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

Recent development concentrated on text rendering correctness and backend hygiene:

- Corrected D3D11 device creation signature and swapchain acquisition flow.
- Added debugger logging via `OutputDebugString` wrapper for early device/swapchain diagnostics.
- Introduced `GlyphRun` scene command carrying glyph indices, advances, origin, size, and style.
- Integrated DirectWrite glyph run rendering (removed naive path accumulation text approach).
- Derived glyph advances from upstream layout absolute positions to maintain shaping spacing fidelity.
- Experimented with heuristic multi-line splitting inside backend; reverted to trusting upstream line segmentation (Parley) to avoid backend duplicating layout logic.
- Added style mapping scaffold (`GlyphRenderStyle` fill/stroke); stroke currently renders as fill pending outline support.
- Removed obsolete `text_format_cache` and associated dead code.
- Cleaned warnings and stabilized build after enum evolution.

## Newly Identified Follow-ups

- Implement outline-based stroke text (geometry generation + stroke drawing).
- Introduce transform command or batched transform stack to avoid baking translation into glyph origins for future animations.
- Provide detailed metrics test rendering (baseline, ascent/descent overlays) for debugging typography.

---

## Decision Log (Key Points)
- Pivoted from wgpu/Vello to Direct2D due to lack of HWND + desire to avoid custom swapchain mediation.
- Chose command recording pattern to stay aligned with existing `anyrender` abstraction and allow future backend swapping.
- Graceful build fallback (skip WinMD regen) to reduce contributor friction; rely on committed generated bindings when tools absent.
- Adopted caching early (gradients/images) to avoid recreating D2D resources every frame.

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
