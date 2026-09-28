//! Wiring for Bevy's headless [`Slider`].
//!
//! `bevy_ui_widgets::Slider` owns the pointer maths and fires [`ValueChange`], and that is all: it
//! does not write [`SliderValue`] back, does not move the thumb and draws nothing. A slider built
//! on it alone works perfectly while looking frozen. [`ManagedSlider`] opts a slider into the
//! missing half: value write-back (clamped, NaN-safe, optionally snapped to its step), thumb and
//! fill positioning, extra keyboard / gamepad steps, and a held state for games that rebuild their
//! panels.

use bevy_ecs::hierarchy::{ChildOf, Children};
use bevy_ecs::prelude::*;
use bevy_input::gamepad::{GamepadButton, GamepadButtonChangedEvent};
use bevy_input::keyboard::{KeyCode, KeyboardInput};
use bevy_input::ButtonState;
use bevy_input_focus::FocusedInput;
use bevy_ui::{ComputedNode, Node, Pressed, Val};
use bevy_ui_widgets::{Slider, SliderDragState, SliderRange, SliderStep, SliderThumb, SliderValue, ValueChange};

/// Extra slider input, inserted by [`UiKitPlugin`](crate::UiKitPlugin) from its `slider` field.
///
/// Bevy's slider already steps on ArrowLeft / ArrowRight and jumps on Home / End while focused.
/// This adds, for [`ManagedSlider`]s only: ArrowUp / ArrowDown on vertical sliders, PageUp /
/// PageDown, and the gamepad D-pad (focused input reaches the slider through Bevy's
/// `InputDispatchPlugin`, part of `DefaultPlugins`).
#[derive(Resource, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize", serde(default))]
pub struct SliderInputConfig {
    /// Handle ArrowUp / ArrowDown (vertical sliders) and PageUp / PageDown. Default `true`.
    pub keyboard: bool,
    /// Handle the D-pad of the gamepad whose input reaches the focused slider. Default `true`.
    pub gamepad: bool,
    /// How many steps PageUp / PageDown move. Default `10.0`.
    pub page_steps: f32,
}

impl Default for SliderInputConfig {
    fn default() -> Self {
        Self { keyboard: true, gamepad: true, page_steps: 10.0 }
    }
}

/// Opt a [`Slider`] into this crate's wiring. Put it on the same entity as the `Slider`.
///
/// - **Write-back:** every [`ValueChange<f32>`] from the slider is clamped to its
///   [`SliderRange`], snapped to its [`SliderStep`] when `snap_to_step` is set, and written into
///   [`SliderValue`]; a NaN value (or a NaN / infinite range or step) is ignored instead of
///   written. Then [`SliderChanged`] is triggered on the slider with the value actually stored.
///   Do not also add Bevy's `slider_self_update` observer.
/// - **Visuals:** the descendant marked [`SliderThumb`] is positioned from the value (its `left`,
///   or `top` on a vertical slider) and every descendant marked [`SliderFill`] gets its `width`
///   (`height` on a vertical slider) set to the value's percentage. See [`position_slider_parts`].
/// - **Held state:** [`SliderHeld`] is present while the slider is pressed or dragged, and
///   [`SliderReleased`] is written when it is let go.
/// - **Extra steps:** see [`SliderInputConfig`] and [`StepSlider`].
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub struct ManagedSlider {
    /// Snap every written value to `start + n * step` (from [`SliderStep`]). The snap divides by
    /// the step COUNT, so a 0.05 step on 0..1 stores exactly `0.9`, not `0.90000004`.
    pub snap_to_step: bool,
}

impl ManagedSlider {
    /// Write-back without snapping.
    pub const fn new() -> Self {
        Self { snap_to_step: false }
    }

    /// Write-back snapped to the slider's [`SliderStep`].
    pub const fn snapped() -> Self {
        Self { snap_to_step: true }
    }
}

/// Marks the "filled" part of a [`ManagedSlider`]'s track (a descendant of the slider). Its
/// `width` (horizontal) or `height` (vertical) is set to `Val::Percent(value fraction x 100)`, a
/// percentage of ITS parent. Everything else about it is yours.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SliderFill;

/// Present on a [`ManagedSlider`] while it is pressed or dragged. Maintained by
/// [`sync_slider_held`]; use it to style a held thumb, or see [`any_slider_held`] to skip a
/// panel rebuild.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SliderHeld;

/// Written when a [`ManagedSlider`] stops being held. A game that skipped rebuilding its panel
/// during the drag rebuilds once on this.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct SliderReleased {
    /// The slider.
    pub entity: Entity,
    /// Its value at release.
    pub value: f32,
}

/// Triggered on a [`ManagedSlider`] after its value was written back.
///
/// `value` is what is now in [`SliderValue`] (clamped, snapped); observe this rather than
/// Bevy's raw [`ValueChange`] when you need the stored value.
#[derive(EntityEvent, Clone, Copy, Debug, PartialEq)]
pub struct SliderChanged {
    /// The slider.
    #[event_target]
    pub entity: Entity,
    /// The stored value.
    pub value: f32,
    /// `false` in the middle of a drag, `true` when the interaction finished (a key step, the
    /// drag's release).
    pub is_final: bool,
}

/// Trigger on a [`ManagedSlider`] to move it by `steps` of its [`SliderStep`] (negative = down),
/// from any input you like (your own action map, a gamepad shoulder button). Goes through the
/// same clamp / snap / NaN guard as a drag.
#[derive(EntityEvent, Clone, Copy, Debug, PartialEq)]
pub struct StepSlider {
    /// The slider.
    #[event_target]
    pub entity: Entity,
    /// How many steps; fractional allowed.
    pub steps: f32,
}

/// Clamp to `[start, end]` without `f32::clamp`'s panics: a reversed range is treated as its
/// sorted pair, a NaN bound yields `None`, a NaN value yields `None`.
pub fn clamp_to_range(value: f32, range: &SliderRange) -> Option<f32> {
    let (a, b) = (range.start(), range.end());
    if value.is_nan() || !a.is_finite() || !b.is_finite() {
        return None;
    }
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    Some(value.max(lo).min(hi))
}

/// The value a [`ManagedSlider`] stores for a requested `value`: clamped to `range`, snapped to
/// `step` when `snap` is set. `None` means "ignore this change" (NaN value, NaN / infinite range).
///
/// A step that is not finite and positive disables snapping rather than failing.
pub fn sanitize_slider_value(value: f32, range: &SliderRange, step: Option<f32>, snap: bool) -> Option<f32> {
    let v = clamp_to_range(value, range)?;
    let Some(step) = step.filter(|s| snap && s.is_finite() && *s > 0.0) else { return Some(v) };
    let (lo, hi) = (range.start().min(range.end()), range.start().max(range.end()));
    let span = hi - lo;
    if span <= 0.0 {
        return Some(lo);
    }
    // Divide by the step COUNT, an exactly representable integer when the step divides the span:
    // `(v * 20).round() / 20` lands on the same float as the literal, `(v / 0.05).round() * 0.05`
    // does not.
    let count = (span / step).round().max(1.0);
    let frac = ((v - lo) / span * count).round() / count;
    let snapped = lo + frac * span;
    Some(snapped.max(lo).min(hi))
}

/// `value`'s position along `range` as a fraction in `0..=1` (0 for NaN / empty ranges).
pub fn slider_fraction(value: f32, range: &SliderRange) -> f32 {
    let (a, b) = (range.start(), range.end());
    let span = b - a;
    if !value.is_finite() || !span.is_finite() || span == 0.0 {
        return 0.0;
    }
    let t = (value - a) / span;
    if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

type ManagedParts<'a> = (&'a SliderValue, &'a SliderRange, Option<&'a SliderStep>, &'a ManagedSlider);

/// Observer: write a [`ManagedSlider`]'s [`ValueChange`] back into [`SliderValue`] and trigger
/// [`SliderChanged`].
pub fn write_back_slider_value(change: On<ValueChange<f32>>, sliders: Query<ManagedParts>, mut commands: Commands) {
    let entity = change.source;
    let Ok((current, range, step, managed)) = sliders.get(entity) else { return };
    let Some(v) = sanitize_slider_value(change.value, range, step.map(|s| s.0), managed.snap_to_step) else {
        tracing::debug!("bevy_ui_kit: ignored a non-finite value change on slider {entity}");
        return;
    };
    if current.0 != v {
        commands.entity(entity).insert(SliderValue(v));
    } else if !change.is_final {
        return;
    }
    commands.trigger(SliderChanged { entity, value: v, is_final: change.is_final });
}

/// Observer: apply a [`StepSlider`].
pub fn on_step_slider(step: On<StepSlider>, sliders: Query<ManagedParts>, mut commands: Commands) {
    let entity = step.entity;
    let Ok((value, _, slider_step, _)) = sliders.get(entity) else { return };
    let size = slider_step.map(|s| s.0).unwrap_or(1.0);
    let target = value.0 + step.steps * size;
    if !target.is_finite() {
        tracing::debug!("bevy_ui_kit: ignored a non-finite step on slider {entity}");
        return;
    }
    // Through Bevy's own event, so every ValueChange observer (yours included) sees it.
    commands.trigger(ValueChange { source: entity, value: target, is_final: true });
}

/// Observer: the extra keys of [`SliderInputConfig`] on a focused [`ManagedSlider`].
pub fn slider_extra_keys(
    mut input: On<FocusedInput<KeyboardInput>>,
    config: Res<SliderInputConfig>,
    sliders: Query<(&Slider, Option<&ComputedNode>), With<ManagedSlider>>,
    mut commands: Commands,
) {
    if !config.keyboard || input.input.state != ButtonState::Pressed {
        return;
    }
    let entity = input.focused_entity;
    let Ok((slider, computed)) = sliders.get(entity) else { return };
    let vertical = computed.map_or(slider.orientation == bevy_ui_widgets::SliderOrientation::Vertical, |n| slider.orientation.is_vertical(n));
    let page = if config.page_steps.is_finite() { config.page_steps } else { 0.0 };
    let steps = match input.input.key_code {
        KeyCode::ArrowUp if vertical => 1.0,
        KeyCode::ArrowDown if vertical => -1.0,
        KeyCode::PageUp => page,
        KeyCode::PageDown => -page,
        _ => return,
    };
    input.propagate(false);
    if steps != 0.0 {
        commands.trigger(StepSlider { entity, steps });
    }
}

/// Observer: the D-pad on a focused [`ManagedSlider`] (right / up = +1 step, left / down = -1).
pub fn slider_gamepad_steps(
    mut input: On<FocusedInput<GamepadButtonChangedEvent>>,
    config: Res<SliderInputConfig>,
    sliders: Query<(), With<ManagedSlider>>,
    mut commands: Commands,
) {
    if !config.gamepad || input.input.state != ButtonState::Pressed {
        return;
    }
    let entity = input.focused_entity;
    if !sliders.contains(entity) {
        return;
    }
    let steps = match input.input.button {
        GamepadButton::DPadRight | GamepadButton::DPadUp => 1.0,
        GamepadButton::DPadLeft | GamepadButton::DPadDown => -1.0,
        _ => return,
    };
    input.propagate(false);
    commands.trigger(StepSlider { entity, steps });
}

/// Is any [`Slider`] (managed or not) pressed or being dragged? A run condition: give it to the
/// system that rebuilds a panel full of sliders as `.run_if(not(any_slider_held))`, or it will
/// despawn the slider under the player's mouse.
///
/// Both halves matter: a click on the TRACK inserts `Pressed` but the drag does not start until
/// the pointer MOVES, so testing only the drag state lets that first click rebuild the panel and
/// the drag then has nothing to land on.
pub fn any_slider_held(sliders: Query<(Option<&SliderDragState>, Has<Pressed>), With<Slider>>) -> bool {
    sliders.iter().any(|(drag, pressed)| pressed || drag.is_some_and(|d| d.dragging))
}

type HeldParts<'a> = (Entity, Option<&'a SliderDragState>, Has<Pressed>, Has<SliderHeld>, &'a SliderValue);

/// Keep [`SliderHeld`] in step with each [`ManagedSlider`]'s pressed / dragged state and write
/// [`SliderReleased`] when one is let go.
pub fn sync_slider_held(sliders: Query<HeldParts, With<ManagedSlider>>, mut released: MessageWriter<SliderReleased>, mut commands: Commands) {
    for (entity, drag, pressed, was_held, value) in &sliders {
        let held = pressed || drag.is_some_and(|d| d.dragging);
        if held && !was_held {
            commands.entity(entity).insert(SliderHeld);
        } else if !held && was_held {
            commands.entity(entity).remove::<SliderHeld>();
            released.write(SliderReleased { entity, value: value.0 });
        }
    }
}

type SliderParts<'a> = (Entity, &'a Slider, &'a SliderValue, &'a SliderRange, Option<&'a ComputedNode>);

type ThumbParts<'a> = (&'a mut Node, Option<&'a ComputedNode>, Option<&'a ChildOf>);

/// Position every [`ManagedSlider`]'s thumb and fills from its value. Runs every frame in
/// `PostUpdate` before Bevy's UI layout, so a held slider looks alive even while your panel is
/// frozen.
///
/// The thumb (the descendant with [`SliderThumb`]) is expected to be absolutely positioned inside
/// a parent that spans the slider (usually the slider itself; Bevy's drag maths measures the
/// slider minus the thumb). Its travel is the parent's inner width minus its own width, so it
/// never hangs off either end; this writes `left` (horizontal) or `top` (vertical, where the
/// value grows upwards as in Bevy) in logical pixels. Before the first layout, when sizes are
/// unknown, it writes a percentage instead.
pub fn position_slider_parts(
    sliders: Query<SliderParts, With<ManagedSlider>>,
    children: Query<&Children>,
    mut thumbs: Query<ThumbParts, (With<SliderThumb>, Without<SliderFill>)>,
    mut fills: Query<&mut Node, (With<SliderFill>, Without<SliderThumb>)>,
    computed: Query<&ComputedNode, Without<SliderThumb>>,
) {
    for (slider_entity, slider, value, range, slider_node) in &sliders {
        let t = slider_fraction(value.0, range);
        let vertical = slider_node.map_or(slider.orientation == bevy_ui_widgets::SliderOrientation::Vertical, |n| slider.orientation.is_vertical(n));
        for part in children.iter_descendants(slider_entity) {
            if let Ok((mut node, own, parent)) = thumbs.get_mut(part) {
                let parent_node = parent.and_then(|p| computed.get(p.parent()).ok());
                let want = thumb_offset(t, vertical, own, parent_node);
                let slot = if vertical { &mut node.top } else { &mut node.left };
                if *slot != want {
                    *slot = want;
                }
            } else if let Ok(mut node) = fills.get_mut(part) {
                let want = Val::Percent(t * 100.0);
                let slot = if vertical { &mut node.height } else { &mut node.width };
                if *slot != want {
                    *slot = want;
                }
            }
        }
    }
}

/// The thumb's `left` / `top` for fraction `t`.
fn thumb_offset(t: f32, vertical: bool, thumb: Option<&ComputedNode>, parent: Option<&ComputedNode>) -> Val {
    // Vertical sliders grow upwards (0 at the bottom), as Bevy's pointer maths assumes.
    let along = if vertical { 1.0 - t } else { t };
    let (Some(thumb), Some(parent)) = (thumb, parent) else { return Val::Percent(along * 100.0) };
    let axis = |v: bevy_math::Vec2| if vertical { v.y } else { v.x };
    let inner = axis(parent.size) - axis(parent.border.min_inset) - axis(parent.border.max_inset);
    if inner <= 0.0 {
        return Val::Percent(along * 100.0);
    }
    let travel = ((inner - axis(thumb.size)).max(0.0)) * parent.inverse_scale_factor;
    let px = along * travel;
    if px.is_finite() {
        Val::Px(px)
    } else {
        Val::Px(0.0)
    }
}
