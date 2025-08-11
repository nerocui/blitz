# anyrender_d2d

Direct2D backend for the `anyrender` abstraction used by Blitz. It renders directly into an existing IDXGISwapChain1 (e.g. a WinUI `SwapChainPanel` composition swapchain) without requiring an HWND or wgpu.

## Current Capabilities (Aug 2025)

- Scene recording + playback (arbitrary path fill & stroke; rectangle fast path intentionally removed to preserve rounded corners)
- Solid / linear / radial (sweep approximated) gradients with caching
- Image drawing with bitmap caching
- Layers via axis‑aligned clip stack (Push/PopLayer)
- Per‑command baked translation transforms
- DirectWrite glyph run submission with per‑glyph advances (primary family + weight selection via DirectWrite font face cache; generic families mapped to system fonts; stroke outline path scaffolding present)
- True Gaussian blur box shadows (outset & inset) via D2D GaussianBlur effect, temporary device contexts (no mid-frame target retarget), rounded corner support, bitmap cache (LRU heuristics WIP)
- Border radius respected for fills, strokes, and shadows
- Runtime‑controllable verbose diagnostics (disabled by default)

## Post‑Mortem: Rendering Failure & Fix (Aug 2025)

### Symptoms
After introducing Gaussian blur shadows (outset + inset) and extensive diagnostics, all regular scene content disappeared: only an opaque fallback background was visible. `EndDraw` began failing with `HRESULT(0x88990001) D2DERR_WRONG_STATE` whenever a full (≈256 command) frame executed. Reducing commands or disabling features changed failure timing, suggesting internal device context state corruption rather than logic errors in scene recording.

### Root Causes
1. Target Mutation During Active Frame: The initial shadow implementation reused the primary `ID2D1DeviceContext` and called `SetTarget` mid‑`BeginDraw` to redirect output to an offscreen bitmap for blur, then restored the original target. Direct2D is sensitive to target switches during an active draw; this pattern intermittently left the context in a wrong state by `EndDraw` (especially with nested clips & effects).
2. Geometry Figure Closure Edge Cases: Some complex paths relied on implicit figure closure. While not the primary trigger, ensuring every opened figure is explicitly ended removed a potential secondary source of state inconsistency.

### Fixes
1. Shadow Refactor: Both outset and inset shadow routines now create a **temporary device context** (`CreateDeviceContext`) dedicated to rasterizing the solid (or ring) mask into an offscreen `ID2D1Bitmap1`. The main context never changes its target after `BeginDraw`.
2. Effect Capture Isolation: Capturing a blurred result for the shadow cache also uses a temporary context instead of temporarily rebinding the main context target.
3. Explicit Geometry Finalization: Path building now tracks whether a figure is open and explicitly `EndFigure` (closed or open) before `sink.Close()`, logging any HRESULT failures.
4. Logging Hygiene: Per‑command logs, clip push/pop, and shadow diagnostics are now gated behind a runtime verbose flag to keep default frames lightweight.

### Outcome
`EndDraw` now returns `S_OK`; all shapes, text, and both inset/outset shadows render correctly with preserved border radii. No regression in performance-critical paths: temporary contexts are short‑lived (per unique uncached shadow) and shadow cache reuse amortizes cost.

### Lessons
- Avoid mutating the primary device context target mid‑frame; prefer auxiliary contexts or command lists for intermediate surfaces.
- Keep diagnostic instrumentation controllable at runtime to prevent masking timing-related issues or adding overhead.
- Always finalize path figures deterministically; defensive closure eliminates a class of subtle state issues.

The codebase has been cleaned so that only necessary logs remain by default; deep diagnostics require opting in (see Verbose Logging section).

## Completed (Recent Session)

- DirectWrite integration (Segoe UI glyph runs; per‑glyph advances preserved).
- True Gaussian blur shadows (outset + inset) with temp device contexts and cache.
- Eliminated mid‑frame primary context `SetTarget` usage (stability improvement).
- Explicit path figure closure & error logging.
- Verbose logging macro (`vlog!`) + pruning of noisy instrumentation.
- Removed obsolete text format caching and naive text path fallback.
- Added ignore rules for generated WinRT artifacts upstream (shell crate).

## Remaining TODO (Short Term)

1. Font style completeness
   - Italic propagation (currently always normal) and stretch mapping into `FontKey`.
   - Multi‑family fallback chain (iterate full CSS family list instead of first only).
   - Per‑glyph fallback for missing codepoints (emoji/symbol coverage, generic `emoji` mapping).
2. Text stroke & outline
   - Finalize glyph outline extraction & stroke rendering path (fallback currently fills).
   - Proper decoration metrics adjustment (avoid descender collisions; thickness scaling by weight).
3. Blend / composite mode mapping (peniko::BlendMode -> `D2D1_PRIMITIVE_BLEND` / composite) with graceful fallbacks.
4. Gradient extend modes (Repeat / Reflect) + improved sweep / true angular gradient implementation.
5. Variable fonts
   - Wire variation axis (`var_coords`) plumbing; map CSS `font-variation-settings` into DirectWrite axis values.
6. Cache policies & stats
   - LRU / size limits for gradient, image, shadow, and font face caches (current caches unbounded).
   - Basic telemetry counters (cache hits/misses, shadow reuse).
7. Device lost handling (`D2DERR_RECREATE_TARGET`) + resource re‑init path & cache repopulation strategy.
8. High DPI audit (DIP vs pixel alignment for glyph baselines, shadow extents, blur padding rounding).
9. Performance / telemetry hooks (encode + playback + blur timings; optional logging channel).
10. Shadow spread / inner spread controls (current: pure Gaussian blur of mask only).
11. Logging polish
    - Replace any remaining ad‑hoc logs with `vlog!` or structured one‑shot diagnostics.
12. Tests
    - Geometry figure closure regression test.
    - Shadow cache reuse test (same rect/radius/std_dev produces single blur computation).
    - Glyph advance preservation & weight/family selection.
13. @font-face / custom font loading (private font collection + memory loader).
14. Fallback heuristics logging (one‑time per (family, weight, style) miss to aid debugging).
15. Optional synthetic emboldening / oblique when requested style not available.
16. Remove or implement variable coord storage (currently `var_coords` unused -> consider wiring once variable fonts active).

## Future Improvements (Longer Term)

- Font Fallback Chain: Multi-font fallback & emoji coverage (custom DirectWrite collection or composite lookup).
- Text Shaping Enhancements: Rich OpenType feature control & variation axis application.
- Effect Graph: Unified abstraction for future blur, shadows, filters (chain of offscreen passes) instead of ad-hoc blur implementation.
- Performance Instrumentation: Optional timing overlays & telemetry hooks (scene encoding, playback, present latency).
- Parallel Recording: Allow multi-thread scene recording segments joined before playback (requires command list segmentation & ordering guarantees).
- Clip Optimization: Combine consecutive identical clips; detect redundant push/pop pairs.
- Optional WIC / Image Scaling Quality Modes: Higher quality bitmap resampling (Fant / Cubic) for large downscales.

## Known Limitations (Current)

- Stroke text not yet visually stroked (renders as fill only).
- Limited blend / extend mode coverage.
- Single‑family selection only (uses first CSS family; no per‑glyph fallback yet).
- Italic & stretch ignored (always normal).
- Variable font axes ignored (var_coords placeholder only).
- Caches (gradient / image / shadow) lack eviction policy.
- Sweep gradient still approximation.
- No explicit shadow spread control (only Gaussian blur sigma).
- No device context lost recovery path yet.

---

Contributions welcome—please keep changes modular and avoid leaking Direct2D types across crate boundaries.

## Future Iterations / TODO

1. (Done) True Gaussian blur via `ID2D1Effect` (GaussianBlur) for box shadows (outset & inset) with temporary device contexts and bitmap cache.
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
- Box shadow implementation now uses real Gaussian blur; further refinement (spread modes, performance tuning) welcome.

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
