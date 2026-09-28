//! The same kit under two completely different looks, side by side: a dark, thin, gold-on-brown
//! menu and a bright, chunky, rounded cartoon menu. One function builds both panels; only the
//! `Theme` value passed in differs. The kit's behaviour (scrolling, sliders, squash, pop-in) is
//! identical in both, because the kit has no look of its own.
//!
//! Keys: `R` rebuilds both panels (each list keeps its own place: different scroll keys).

use bevy::prelude::*;
use bevy::ui_widgets::{ScrollbarThumb, Slider, SliderRange, SliderStep, SliderThumb, SliderValue, TrackClick};
use bevy_ui_kit::*;

/// Everything visual lives here, in the example, never in the kit.
struct Theme {
    name: &'static str,
    panel: Color,
    panel_border: Color,
    ink: Color,
    accent: Color,
    track: Color,
    radius: f32,
    border: f32,
    padding: f32,
    font_px: f32,
    bar_px: f32,
    knob_px: f32,
    row_px: f32,
    pop: PopSpec,
    squash: SquashSpec,
}

const DARK: Theme = Theme {
    name: "Dusk Keep",
    panel: Color::srgb(0.10, 0.08, 0.06),
    panel_border: Color::srgb(0.55, 0.43, 0.20),
    ink: Color::srgb(0.86, 0.76, 0.52),
    accent: Color::srgb(0.70, 0.55, 0.25),
    track: Color::srgb(0.20, 0.16, 0.11),
    radius: 2.0,
    border: 1.0,
    padding: 10.0,
    font_px: 14.0,
    bar_px: 4.0,
    knob_px: 10.0,
    row_px: 22.0,
    pop: PopSpec::new(0.18, 0.96, 1.0),
    squash: SquashSpec { pressed_scale: Vec2::new(0.98, 0.98), rate: 40.0 },
};

const BRIGHT: Theme = Theme {
    name: "SUNNY SIDE!",
    panel: Color::srgb(0.30, 0.62, 0.98),
    panel_border: Color::srgb(0.10, 0.25, 0.60),
    ink: Color::srgb(1.0, 1.0, 1.0),
    accent: Color::srgb(1.0, 0.80, 0.10),
    track: Color::srgb(0.12, 0.35, 0.75),
    radius: 22.0,
    border: 5.0,
    padding: 20.0,
    font_px: 22.0,
    bar_px: 16.0,
    knob_px: 30.0,
    row_px: 40.0,
    pop: PopSpec::new(0.35, 0.4, 1.18),
    squash: SquashSpec { pressed_scale: Vec2::new(1.12, 0.82), rate: 18.0 },
};

#[derive(Component)]
struct PanelRoot;

#[derive(Resource)]
struct Rebuild;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, UiKitPlugin::default()))
        .insert_resource(Rebuild)
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(Camera2d);
        })
        .add_systems(Update, (keys, build.after(UiKitSystems::Scroll).run_if(not(any_slider_held))).chain())
        .run();
}

fn keys(input: Res<ButtonInput<KeyCode>>, mut rebuild: ResMut<Rebuild>) {
    if input.just_pressed(KeyCode::KeyR) {
        rebuild.set_changed();
    }
}

fn build(mut commands: Commands, rebuild: Res<Rebuild>, roots: Query<Entity, With<PanelRoot>>) {
    if !rebuild.is_changed() {
        return;
    }
    for e in &roots {
        commands.entity(e).despawn();
    }
    commands
        .spawn((
            PanelRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::SpaceEvenly,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgb(0.05, 0.05, 0.05)),
        ))
        .with_children(|root| {
            panel(root, &DARK, "dark");
            panel(root, &BRIGHT, "bright");
        });
}

/// ONE panel builder. The kit calls are identical for both themes.
fn panel(parent: &mut ChildSpawnerCommands, t: &Theme, key: &'static str) {
    let text = |s: String, px: f32| (Text::new(s), TextFont::from_font_size(px), TextColor(t.ink));
    parent
        .spawn((
            PopIn::new(t.pop),
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(t.padding * 0.6),
                padding: UiRect::all(Val::Px(t.padding)),
                border: UiRect::all(Val::Px(t.border)),
                border_radius: BorderRadius::all(Val::Px(t.radius)),
                width: Val::Px(340.0),
                ..default()
            },
            BackgroundColor(t.panel),
            BorderColor::all(t.panel_border),
        ))
        .with_children(|p| {
            p.spawn(text(t.name.to_string(), t.font_px * 1.4));

            // A scroll area: the same helper, different bundles.
            let look = ScrollAreaLook {
                row: Node { column_gap: Val::Px(t.padding * 0.5), ..default() },
                scroller: Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, max_height: Val::Px(200.0), ..default() },
                bar: (Node { width: Val::Px(t.bar_px), border_radius: BorderRadius::all(Val::Px(t.bar_px * 0.5)), ..default() }, BackgroundColor(t.track)),
                // Bevy's scrollbar sizes the thumb; its corner radius rides on `ScrollbarThumb` itself.
                thumb: (ScrollbarThumb { border_radius: BorderRadius::all(Val::Px(t.bar_px * 0.5)), ..default() }, BackgroundColor(t.accent)),
                min_thumb_length: t.bar_px * 3.0,
            };
            spawn_scroll_area(p, ScrollKey::new(key), look, |list| {
                for i in 0..40 {
                    list.spawn((text(format!("Quest {}", i + 1), t.font_px), Node { min_height: Val::Px(t.row_px), ..default() }));
                }
            });

            // A slider: the same kit components, different nodes.
            p.spawn((
                Slider { track_click: TrackClick::Snap, ..default() },
                SliderValue(0.6),
                SliderRange::new(0.0, 1.0),
                SliderStep(0.1),
                ManagedSlider::snapped(),
                Node { width: Val::Percent(100.0), height: Val::Px(t.knob_px), align_items: AlignItems::Center, ..default() },
            ))
            .with_children(|s| {
                s.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Percent(100.0),
                        height: Val::Px(t.bar_px),
                        border_radius: BorderRadius::all(Val::Px(t.bar_px * 0.5)),
                        ..default()
                    },
                    BackgroundColor(t.track),
                ))
                .with_children(|track| {
                    track.spawn((
                        SliderFill,
                        Node { height: Val::Percent(100.0), border_radius: BorderRadius::all(Val::Px(t.bar_px * 0.5)), ..default() },
                        BackgroundColor(t.accent),
                    ));
                });
                s.spawn((
                    SliderThumb,
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Px(t.knob_px),
                        height: Val::Px(t.knob_px),
                        border: UiRect::all(Val::Px(t.border)),
                        border_radius: BorderRadius::all(Val::Px(t.knob_px * 0.5)),
                        ..default()
                    },
                    BackgroundColor(t.ink),
                    BorderColor::all(t.panel_border),
                ));
            });

            // A button with the same press squash, tuned per theme.
            p.spawn((
                Button,
                PressSquash::new(t.squash),
                Node {
                    padding: UiRect::all(Val::Px(t.padding * 0.6)),
                    justify_content: JustifyContent::Center,
                    border_radius: BorderRadius::all(Val::Px(t.radius)),
                    ..default()
                },
                BackgroundColor(t.accent),
            ))
            .with_children(|b| {
                b.spawn(text("Press me".to_string(), t.font_px));
            });
        });
}
