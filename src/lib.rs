//! # bevy_ui_kit
//!
//! Style-free UI **behaviour** for [Bevy](https://bevyengine.org) 0.19: the mechanics Bevy UI
//! leaves to you, and no look at all. **Behaviour only - you bring the look.** The crate ships no
//! colours, fonts, sizes, images or theme; every node it spawns or moves gets its appearance from
//! bundles and numbers you pass in.
//!
//! - **Scroll areas** ([`spawn_scroll_area`], [`Scroller`], [`ScrollKey`], [`ScrollMemory`]):
//!   a draggable scrollbar as a flex sibling, the mouse wheel over the hovered area, and the
//!   scroll offset kept when a panel rebuilds.
//! - **Slider wiring** ([`ManagedSlider`], [`SliderFill`], [`SliderHeld`], [`any_slider_held`]):
//!   value write-back, thumb and fill positioning, keyboard / gamepad steps and a held state for
//!   Bevy's headless `Slider`.
//! - **Motion** ([`PopIn`], [`PopOut`], [`PressSquash`], [`Wobble`], [`EasedValue`]): pop with
//!   overshoot, press squash, a "denied" wobble and eased numbers, all parameters yours.
//! - **Glyph check** ([`check_ascii`], [`check_glyphs`]): find text your font cannot draw, in
//!   tests.
//!
//! Add [`UiKitPlugin`] after Bevy's UI plugins (`DefaultPlugins` includes them). The full manual
//! is the README.

#![warn(missing_docs)]

mod motion;
mod scroll;
mod slider;
mod text_check;

/// Every Rust example in the README compiles (checked by `cargo test`).
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

use bevy_app::{App, Plugin, PostUpdate, Update};
use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy_ui::UiSystems;

/// The headless widgets this crate wires up (use it to be sure your versions match).
pub use bevy_ui_widgets;
pub use motion::{
    ease_toward, pop_scale, run_eased_values, run_transform_motion, wobble_offset, EaseSpec, EasedFill, EasedValue, Easing, FillAxis, MotionConfig,
    MotionFinished, MotionKind, PopIn, PopOut, PopOutEnd, PopSpec, PressSquash, SquashSpec, Wobble, WobbleSpec, EASE_EPSILON,
};
pub use scroll::{
    auto_hide_scrollbars, check_scroll_wiring, max_scroll_offset, remember_scroll, spawn_scroll_area, wheel_scroll, AutoHideScrollbar, ScrollAreaLook,
    ScrollAreaParts, ScrollConfig, ScrollKey, ScrollMemory, ScrollWiringError, Scroller,
};
pub use slider::{
    any_slider_held, clamp_to_range, on_step_slider, position_slider_parts, sanitize_slider_value, slider_extra_keys, slider_fraction, slider_gamepad_steps,
    sync_slider_held, write_back_slider_value, ManagedSlider, SliderChanged, SliderFill, SliderHeld, SliderInputConfig, SliderReleased, StepSlider,
};
pub use text_check::{check_all_glyphs, check_ascii, check_glyphs, is_printable_ascii, GlyphIssue, GlyphReport};

/// The system sets of this crate. Order your systems against them.
///
/// | set | schedule | what | order after it |
/// |---|---|---|---|
/// | `Scroll` | `Update` | wheel -> [`ScrollMemory`] -> scrollbar auto-hide | a system that rebuilds a scroll area, so it restores this frame's offset |
/// | `Sliders` | `Update` | [`SliderHeld`] + [`SliderReleased`] | a panel rebuild that reads them |
/// | `Motion` | `Update` | transform motion + eased values | a system that writes `UiTransform` scale / translation of a moving node (it would be overwritten otherwise) |
/// | `SliderVisuals` | `PostUpdate`, before `UiSystems::Prepare` | thumb + fill positioning | - |
///
/// In `Update` the three sets run in the order `Scroll`, `Sliders`, `Motion`.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiKitSystems {
    /// Wheel scrolling, the offset memory, scrollbar auto-hide.
    Scroll,
    /// The slider held state.
    Sliders,
    /// Transform motion and eased values.
    Motion,
    /// Slider thumb and fill positioning.
    SliderVisuals,
}

/// The plugin. Its fields are copied into the [`ScrollConfig`], [`SliderInputConfig`] and
/// [`MotionConfig`] resources, which you may change at runtime.
///
/// It does NOT add Bevy's widget plugins (`bevy::ui_widgets::UiWidgetsPlugins`, part of
/// `DefaultPlugins`): adding them twice would panic. A lean app needs `ScrollbarPlugin` and
/// `SliderPlugin` for the bar drag and the slider pointer input, and `InputDispatchPlugin` for
/// focused keyboard / gamepad input.
#[derive(Clone, Debug, Default)]
pub struct UiKitPlugin {
    /// Wheel behaviour.
    pub scroll: ScrollConfig,
    /// Extra slider input.
    pub slider: SliderInputConfig,
    /// Global motion settings.
    pub motion: MotionConfig,
}

impl Plugin for UiKitPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.scroll.clone())
            .insert_resource(self.slider.clone())
            .insert_resource(self.motion.clone())
            .init_resource::<ScrollMemory>()
            .add_message::<SliderReleased>()
            .add_observer(write_back_slider_value)
            .add_observer(on_step_slider)
            .add_observer(slider_extra_keys)
            .add_observer(slider_gamepad_steps)
            .configure_sets(Update, (UiKitSystems::Scroll, UiKitSystems::Sliders, UiKitSystems::Motion).chain())
            .configure_sets(PostUpdate, UiKitSystems::SliderVisuals.before(UiSystems::Prepare))
            .add_systems(Update, (wheel_scroll, remember_scroll, auto_hide_scrollbars).chain().in_set(UiKitSystems::Scroll))
            .add_systems(Update, sync_slider_held.in_set(UiKitSystems::Sliders))
            .add_systems(Update, (run_transform_motion, run_eased_values).chain().in_set(UiKitSystems::Motion))
            .add_systems(PostUpdate, position_slider_parts.in_set(UiKitSystems::SliderVisuals));
    }
}
