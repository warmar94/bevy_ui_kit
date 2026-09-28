//! A long list in a scroll area: wheel over it, drag the bar, and see the offset survive rebuilds.
//!
//! Keys: `R` rebuilds the whole panel (the list keeps its place), `Tab` switches between two
//! pages (each page is its own area and starts at the top), `C` closes / reopens the panel (a
//! reopened panel starts at the top).
//!
//! The kit has no look: every colour and size below is this example's own, kept plain on purpose.

use bevy::prelude::*;
use bevy_ui_kit::*;

const INK: Color = Color::srgb(0.9, 0.9, 0.9);
const TRACK: Color = Color::srgb(0.25, 0.25, 0.25);
const THUMB: Color = Color::srgb(0.6, 0.6, 0.6);
const PANEL: Color = Color::srgb(0.12, 0.12, 0.12);

#[derive(Resource)]
struct View {
    page: u64,
    open: bool,
}

#[derive(Component)]
struct PanelRoot;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, UiKitPlugin::default()))
        .insert_resource(View { page: 0, open: true })
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(Camera2d);
        })
        // Rebuild AFTER the kit's scroll set, so the new list restores this frame's offset.
        .add_systems(Update, (keys, rebuild.after(UiKitSystems::Scroll)).chain())
        .run();
}

fn keys(input: Res<ButtonInput<KeyCode>>, mut view: ResMut<View>) {
    if input.just_pressed(KeyCode::KeyR) {
        view.set_changed();
    }
    if input.just_pressed(KeyCode::Tab) {
        view.page = 1 - view.page;
    }
    if input.just_pressed(KeyCode::KeyC) {
        view.open = !view.open;
    }
}

fn rebuild(mut commands: Commands, view: Res<View>, roots: Query<Entity, With<PanelRoot>>) {
    if !view.is_changed() {
        return;
    }
    for e in &roots {
        commands.entity(e).despawn();
    }
    if !view.open {
        return;
    }
    let page = view.page;
    commands
        .spawn((
            PanelRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .with_children(|root| {
            root.spawn((
                Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(8.0), padding: UiRect::all(Val::Px(12.0)), width: Val::Px(360.0), ..default() },
                BackgroundColor(PANEL),
            ))
            .with_children(|panel| {
                panel.spawn((Text::new(format!("Page {} - R rebuild, Tab page, C close", page + 1)), TextFont::from_font_size(16.0), TextColor(INK)));
                let look = ScrollAreaLook {
                    row: Node { column_gap: Val::Px(4.0), ..default() },
                    // A bounded height, or the list just grows and never scrolls.
                    scroller: Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, max_height: Val::Px(320.0), ..default() },
                    bar: (Node { width: Val::Px(8.0), ..default() }, BackgroundColor(TRACK), AutoHideScrollbar),
                    thumb: BackgroundColor(THUMB),
                    min_thumb_length: 24.0,
                };
                spawn_scroll_area(panel, ScrollKey::page("example.list", page), look, |list| {
                    for i in 0..200 {
                        list.spawn((Text::new(format!("page {} / row {i}", page + 1)), TextFont::from_font_size(14.0), TextColor(INK)));
                    }
                });
            });
        });
}
