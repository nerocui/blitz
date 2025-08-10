# anyrender_d2d

Direct2D backend for the `anyrender` abstraction used by Blitz. It renders directly into an existing IDXGISwapChain1 (e.g. a WinUI `SwapChainPanel` composition swapchain) without requiring an HWND or wgpu.

## Current Capabilities

- Scene recording + playback (rect + arbitrary path fill & stroke)
- Solid, linear & radial (approx sweep) gradients (cached)
- Image drawing (bitmap caching)
- Layers via axis-aligned clip stack
- Affine transforms per command
- Basic text drawing (DirectWrite layout) using a default font
- Approximate box shadow (simple inflated, semi-transparent rect)

## Recent Session Summary (2025-08)

This session focused on stabilizing and extending the Direct2D backend for the WinUI shell pivot:

1. Device & Swapchain: Corrected D3D11 device creation (explicit `D3D11_SDK_VERSION`) and robust DXGI swapchain handling for `SwapChainPanel`.
2. Logging: Redirected internal diagnostics to `OutputDebugString` for visibility in WinDbg / VS Output window.
3. Text Rendering Overhaul: Replaced the earlier naive text path accumulation with true DirectWrite glyph run submission (`IDWriteFontFace::DrawGlyphRun` via `ID2D1DeviceContext::DrawGlyphRun`). Introduced a `GlyphRun` scene command storing glyph indices + per‑glyph advances.
4. Advances Derivation: Derived advances from upstream absolute glyph x positions, preserving shaping decisions made by the layout engine (Parley) instead of reconstructing them heuristically.
5. Multi-line Handling Attempt: Added (then removed) heuristic line splitting inside the backend. We reverted that change to rely solely on upstream layout line iteration so backend remains a dumb recorder for each glyph run line.
6. Style Handling Refactor: Began mapping `StyleRef` into a `GlyphRenderStyle` (fill vs stroke). Stroke currently falls back to fill rendering pending proper outline stroking implementation.
7. Cleanup: Removed legacy `text_format_cache` path and unused code; pruned warnings; consolidated glyph rendering code path.
8. Enum / Command Evolution: Added `GlyphRun { glyph_indices, advances, origin, size, style }` to the command list.

## Completed (This Session)

- DirectWrite integration (basic font face: Segoe UI; glyph indices rendered correctly).
- Per-glyph advance handling (spacing fidelity relative to upstream layout data).
- Command abstraction updated to carry glyph style (fill/stroke placeholder).
- Removed obsolete text format caching and naive text path code.
- Logging & swapchain/device creation robustness improvements.

## Remaining TODO (Short Term)

1. Font Selection & Styles: Map `peniko::Font` / CSS style (weight, italic) to `IDWriteFontFace` instances; cache faces keyed by (family, weight, style, stretch).
2. Stroke Text: Implement outline extraction + stroking (convert glyph run to geometry with `GetGlyphRunOutline`) safely within current windows crate types.
3. Text Decoration: Underline / strikethrough painting inside backend (currently handled upstream by drawing lines; verify alignment with DWrite metrics for high DPI).
4. Centering / Alignment: Ensure upstream layout passes correct translated origins for centered flex container; optionally add a transform command if needed to avoid baking translation into glyph origins.
5. Box Shadow: Replace placeholder inflated rect with Gaussian blur effect pipeline (offscreen bitmap + `ID2D1Effect` GaussianBlur + composite).
6. Blend Modes: Map `peniko::BlendMode` variants to D2D blend/composite (fallback with logging when unsupported).
7. Gradient Extend Modes: Support Repeat / Reflect; accurate Sweep gradient emulation.
8. Resource Lifetime: Add cache eviction (LRU size / count limits) + memory stats.
9. Device Lost Handling: Detect `D2DERR_RECREATE_TARGET` on `EndDraw` and rebuild device context + caches.
10. High DPI: Audit pixel vs DIP usage; scale glyph positions & brush transforms appropriately for non‑96 DPI.

## Future Improvements (Longer Term)

- Font Fallback Chain: Implement multi-font fallback and symbol/emoji coverage via custom font collection.
- Text Shaping Enhancements: Integrate richer shaping (OpenType features, variation axes) beyond basics provided today.
- Effect Graph: Unified abstraction for future blur, shadows, filters (chain of offscreen passes) instead of ad-hoc blur implementation.
- Performance Instrumentation: Optional timing overlays & telemetry hooks (scene encoding, playback, present latency).
- Parallel Recording: Allow multi-thread scene recording segments joined before playback (requires command list segmentation & ordering guarantees).
- Clip Optimization: Combine consecutive identical clips; detect redundant push/pop pairs.
- Optional WIC / Image Scaling Quality Modes: Higher quality bitmap resampling (Fant / Cubic) for large downscales.

## Known Limitations (Reiterated)

- Stroke text not yet visually stroked (renders as fill).
- No real blur for shadows.
- Limited blend/extend mode coverage.
- Single default system font (Segoe UI) regardless of requested family.
- Unbounded caches.

---

Contributions welcome—please keep changes modular and avoid leaking Direct2D types across crate boundaries.

## Future Iterations / TODO

1. True Gaussian blur via `ID2D1Effect` (GaussianBlur) for box shadows
   - Replace placeholder inflated rect with an offscreen target + blur effect.
   - Consider caching blurred rounded-rect masks for common radii.
2. Full blend mode & extend mode mapping
   - Map `peniko::BlendMode { mix, compose }` to Direct2D primitive blend & composite.
   - Respect gradient/image extend (Pad/Repeat/Reflect) instead of always Clamp.
3. Proper glyph shaping & font fallback
   - Create `IDWriteFontFace` from `peniko::Font` data (custom loader) and build glyph runs.
   - Support glyph transforms & subpixel positioning.
4. Workspace slimming: optionally remove `anyrender_vello` from the WinUI shell build
   - Add a cargo feature (e.g. `vello-backend`) or separate workspace profile.
   - Update root `Cargo.toml` membership / dependency gating.
5. Sweep gradient emulation
   - Implement via offscreen radial sweep texture or shader-like approximation using lookup bitmap.
6. Resource lifetime & eviction
   - LRU for gradient and image caches (currently unbounded growth keyed by hash).
7. Device loss handling
   - Detect `D2DERR_RECREATE_TARGET` on `EndDraw` and recreate device/context + caches.
8. Brush alpha & opacity layers
   - Support per-layer alpha and advanced blending via `PushLayer / PopLayer` with `D2D1_LAYER_PARAMETERS1`.
9. High-DPI text metrics
   - Scale layout bounds & baseline properly for non-96 DPI.
10. Performance profiling hooks
    - Optional timing of scene encode, D2D playback, and present.

## Notes

- Gradient sweep currently falls back to a simple linear approximation.
- Text rendering is placeholder-quality; do not rely on it for production typography.
- Box shadow is visually incorrect; flagged for replacement.

Contributions implementing any of the above are welcome—please keep changes modular.

## Verbose Logging

The backend exposes a runtime switch to reduce per-frame logging overhead in production:

```rust
anyrender_d2d::set_verbose_logging(true);  // enable detailed logs
anyrender_d2d::set_verbose_logging(false); // disable (default)
```

Verbose mode adds:
- Scene command pre/post counts
- Successful bitmap creation attempts
- Box shadow command enumeration
- Lazy init success traces

Essential errors (e.g. all bitmap creation attempts failed) always log regardless of the flag.

## Swapchain Backbuffer Bitmap Creation (E_INVALIDARG Diagnostics)

Some drivers reject explicit bitmap property combinations for swapchain surfaces (returning `0x80070057`).

Strategy implemented:
1. Attempt explicit props using context DPI and premultiplied BGRA.
2. If that fails, attempt inherit (`None`).
3. If still failing, attempt explicit 96 DPI fallback.
4. If all fail, frame is skipped and an error is logged.

Future refinement: cache a per-renderer flag to skip explicit attempts after a first failure sequence to avoid repeated failing COM calls.

## Resize Handling

Before `IDXGISwapChain1::ResizeBuffers` we drop the cached `ID2D1Bitmap1` and clear the device context target via `release_backbuffer_resources` to prevent DXGI error 0x887A0001 (outstanding references). A retry is attempted if the first resize fails.
