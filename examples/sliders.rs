//! Bevy's headless `Slider`, made to work: drag, click the track, or click a slider and use the
//! arrow keys, PageUp / PageDown, Home / End or a gamepad D-pad.
//!
//! The panel is rebuilt from the settings on EVERY change, the way many games build menus, and it
//! still never despawns the slider under your mouse: the rebuild is skipped while one is held and
//! runs once on release.
//!
//! The kit has no look: every colour and size below is this example's own, kept plain on purpose.

use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::ui_widgets::{Slider, SliderOrientation, SliderRange, SliderStep, SliderThumb, SliderValue, TrackClick};
use bevy_ui_kit::*;

const INK: Color = Color::srgb(0.9, 0.9, 0.9);
const TRACK: Color = Color::srgb(0.25, 0.25, 0.25);
const FILL: Color = Color::srgb(0.5, 0.5, 0.5);
const KNOB: Color = Color::srgb(0.85, 0.85, 0.85);

const LEN: f32 = 240.0;
const KNOB_PX: f32 = 16.0;

/// What the sliders edit (a game's settings resource).
#[derive(Resource)]
struct Settings {
    values: [f32; 3],
}

/// Which setting a slider edits.
#[derive(Component, Clone, Copy)]
struct Field(usize);

#[derive(Component)]
struct PanelRoot;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, UiKitPlugin::default()))
        .insert_resource(Settings { values: [0.8, 0.5, 0.25] })
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(Camera2d);
        })
        .add_observer(store_value)
        .add_observer(focus_on_press)
        .add_systems(Update, rebuild.after(UiKitSystems::Sliders).run_if(not(any_slider_held)))
        .run();
}

/// The kit wrote the (clamped, snapped) value back; store it in the settings.
fn store_value(changed: On<SliderChanged>, fields: Query<&Field>, mut settings: ResMut<Settings>) {
    if let Ok(field) = fields.get(changed.entity) {
        settings.values[field.0] = changed.value;
    }
}

/// Keyboard / gamepad steps go to the focused slider: focus it when it is clicked.
fn focus_on_press(press: On<Pointer<Press>>, sliders: Query<(), With<Slider>>, mut focus: ResMut<InputFocus>) {
    if sliders.contains(press.entity) {
        *focus = InputFocus::from_entity(press.entity);
    }
}

/// Rebuild on any settings change (skipped while a slider is held) and once on release.
fn rebuild(
    mut commands: Commands,
    settings: Res<Settings>,
    mut released: MessageReader<SliderReleased>,
    roots: Query<Entity, With<PanelRoot>>,
    fields: Query<&Field>,
    mut focus: ResMut<InputFocus>,
) {
    let released = released.read().count() > 0;
    if !(settings.is_changed() || released) {
        return;
    }
    // The rebuild makes new entities: remember which field had keyboard focus.
    let focused = focus.get().and_then(|e| fields.get(e).ok()).map_or(0, |f| f.0);
    for e in &roots {
        commands.entity(e).despawn();
    }
    let mut spawned = [Entity::PLACEHOLDER; 3];
    commands
        .spawn((
            PanelRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                column_gap: Val::Px(40.0),
                ..default()
            },
        ))
        .with_children(|root| {
            root.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(16.0), ..default() }).with_children(|col| {
                for (i, name) in ["Master", "Music"].iter().enumerate() {
                    col.spawn(Node { column_gap: Val::Px(12.0), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                        row.spawn((Text::new(*name), TextColor(INK), Node { width: Val::Px(70.0), ..default() }));
                        spawned[i] = slider(row, Field(i), settings.values[i], SliderOrientation::Horizontal);
                        row.spawn((Text::new(format!("{:.0}%", settings.values[i] * 100.0)), TextColor(INK)));
                    });
                }
            });
            root.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: Val::Px(8.0), ..default() }).with_children(
                |col| {
                    spawned[2] = slider(col, Field(2), settings.values[2], SliderOrientation::Vertical);
                    col.spawn((Text::new(format!("Height {:.0}%", settings.values[2] * 100.0)), TextColor(INK)));
                },
            );
        });
    if let Some(e) = spawned.get(focused) {
        *focus = InputFocus::from_entity(*e);
    }
}

/// One slider: the kit's behaviour + this example's plain look.
fn slider(parent: &mut ChildSpawnerCommands, field: Field, value: f32, orientation: SliderOrientation) -> Entity {
    let vertical = orientation == SliderOrientation::Vertical;
    let (w, h) = if vertical { (KNOB_PX, LEN) } else { (LEN, KNOB_PX) };
    parent
        .spawn((
            Slider { track_click: TrackClick::Snap, orientation },
            SliderValue(value),
            SliderRange::new(0.0, 1.0),
            SliderStep(0.05),
            ManagedSlider::snapped(),
            field,
            Node { width: Val::Px(w), height: Val::Px(h), ..default() },
        ))
        .with_children(|s| {
            // The track: absolute, full size. The fill grows from the bottom on the vertical one.
            s.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    flex_direction: if vertical { FlexDirection::ColumnReverse } else { FlexDirection::Row },
                    ..default()
                },
                BackgroundColor(TRACK),
            ))
            .with_children(|t| {
                t.spawn((SliderFill, Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(FILL)));
            });
            s.spawn((
                SliderThumb,
                Node { position_type: PositionType::Absolute, width: Val::Px(KNOB_PX), height: Val::Px(KNOB_PX), ..default() },
                BackgroundColor(KNOB),
            ));
        })
        .id()
}
