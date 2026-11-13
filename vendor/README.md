# Patched Slint crates

Copies of three crates from Slint 1.18.1 (crates.io), used through `[patch.crates-io]` in the
root `Cargo.toml`. They exist only to make `libmain.so` smaller (measured: -3.8 MB and -1.8 MB
stripped, see `reports/mobile-sentinel lighter builds.md`). They go away once Slint has an option
for this (a patch to Slint that makes both features optional is prepared, not yet sent).

| Crate | Change | Effect |
|---|---|---|
| `i-slint-core` | `Cargo.toml`: the `parley` dependency no longer enables `complex-scripts` | -3.8 MB (ICU4X dictionaries for Thai, Lao, Khmer, Burmese, Chinese, Japanese). Text in those scripts breaks lines by rule instead of by dictionary. AlarmFree shows English, Dutch, Spanish, French and German only |
| `i-slint-renderer-skia` | `Cargo.toml`: `"svg"` no longer enabled on `i-slint-core`; new `svg` feature (off); `cached_image.rs`: the `ImageInner::Svg` arm is `#[cfg(feature = "svg")]` (and `image_fit` is allowed to be unused without it) | about -1.8 MB with the next row (resvg, usvg, tiny-skia, svgtypes). `@image-url("x.svg")` stops working |
| `i-slint-backend-android-activity` | `Cargo.toml`: `"svg"` no longer enabled on `i-slint-core` | see above |

AlarmFree uses no `.svg` and no `@image-url`; the only `Image` is the ring screen's photo (a JPEG
the app wrote itself). `image-decoders` stays on.

Compare with the registry source to see the exact edits:
`diff -ru ~/.cargo/registry/src/*/i-slint-core-1.18.1 vendor/i-slint-core`. Unneeded files
(`.cargo-ok`, `Cargo.lock`, `Cargo.toml.orig`) were removed. On a Slint update, redo the edits on
the new version or drop the patch.
