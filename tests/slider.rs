//! Slider wiring: write-back, thumb / fill positioning, steps, the held state.

mod common;

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::ButtonState;
use bevy::input_focus::{InputDispatchPlugin, InputFocus};
use bevy::prelude::*;
use bevy::ui::Pressed;
use bevy::ui_widgets::{Slider, SliderDragState, SliderPlugin, SliderRange, SliderStep, SliderThumb, SliderValue, ValueChange};
use bevy::window::PrimaryWindow;
use bevy_ui_kit::*;
use common::*;

/// Thumb 20 px wide inside a 220 px slider: 200 px of travel.
const SLIDER_W: f32 = 220.0;
const THUMB_W: f32 = 20.0;

struct Parts {
    slider: Entity,
    thumb: Entity,
    fill: Entity,
}

/// A deliberately plain slider: layout only, no colours.
fn spawn_slider(app: &mut App, value: f32, managed: ManagedSlider) -> Parts {
    let w = app.world_mut();
    let slider = w
        .spawn((
            Slider::default(),
            SliderValue(value),
            SliderRange::new(0.0, 1.0),
            SliderStep(0.05),
            managed,
            Node { width: Val::Px(SLIDER_W), height: Val::Px(THUMB_W), ..default() },
            laid_out(Vec2::new(SLIDER_W, THUMB_W), Vec2::ZERO),
        ))
        .id();
    let track = w.spawn((Node { width: Val::Percent(100.0), ..default() }, ChildOf(slider))).id();
    let fill = w.spawn((Node::default(), SliderFill, ChildOf(track))).id();
    let thumb = w
        .spawn((
            SliderThumb,
            Node { position_type: PositionType::Absolute, width: Val::Px(THUMB_W), ..default() },
            laid_out(Vec2::splat(THUMB_W), Vec2::ZERO),
            ChildOf(slider),
        ))
        .id();
    Parts { slider, thumb, fill }
}

fn value(app: &App, e: Entity) -> f32 {
    app.world().get::<SliderValue>(e).expect("a slider value").0
}

fn change(app: &mut App, source: Entity, value: f32, is_final: bool) {
    app.world_mut().trigger(ValueChange { source, value, is_final });
    app.update();
}

#[test]
fn the_value_positions_the_thumb_and_the_fill() {
    let mut app = app();
    let p = spawn_slider(&mut app, 0.25, ManagedSlider::new());
    app.update();
    let thumb = app.world().get::<Node>(p.thumb).unwrap();
    assert_eq!(thumb.left, Val::Px(0.25 * (SLIDER_W - THUMB_W)), "inset by the thumb so it never hangs off the end");
    assert_eq!(app.world().get::<Node>(p.fill).unwrap().width, Val::Percent(25.0));

    change(&mut app, p.slider, 1.0, true);
    assert_eq!(app.world().get::<Node>(p.thumb).unwrap().left, Val::Px(SLIDER_W - THUMB_W));
    assert_eq!(app.world().get::<Node>(p.fill).unwrap().width, Val::Percent(100.0));
    println!(">>> slider: value 1.0 -> thumb left {:?}", app.world().get::<Node>(p.thumb).unwrap().left);
}

#[test]
fn a_vertical_slider_grows_upwards_and_sizes_the_fill_height() {
    let mut app = app();
    let p = spawn_slider(&mut app, 0.0, ManagedSlider::new());
    app.world_mut()
        .entity_mut(p.slider)
        .insert((Slider { orientation: bevy::ui_widgets::SliderOrientation::Vertical, ..default() }, laid_out(Vec2::new(THUMB_W, SLIDER_W), Vec2::ZERO)));
    app.update();
    assert_eq!(app.world().get::<Node>(p.thumb).unwrap().top, Val::Px(SLIDER_W - THUMB_W), "0 sits at the bottom");
    change(&mut app, p.slider, 1.0, true);
    assert_eq!(app.world().get::<Node>(p.thumb).unwrap().top, Val::Px(0.0));
    assert_eq!(app.world().get::<Node>(p.fill).unwrap().height, Val::Percent(100.0));
}

#[test]
fn before_layout_the_thumb_falls_back_to_a_percentage() {
    let mut app = app();
    let p = spawn_slider(&mut app, 0.5, ManagedSlider::new());
    app.world_mut().entity_mut(p.slider).insert(ComputedNode::default());
    app.update();
    assert_eq!(app.world().get::<Node>(p.thumb).unwrap().left, Val::Percent(50.0));
}

#[test]
fn a_value_change_is_written_back_and_announced() {
    let mut app = app();
    #[derive(Resource, Default)]
    struct Seen(Vec<(f32, bool)>);
    app.init_resource::<Seen>().add_observer(|c: On<SliderChanged>, mut seen: ResMut<Seen>| seen.0.push((c.value, c.is_final)));
    let p = spawn_slider(&mut app, 0.5, ManagedSlider::new());
    change(&mut app, p.slider, 0.7, false);
    assert_eq!(value(&app, p.slider), 0.7, "Bevy's Slider never writes this itself");
    assert_eq!(app.world().resource::<Seen>().0, vec![(0.7, false)]);
}

#[test]
fn an_unmanaged_slider_is_left_alone() {
    let mut app = app();
    let slider = app.world_mut().spawn((Slider::default(), SliderValue(0.5), Node::default())).id();
    change(&mut app, slider, 0.9, true);
    assert_eq!(value(&app, slider), 0.5, "only a ManagedSlider is written back");
}

#[test]
fn values_are_clamped_and_nan_is_ignored() {
    let mut app = app();
    let p = spawn_slider(&mut app, 0.5, ManagedSlider::new());
    change(&mut app, p.slider, 7.0, true);
    assert_eq!(value(&app, p.slider), 1.0);
    change(&mut app, p.slider, -3.0, true);
    assert_eq!(value(&app, p.slider), 0.0);
    change(&mut app, p.slider, f32::NAN, true);
    assert_eq!(value(&app, p.slider), 0.0, "NaN never reaches SliderValue");
    change(&mut app, p.slider, f32::INFINITY, true);
    assert_eq!(value(&app, p.slider), 1.0, "infinity clamps to the end");

    // The pure helpers never panic on a broken range (f32::clamp would).
    assert_eq!(clamp_to_range(0.5, &SliderRange::from_range(1.0..=0.0)), Some(0.5));
    assert_eq!(clamp_to_range(0.5, &SliderRange::from_range(f32::NAN..=1.0)), None);
    assert_eq!(slider_fraction(f32::NAN, &SliderRange::new(0.0, 1.0)), 0.0);
    assert_eq!(slider_fraction(0.5, &SliderRange::from_range(2.0..=2.0)), 0.0);
}

#[test]
fn snapping_lands_on_the_exact_grid_float() {
    let mut app = app();
    let p = spawn_slider(&mut app, 0.5, ManagedSlider::snapped());
    change(&mut app, p.slider, 0.8999, true);
    assert_eq!(value(&app, p.slider), 0.9, "0.9 exactly, not 0.90000004");
    assert_eq!(sanitize_slider_value(0.37, &SliderRange::new(0.0, 1.0), Some(0.05), true), Some(0.35));
    assert_eq!(sanitize_slider_value(0.37, &SliderRange::new(0.0, 1.0), Some(f32::NAN), true), Some(0.37), "a broken step disables snapping");
}

#[test]
fn step_slider_moves_by_steps_and_clamps() {
    let mut app = app();
    let p = spawn_slider(&mut app, 0.5, ManagedSlider::snapped());
    app.world_mut().trigger(StepSlider { entity: p.slider, steps: 2.0 });
    app.update();
    assert_eq!(value(&app, p.slider), 0.6);
    app.world_mut().trigger(StepSlider { entity: p.slider, steps: -100.0 });
    app.update();
    assert_eq!(value(&app, p.slider), 0.0);
    app.world_mut().trigger(StepSlider { entity: p.slider, steps: f32::NAN });
    app.update();
    assert_eq!(value(&app, p.slider), 0.0, "a NaN step is ignored");
}

/// Real keys, through Bevy's focus dispatch and Bevy's own slider key handling plus the kit's.
#[test]
fn focused_keyboard_steps_through_bevy_and_the_kit() {
    let mut app = app();
    app.add_plugins((SliderPlugin, InputDispatchPlugin)).init_resource::<bevy::ui::UiScale>();
    let window = app.world_mut().spawn((Window::default(), PrimaryWindow)).id();
    let p = spawn_slider(&mut app, 0.5, ManagedSlider::snapped());
    app.insert_resource(InputFocus::from_entity(p.slider));
    let press = |app: &mut App, key: KeyCode| {
        app.world_mut().write_message(KeyboardInput {
            key_code: key,
            logical_key: Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified),
            state: ButtonState::Pressed,
            text: None,
            repeat: false,
            window,
        });
        app.update();
    };
    press(&mut app, KeyCode::ArrowRight); // Bevy's own handler
    assert_eq!(value(&app, p.slider), 0.55);
    press(&mut app, KeyCode::PageDown); // the kit: 10 steps
    assert_eq!(value(&app, p.slider), 0.05);
    press(&mut app, KeyCode::PageDown);
    assert_eq!(value(&app, p.slider), 0.0, "clamped at the start");
    press(&mut app, KeyCode::End);
    assert_eq!(value(&app, p.slider), 1.0);
    press(&mut app, KeyCode::ArrowUp); // horizontal slider: not the kit's key
    assert_eq!(value(&app, p.slider), 1.0);
    println!(">>> slider keys: ArrowRight/PageDown/End all landed on the grid");
}

#[derive(Component)]
struct PanelRoot;

#[derive(Resource, Default)]
struct Rebuilds(u32);

/// A panel that rebuilds on every frame it is allowed to - the worst case for a held slider.
fn rebuild_panel(mut commands: Commands, roots: Query<Entity, With<PanelRoot>>, mut n: ResMut<Rebuilds>) {
    for e in &roots {
        commands.entity(e).despawn();
    }
    n.0 += 1;
    commands.spawn((PanelRoot, Node::default())).with_children(|p| {
        p.spawn((Slider::default(), SliderValue(0.5), ManagedSlider::new(), Node::default()));
    });
}

fn panel_slider(app: &mut App) -> Entity {
    let w = app.world_mut();
    w.query_filtered::<Entity, With<Slider>>().single(w).unwrap()
}

#[test]
fn a_held_slider_is_not_rebuilt_and_release_is_announced() {
    let mut app = app();
    app.init_resource::<Rebuilds>().add_systems(Update, rebuild_panel.after(UiKitSystems::Sliders).run_if(not(any_slider_held)));
    app.update();
    let held = panel_slider(&mut app);

    // A track click: `Pressed` but not yet dragging - the one-frame window that is easy to miss.
    app.world_mut().entity_mut(held).insert(Pressed);
    app.update();
    assert_eq!(panel_slider(&mut app), held, "a pressed slider was despawned");
    assert!(app.world().get::<SliderHeld>(held).is_some());

    // A real drag.
    let mut drag = SliderDragState::default();
    drag.dragging = true;
    app.world_mut().entity_mut(held).remove::<Pressed>().insert(drag);
    let before = app.world().resource::<Rebuilds>().0;
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(app.world().resource::<Rebuilds>().0, before, "no rebuild mid-drag");
    assert_eq!(panel_slider(&mut app), held);

    // Release: the message arrives, the panel rebuilds again.
    app.world_mut().entity_mut(held).insert(SliderDragState::default());
    app.update();
    let released: Vec<SliderReleased> = app.world_mut().resource_mut::<Messages<SliderReleased>>().drain().collect();
    assert_eq!(released, vec![SliderReleased { entity: held, value: 0.5 }]);
    app.update();
    assert_ne!(panel_slider(&mut app), held, "released: rebuilding resumes");
}

/// The gamepad D-pad, through Bevy's focus dispatch.
#[test]
fn focused_dpad_steps_the_slider() {
    use bevy::input::gamepad::{GamepadButton, GamepadButtonChangedEvent};
    let mut app = app();
    app.add_plugins((SliderPlugin, InputDispatchPlugin));
    app.world_mut().spawn((Window::default(), PrimaryWindow));
    let pad = app.world_mut().spawn_empty().id();
    let p = spawn_slider(&mut app, 0.5, ManagedSlider::snapped());
    app.insert_resource(InputFocus::from_entity(p.slider));
    let press = |app: &mut App, button: GamepadButton| {
        app.world_mut().write_message(GamepadButtonChangedEvent::new(pad, button, ButtonState::Pressed, 1.0));
        app.update();
    };
    press(&mut app, GamepadButton::DPadRight);
    assert_eq!(value(&app, p.slider), 0.55);
    press(&mut app, GamepadButton::DPadLeft);
    press(&mut app, GamepadButton::DPadLeft);
    assert_eq!(value(&app, p.slider), 0.45);
    press(&mut app, GamepadButton::South);
    assert_eq!(value(&app, p.slider), 0.45, "other buttons do not move the slider");
}
