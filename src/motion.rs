//! Motion helpers: pop-in / pop-out with overshoot, press squash, a "denied" wobble and eased
//! values. Pure transform and number animation; every duration, scale and distance is yours.
//!
//! The transform motions ([`PopIn`], [`PopOut`], [`PressSquash`], [`Wobble`]) write the node's
//! [`UiTransform`] **scale and translation** while one of them is on the entity (rotation is never
//! touched): scales multiply, translations add. They run on real time, so a paused game's menus
//! still animate. None of them has a default: you pass the numbers, typically from your theme.

use bevy_camera::visibility::Visibility;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::world::DeferredWorld;
use bevy_math::Vec2;
use bevy_time::{Real, Time};
use bevy_ui::{Interaction, Node, Pressed, UiTransform, Val, Val2};

/// Global motion settings, inserted by [`UiKitPlugin`](crate::UiKitPlugin) from its `motion`
/// field.
#[derive(Resource, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize", serde(default))]
pub struct MotionConfig {
    /// `false` = reduced motion: one-shot motions finish at once, squash and eased values jump to
    /// their targets. Default `true`.
    pub enabled: bool,
    /// The longest time step one frame may advance a motion, in seconds. A panel is often spawned
    /// on a slow frame; without a cap its pop-in would be over before it was ever drawn. `0` or a
    /// non-finite value = no cap. Default `0.05`.
    pub max_step_secs: f32,
}

impl Default for MotionConfig {
    fn default() -> Self {
        Self { enabled: true, max_step_secs: 0.05 }
    }
}

/// The shape of a pop: scale from `from` up to `overshoot` (reached at `peak_at` of the
/// duration, easing out), then settle to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub struct PopSpec {
    /// Total duration, seconds.
    pub secs: f32,
    /// The starting scale (a pop-out's final scale).
    pub from: f32,
    /// The peak scale (above 1 for an overshoot; 1 for none).
    pub overshoot: f32,
    /// Where in the duration the peak is, as a fraction in `0..1`.
    pub peak_at: f32,
}

impl PopSpec {
    /// A pop with its peak at 60 % of the duration.
    pub const fn new(secs: f32, from: f32, overshoot: f32) -> Self {
        Self { secs, from, overshoot, peak_at: 0.6 }
    }

    /// The scale `t` seconds into a pop-in (1 when done or when the spec is degenerate).
    pub fn scale_at(&self, t: f32) -> f32 {
        pop_scale(self, t)
    }
}

/// The pop-in scale `t` seconds in. Returns 1 once `t >= secs`, and for a non-finite or
/// non-positive duration, a non-finite `t`, or non-finite scales.
pub fn pop_scale(spec: &PopSpec, t: f32) -> f32 {
    let PopSpec { secs, from, overshoot, peak_at } = *spec;
    let playing = positive(secs) && t.is_finite() && t < secs && from.is_finite() && overshoot.is_finite();
    if !playing {
        return 1.0;
    }
    let u = (t / secs).clamp(0.0, 1.0);
    let split = if peak_at.is_finite() { peak_at.clamp(0.01, 0.99) } else { 0.6 };
    if u < split {
        let k = u / split;
        from + (overshoot - from) * (1.0 - (1.0 - k) * (1.0 - k))
    } else {
        let k = (u - split) / (1.0 - split);
        overshoot + (1.0 - overshoot) * k * (2.0 - k)
    }
}

/// Scale the node in with [`PopSpec`] (from `spec.from` through the overshoot to 1), then remove
/// itself and trigger [`MotionFinished`]. Inserting it sets the scale to `spec.from` at once, so
/// the node is never drawn full size for a frame. Insert it again to restart.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
#[require(UiTransform)]
#[component(on_insert = pop_in_on_insert)]
pub struct PopIn {
    /// The shape.
    pub spec: PopSpec,
    elapsed: f32,
}

impl PopIn {
    /// Pop in with `spec`.
    pub const fn new(spec: PopSpec) -> Self {
        Self { spec, elapsed: 0.0 }
    }

    /// Seconds played so far.
    pub fn elapsed(&self) -> f32 {
        self.elapsed
    }
}

fn pop_in_on_insert(mut world: DeferredWorld, ctx: HookContext) {
    let Some(pop) = world.get::<PopIn>(ctx.entity).copied() else { return };
    let enabled = world.get_resource::<MotionConfig>().is_none_or(|c| c.enabled);
    if !enabled {
        return;
    }
    if let Some(mut tf) = world.get_mut::<UiTransform>(ctx.entity) {
        tf.scale = Vec2::splat(pop_scale(&pop.spec, 0.0));
    }
}

/// What happens when a [`PopOut`] finishes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub enum PopOutEnd {
    /// Despawn the entity (and its children).
    #[default]
    Despawn,
    /// Set `Visibility::Hidden` and reset the scale to 1.
    Hide,
    /// Leave it at its final scale.
    Keep,
}

/// Scale the node out: the reverse of a pop-in (1, up to the overshoot, down to `spec.from`),
/// then [`PopOutEnd`]. Triggers [`MotionFinished`] before the ending is applied. Replaces a
/// running [`PopIn`].
#[derive(Component, Clone, Copy, Debug, PartialEq)]
#[require(UiTransform)]
pub struct PopOut {
    /// The shape (played backwards).
    pub spec: PopSpec,
    /// What to do at the end.
    pub then: PopOutEnd,
    elapsed: f32,
}

impl PopOut {
    /// Pop out with `spec`, then `then`.
    pub const fn new(spec: PopSpec, then: PopOutEnd) -> Self {
        Self { spec, then, elapsed: 0.0 }
    }

    /// Seconds played so far.
    pub fn elapsed(&self) -> f32 {
        self.elapsed
    }
}

/// How a pressed node squashes.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub struct SquashSpec {
    /// The scale while pressed (e.g. wider and shorter).
    pub pressed_scale: Vec2,
    /// How fast the scale follows the press, per second (exponential; ~5 / rate seconds to
    /// settle). Non-finite or non-positive = instant.
    pub rate: f32,
}

/// Squash the node while it is pressed ([`Interaction::Pressed`] or Bevy's [`Pressed`] marker)
/// and spring back when released. Stays on the entity; while it is there, it owns the scale
/// (together with any other motion on the same node).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
#[require(UiTransform)]
pub struct PressSquash {
    /// The squash.
    pub spec: SquashSpec,
    current: Vec2,
}

impl PressSquash {
    /// Squash with `spec`.
    pub const fn new(spec: SquashSpec) -> Self {
        Self { spec, current: Vec2::ONE }
    }

    /// The current squash scale.
    pub fn current(&self) -> Vec2 {
        self.current
    }
}

/// A "denied" wobble: a sideways shake that decays to nothing.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub struct WobbleSpec {
    /// Duration, seconds.
    pub secs: f32,
    /// Peak displacement, logical pixels.
    pub amplitude_px: f32,
    /// Oscillations per second.
    pub hz: f32,
    /// Direction of the shake (normalised when used; zero = horizontal).
    pub direction: Vec2,
}

impl WobbleSpec {
    /// A horizontal wobble.
    pub const fn horizontal(secs: f32, amplitude_px: f32, hz: f32) -> Self {
        Self { secs, amplitude_px, hz, direction: Vec2::X }
    }

    /// The offset `t` seconds in (zero when done or degenerate).
    pub fn offset_at(&self, t: f32) -> Vec2 {
        wobble_offset(self, t)
    }
}

/// The wobble offset `t` seconds in: `sin(2 pi hz t) x amplitude x (1 - t / secs)`.
pub fn wobble_offset(spec: &WobbleSpec, t: f32) -> Vec2 {
    let WobbleSpec { secs, amplitude_px, hz, direction } = *spec;
    let playing = positive(secs) && t.is_finite() && (0.0..secs).contains(&t) && amplitude_px.is_finite() && hz.is_finite();
    if !playing {
        return Vec2::ZERO;
    }
    let dir = direction.try_normalize().unwrap_or(Vec2::X);
    let fade = 1.0 - t / secs;
    dir * (t * hz * std::f32::consts::TAU).sin() * amplitude_px * fade
}

/// Shake the node (translation) with [`WobbleSpec`], then remove itself and trigger
/// [`MotionFinished`]. Insert it again to restart.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
#[require(UiTransform)]
pub struct Wobble {
    /// The shake.
    pub spec: WobbleSpec,
    elapsed: f32,
}

impl Wobble {
    /// Wobble with `spec`.
    pub const fn new(spec: WobbleSpec) -> Self {
        Self { spec, elapsed: 0.0 }
    }

    /// Seconds played so far.
    pub fn elapsed(&self) -> f32 {
        self.elapsed
    }
}

/// Which one-shot motion finished (see [`MotionFinished`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionKind {
    /// A [`PopIn`].
    PopIn,
    /// A [`PopOut`] (triggered before its [`PopOutEnd`] is applied).
    PopOut,
    /// A [`Wobble`].
    Wobble,
}

/// Triggered on the entity when a one-shot motion finishes.
#[derive(EntityEvent, Clone, Copy, Debug, PartialEq)]
pub struct MotionFinished {
    /// The node.
    #[event_target]
    pub entity: Entity,
    /// Which motion.
    pub kind: MotionKind,
}

/// How an [`EasedValue`] moves toward its target.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub enum Easing {
    /// Jump.
    Instant,
    /// Close `1 - e^(-rate x dt)` of the remaining gap per step (fast, then slow).
    Exponential {
        /// Per second.
        rate: f32,
    },
    /// Move at a constant speed.
    Linear {
        /// Units per second.
        per_sec: f32,
    },
}

/// The easing up and down (a health bar that drops fast and refills slowly; a level meter that
/// rises at once and falls back gently).
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub struct EaseSpec {
    /// Used while the target is above the current value.
    pub rise: Easing,
    /// Used while the target is below.
    pub fall: Easing,
}

impl EaseSpec {
    /// The same easing both ways.
    pub const fn symmetric(ease: Easing) -> Self {
        Self { rise: ease, fall: ease }
    }
}

/// Values closer than this to their target snap onto it.
pub const EASE_EPSILON: f32 = 1e-4;

/// One easing step of `dt` seconds from `current` toward `target`. Never overshoots; a
/// non-finite `target` leaves `current` unchanged; a non-finite `current` jumps to `target`.
pub fn ease_toward(current: f32, target: f32, spec: &EaseSpec, dt: f32) -> f32 {
    if !target.is_finite() {
        return current;
    }
    if !current.is_finite() {
        return target;
    }
    let gap = target - current;
    if gap.abs() <= EASE_EPSILON {
        return target;
    }
    let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
    let ease = if gap > 0.0 { spec.rise } else { spec.fall };
    let next = match ease {
        Easing::Instant => target,
        Easing::Exponential { rate } if rate.is_finite() && rate > 0.0 => current + gap * (1.0 - (-rate * dt).exp()),
        Easing::Linear { per_sec } if per_sec.is_finite() && per_sec > 0.0 => current + gap.signum() * (per_sec * dt).min(gap.abs()),
        _ => target,
    };
    if (target - next).abs() <= EASE_EPSILON {
        target
    } else {
        next
    }
}

/// A number that eases toward a target every frame (a bar, a counter). Set the target with
/// [`EasedValue::set_target`]; read [`EasedValue::current`] to draw it. Add [`EasedFill`] on the
/// same entity to drive a bar node from it.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct EasedValue {
    /// How it moves.
    pub spec: EaseSpec,
    target: f32,
    current: f32,
}

impl EasedValue {
    /// Start at `value`, resting.
    pub fn new(value: f32, spec: EaseSpec) -> Self {
        let v = if value.is_finite() { value } else { 0.0 };
        Self { spec, target: v, current: v }
    }

    /// The value to draw.
    pub fn current(&self) -> f32 {
        self.current
    }

    /// Where it is heading.
    pub fn target(&self) -> f32 {
        self.target
    }

    /// Head for `target` (ignored if not finite).
    pub fn set_target(&mut self, target: f32) {
        if target.is_finite() {
            self.target = target;
        }
    }

    /// Jump to `value` and rest there (ignored if not finite).
    pub fn snap(&mut self, value: f32) {
        if value.is_finite() {
            self.target = value;
            self.current = value;
        }
    }

    /// At its target?
    pub fn is_settled(&self) -> bool {
        self.current == self.target
    }

    /// Advance by `dt` seconds (what the plugin's system does each frame).
    pub fn step(&mut self, dt: f32) {
        self.current = ease_toward(self.current, self.target, &self.spec, dt);
    }
}

/// Which size an [`EasedFill`] drives.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub enum FillAxis {
    /// `Node::width`.
    #[default]
    Width,
    /// `Node::height`.
    Height,
}

/// Drive this node's width or height from the [`EasedValue`] on the same entity, as
/// `Val::Percent(current x 100)` with `current` clamped to `0..=1`: a bar.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[require(Node)]
pub struct EasedFill {
    /// Which size.
    pub axis: FillAxis,
}

/// Finite and above zero.
fn positive(x: f32) -> bool {
    x.is_finite() && x > 0.0
}

/// Entities with at least one transform motion.
type WithMotion = Or<(With<PopIn>, With<PopOut>, With<PressSquash>, With<Wobble>)>;

type MotionParts<'a> = (
    Entity,
    &'a mut UiTransform,
    Option<&'a mut PopIn>,
    Option<&'a mut PopOut>,
    Option<&'a mut PressSquash>,
    Option<&'a mut Wobble>,
    Option<&'a Interaction>,
    Has<Pressed>,
);

/// The frame's motion time step: real time, capped by [`MotionConfig::max_step_secs`].
fn motion_dt(time: Option<&Time<Real>>, config: &MotionConfig) -> f32 {
    let dt = time.map_or(0.0, |t| t.delta_secs());
    let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
    if config.max_step_secs.is_finite() && config.max_step_secs > 0.0 {
        dt.min(config.max_step_secs)
    } else {
        dt
    }
}

/// Advance every transform motion and write the combined scale and translation.
pub fn run_transform_motion(time: Option<Res<Time<Real>>>, config: Res<MotionConfig>, mut nodes: Query<MotionParts, WithMotion>, mut commands: Commands) {
    let dt = motion_dt(time.as_deref(), &config);
    for (entity, mut tf, pop_in, pop_out, squash, wobble, interaction, pressed) in &mut nodes {
        let mut scale = Vec2::ONE;
        let mut offset = Vec2::ZERO;

        if let Some(mut out) = pop_out {
            if pop_in.is_some() {
                commands.entity(entity).remove::<PopIn>();
            }
            out.elapsed += dt;
            let secs = out.spec.secs;
            let done = !(config.enabled && positive(secs)) || out.elapsed >= secs;
            if done {
                let end = if out.spec.from.is_finite() { out.spec.from } else { 0.0 };
                scale *= match out.then {
                    PopOutEnd::Keep => end,
                    PopOutEnd::Hide | PopOutEnd::Despawn => 1.0,
                };
                commands.trigger(MotionFinished { entity, kind: MotionKind::PopOut });
                match out.then {
                    PopOutEnd::Despawn => {
                        commands.entity(entity).try_despawn();
                        continue;
                    }
                    PopOutEnd::Hide => {
                        commands.entity(entity).try_insert(Visibility::Hidden).try_remove::<PopOut>();
                    }
                    PopOutEnd::Keep => {
                        commands.entity(entity).try_remove::<PopOut>();
                    }
                }
            } else {
                scale *= pop_scale(&out.spec, secs - out.elapsed);
            }
        } else if let Some(mut pop) = pop_in {
            pop.elapsed += dt;
            let secs = pop.spec.secs;
            let done = !(config.enabled && positive(secs)) || pop.elapsed >= secs;
            if done {
                commands.trigger(MotionFinished { entity, kind: MotionKind::PopIn });
                commands.entity(entity).try_remove::<PopIn>();
            } else {
                scale *= pop_scale(&pop.spec, pop.elapsed);
            }
        }

        if let Some(mut squash) = squash {
            let held = pressed || interaction == Some(&Interaction::Pressed);
            let want = if held && squash.spec.pressed_scale.is_finite() { squash.spec.pressed_scale } else { Vec2::ONE };
            let rate = squash.spec.rate;
            let next = if !(config.enabled && positive(rate)) {
                want
            } else {
                let k = 1.0 - (-rate * dt).exp();
                let v = squash.current + (want - squash.current) * k;
                if (want - v).abs().max_element() <= EASE_EPSILON {
                    want
                } else {
                    v
                }
            };
            if squash.current != next {
                squash.current = next;
            }
            scale *= squash.current;
        }

        if let Some(mut wobble) = wobble {
            wobble.elapsed += dt;
            let secs = wobble.spec.secs;
            let done = !(config.enabled && positive(secs)) || wobble.elapsed >= secs;
            if done {
                commands.trigger(MotionFinished { entity, kind: MotionKind::Wobble });
                commands.entity(entity).try_remove::<Wobble>();
            } else {
                offset += wobble_offset(&wobble.spec, wobble.elapsed);
            }
        }

        let translation = Val2::px(offset.x, offset.y);
        if tf.scale != scale {
            tf.scale = scale;
        }
        if tf.translation != translation {
            tf.translation = translation;
        }
    }
}

/// Advance every [`EasedValue`] and drive every [`EasedFill`].
pub fn run_eased_values(time: Option<Res<Time<Real>>>, config: Res<MotionConfig>, mut values: Query<(&mut EasedValue, Option<&EasedFill>, Option<&mut Node>)>) {
    let dt = motion_dt(time.as_deref(), &config);
    for (mut value, fill, node) in &mut values {
        if !value.is_settled() {
            if config.enabled {
                value.step(dt);
            } else {
                let t = value.target;
                value.current = t;
            }
        }
        let (Some(fill), Some(mut node)) = (fill, node) else { continue };
        let want = Val::Percent(value.current.clamp(0.0, 1.0) * 100.0);
        let slot = match fill.axis {
            FillAxis::Width => &mut node.width,
            FillAxis::Height => &mut node.height,
        };
        if *slot != want {
            *slot = want;
        }
    }
}
