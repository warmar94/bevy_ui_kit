//! Motion helpers: every one reaches its target deterministically under a fixed time step.

mod common;

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::ui::{Pressed, Val2};
use bevy_ui_kit::*;
use common::*;

/// The example numbers a game would take from its theme.
const POP: PopSpec = PopSpec::new(0.2, 0.8, 1.08);
const WOBBLE: WobbleSpec = WobbleSpec::horizontal(0.3, 6.0, 12.0);

fn frames_for(secs: f32) -> usize {
    (secs / STEP.as_secs_f32()).ceil() as usize + 3
}

#[derive(Resource, Default)]
struct Finished(Vec<(Entity, MotionKind)>);

fn motion_app() -> App {
    let mut app = app();
    app.init_resource::<Finished>().add_observer(|f: On<MotionFinished>, mut seen: ResMut<Finished>| seen.0.push((f.entity, f.kind)));
    app.update(); // the first frame has no time delta
    app
}

fn scale(app: &App, e: Entity) -> Vec2 {
    app.world().get::<UiTransform>(e).unwrap().scale
}

#[test]
fn pop_in_starts_small_overshoots_and_settles_at_one() {
    let mut app = motion_app();
    let e = app.world_mut().spawn((Node::default(), PopIn::new(POP))).id();
    assert_eq!(scale(&app, e), Vec2::splat(0.8), "never drawn full size for a frame");
    let mut peak = 0.0f32;
    for _ in 0..frames_for(POP.secs) {
        app.update();
        peak = peak.max(scale(&app, e).x);
    }
    assert_eq!(scale(&app, e), Vec2::ONE);
    assert!(app.world().get::<PopIn>(e).is_none(), "removes itself");
    assert!(peak > 1.0 && peak <= 1.08 + 1e-4, "overshoot seen: {peak}");
    assert_eq!(app.world().resource::<Finished>().0, vec![(e, MotionKind::PopIn)]);
    println!(">>> pop-in: peak scale {peak:.4}, settled at 1");
}

#[test]
fn pop_scale_is_pure_and_safe() {
    assert_eq!(pop_scale(&POP, 0.0), 0.8);
    assert_eq!(pop_scale(&POP, POP.secs), 1.0);
    assert!((pop_scale(&POP, POP.secs * POP.peak_at) - 1.08).abs() < 1e-5, "the peak is where it says");
    for bad in [PopSpec::new(f32::NAN, 0.8, 1.1), PopSpec::new(0.0, 0.8, 1.1), PopSpec::new(-1.0, 0.8, 1.1), PopSpec::new(0.2, f32::NAN, 1.1)] {
        assert_eq!(pop_scale(&bad, 0.05), 1.0);
    }
    assert_eq!(pop_scale(&POP, f32::INFINITY), 1.0);
}

#[test]
fn pop_out_shrinks_then_despawns_or_hides() {
    let mut app = motion_app();
    let gone = app
        .world_mut()
        .spawn((Node::default(), PopOut::new(POP, PopOutEnd::Despawn)))
        .with_children(|c| {
            c.spawn(Node::default());
        })
        .id();
    let hidden = app.world_mut().spawn((Node::default(), PopOut::new(POP, PopOutEnd::Hide))).id();
    let kept = app.world_mut().spawn((Node::default(), PopOut::new(POP, PopOutEnd::Keep))).id();
    app.update();
    assert!(scale(&app, kept).x > 1.0, "rises to the overshoot first");
    for _ in 0..frames_for(POP.secs) {
        app.update();
    }
    assert!(app.world().get_entity(gone).is_err(), "despawned");
    assert_eq!(*app.world().get::<Visibility>(hidden).unwrap(), Visibility::Hidden);
    assert_eq!(scale(&app, hidden), Vec2::ONE, "hidden at its normal size, ready to show again");
    assert_eq!(scale(&app, kept), Vec2::splat(0.8));
    assert_eq!(app.world().resource::<Finished>().0.len(), 3);
}

#[test]
fn press_squash_follows_the_press_and_springs_back() {
    let mut app = motion_app();
    let spec = SquashSpec { pressed_scale: Vec2::new(1.06, 0.9), rate: 30.0 };
    let button = app.world_mut().spawn((Node::default(), PressSquash::new(spec), Interaction::None)).id();
    let widget = app.world_mut().spawn((Node::default(), PressSquash::new(spec))).id();

    *app.world_mut().get_mut::<Interaction>(button).unwrap() = Interaction::Pressed;
    app.world_mut().entity_mut(widget).insert(Pressed);
    for _ in 0..100 {
        app.update();
    }
    assert_eq!(scale(&app, button), spec.pressed_scale, "bevy_ui Interaction::Pressed");
    assert_eq!(scale(&app, widget), spec.pressed_scale, "bevy_ui_widgets Pressed marker");

    *app.world_mut().get_mut::<Interaction>(button).unwrap() = Interaction::None;
    app.world_mut().entity_mut(widget).remove::<Pressed>();
    for _ in 0..100 {
        app.update();
    }
    assert_eq!(scale(&app, button), Vec2::ONE);
    assert_eq!(scale(&app, widget), Vec2::ONE);
}

#[test]
fn wobble_shakes_and_comes_to_rest() {
    let mut app = motion_app();
    let e = app.world_mut().spawn((Node::default(), Wobble::new(WOBBLE))).id();
    let mut max_dx = 0.0f32;
    for _ in 0..frames_for(WOBBLE.secs) {
        app.update();
        if let Val::Px(x) = app.world().get::<UiTransform>(e).unwrap().translation.x {
            max_dx = max_dx.max(x.abs());
        }
    }
    assert!(max_dx > 1.0 && max_dx <= 6.0, "it moved, within the amplitude: {max_dx}");
    assert_eq!(app.world().get::<UiTransform>(e).unwrap().translation, Val2::px(0.0, 0.0));
    assert!(app.world().get::<Wobble>(e).is_none());
    assert_eq!(wobble_offset(&WOBBLE, f32::NAN), Vec2::ZERO);
    assert_eq!(wobble_offset(&WobbleSpec::horizontal(0.0, 6.0, 12.0), 0.1), Vec2::ZERO);
}

#[test]
fn motions_on_one_node_combine() {
    let mut app = motion_app();
    let spec = SquashSpec { pressed_scale: Vec2::splat(0.5), rate: 0.0 };
    let e = app.world_mut().spawn((Node::default(), PressSquash::new(spec), Pressed, Wobble::new(WOBBLE))).id();
    app.update();
    app.update();
    let tf = *app.world().get::<UiTransform>(e).unwrap();
    assert_eq!(tf.scale, Vec2::splat(0.5), "squash (instant at rate 0)");
    assert_ne!(tf.translation, Val2::px(0.0, 0.0), "and the wobble at the same time");
}

#[test]
fn eased_values_reach_their_targets_and_drive_a_bar() {
    let mut app = motion_app();
    let spec = EaseSpec { rise: Easing::Instant, fall: Easing::Linear { per_sec: 1.0 } };
    let bar = app.world_mut().spawn((EasedValue::new(1.0, spec), EasedFill::default())).id();
    app.world_mut().get_mut::<EasedValue>(bar).unwrap().set_target(0.5);
    app.update();
    let v = app.world().get::<EasedValue>(bar).unwrap().current();
    assert!((v - 0.99).abs() < 1e-4, "one 10 ms step at 1/s: {v}");
    for _ in 0..60 {
        app.update();
    }
    assert_eq!(app.world().get::<EasedValue>(bar).unwrap().current(), 0.5);
    assert_eq!(app.world().get::<Node>(bar).unwrap().width, Val::Percent(50.0));

    app.world_mut().get_mut::<EasedValue>(bar).unwrap().set_target(0.9);
    app.update();
    assert_eq!(app.world().get::<EasedValue>(bar).unwrap().current(), 0.9, "rises instantly");

    let exp = EaseSpec::symmetric(Easing::Exponential { rate: 10.0 });
    let mut v = 0.0;
    for _ in 0..200 {
        v = ease_toward(v, 1.0, &exp, 0.01);
    }
    assert_eq!(v, 1.0, "exponential easing snaps onto the target instead of creeping forever");
    assert_eq!(ease_toward(0.3, f32::NAN, &exp, 0.01), 0.3);
    assert_eq!(ease_toward(f32::NAN, 0.7, &exp, 0.01), 0.7);
    assert!(ease_toward(0.0, 1.0, &exp, 1.0e9) <= 1.0, "never overshoots");
}

#[test]
fn reduced_motion_finishes_everything_at_once() {
    let mut app = app_with(UiKitPlugin { motion: MotionConfig { enabled: false, ..default() }, ..default() });
    app.update();
    let pop = app.world_mut().spawn((Node::default(), PopIn::new(POP))).id();
    let eased = app.world_mut().spawn(EasedValue::new(0.0, EaseSpec::symmetric(Easing::Linear { per_sec: 0.001 }))).id();
    app.world_mut().get_mut::<EasedValue>(eased).unwrap().set_target(1.0);
    app.update();
    assert_eq!(scale(&app, pop), Vec2::ONE);
    assert!(app.world().get::<PopIn>(pop).is_none());
    assert_eq!(app.world().get::<EasedValue>(eased).unwrap().current(), 1.0);
}

#[test]
fn a_long_frame_does_not_skip_the_pop() {
    let mut app = motion_app();
    let e = app.world_mut().spawn((Node::default(), PopIn::new(POP))).id();
    // One 2-second hitch (a panel spawned on a loading frame).
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs(2)));
    app.update();
    let pop = app.world().get::<PopIn>(e).expect("capped at max_step_secs: still playing");
    assert!((pop.elapsed() - 0.05).abs() < 1e-6, "advanced by the cap, not by 2 s: {}", pop.elapsed());
}
