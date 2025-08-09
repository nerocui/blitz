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
