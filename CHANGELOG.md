# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html) (before 1.0, a breaking
change or a Bevy bump raises the minor version).

## [0.1.0] - Unreleased

First release, for Bevy 0.19.0. Behaviour only: the crate ships no colours, fonts, sizes, images,
timings or theme.

### Added

- `UiKitPlugin` (fields `scroll`, `slider`, `motion`, copied into the `ScrollConfig`,
  `SliderInputConfig` and `MotionConfig` resources) and the public `UiKitSystems::{Scroll,
  Sliders, Motion, SliderVisuals}` sets.
- Scroll areas: `spawn_scroll_area` + `ScrollAreaLook` / `ScrollAreaParts` (a `[scroller | bar]`
  row with Bevy's `Scrollbar` wired to the scroller); the `Scroller` component (wheel over the
  innermost hovered area, the lone-area rule, horizontal wheel for `overflow.x = Scroll`, clamped
  to the laid-out content); `ScrollKey` + the `ScrollMemory` resource (offset kept across a
  rebuild, a new page and a reopened panel start at the top); `AutoHideScrollbar`;
  `check_scroll_wiring` + `ScrollWiringError` for tests; `max_scroll_offset`.
- Sliders for Bevy's headless `Slider`: `ManagedSlider` (value write-back, clamped, NaN-safe,
  optional exact-grid snapping), `SliderFill` + thumb positioning from the value (horizontal and
  vertical), `SliderChanged`, `StepSlider`, PageUp / PageDown, vertical arrow keys and the gamepad
  D-pad on a focused slider, `SliderHeld`, `SliderReleased` and the `any_slider_held` run
  condition; pure helpers `sanitize_slider_value`, `clamp_to_range`, `slider_fraction`.
- Motion: `PopIn` / `PopOut` (`PopSpec`, `PopOutEnd`), `PressSquash` (`SquashSpec`), `Wobble`
  (`WobbleSpec`), `MotionFinished`, `EasedValue` + `EasedFill` (`Easing`, `EaseSpec`, `FillAxis`);
  real time, a capped frame step and a reduced-motion switch; pure `pop_scale`, `wobble_offset`,
  `ease_toward`.
- Glyph check for tests: `check_ascii`, `check_glyphs`, `check_all_glyphs`, `is_printable_ascii`,
  `GlyphReport`, `GlyphIssue`.
- Feature `serialize`: `Serialize` / `Deserialize` on every parameter and config type.
- Examples `scroll_list`, `sliders`, `motion` and `two_styles`.
