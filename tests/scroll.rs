//! Scroll areas: wiring, the wheel, the offset memory.

mod common;

use bevy::prelude::*;
use bevy::ui::RelativeCursorPosition;
use bevy::ui_widgets::{ControlOrientation, Scrollbar, ScrollbarThumb};
use bevy_ui_kit::*;
use common::*;

/// Which area a stand-in panel shows; changing it (or touching it) rebuilds the panel.
#[derive(Resource, Clone)]
struct Shown(Option<ScrollKey>);

#[derive(Component)]
struct PanelRoot;

/// A deliberately plain look: only layout, no colours (the kit must not need any).
fn look() -> ScrollAreaLook<Node, Node, Node, ()> {
    ScrollAreaLook {
        row: Node::default(),
        scroller: Node { flex_direction: FlexDirection::Column, max_height: Val::Px(100.0), ..default() },
        bar: Node { width: Val::Px(6.0), ..default() },
        thumb: (),
        min_thumb_length: 12.0,
    }
}

/// The shape of every real panel: despawn and rebuild the whole body on any change.
fn rebuild(mut commands: Commands, shown: Res<Shown>, roots: Query<Entity, With<PanelRoot>>) {
    if !shown.is_changed() {
        return;
    }
    for e in &roots {
        commands.entity(e).despawn();
    }
    let Some(key) = shown.0.clone() else { return };
    commands.spawn((PanelRoot, Node::default())).with_children(|p| {
        spawn_scroll_area(p, key, look(), |list| {
            for i in 0..50 {
                list.spawn(Text::new(format!("row {i}")));
            }
        });
    });
}

fn panel_app(key: ScrollKey) -> App {
    let mut app = app();
    app.insert_resource(Shown(Some(key))).add_systems(Update, rebuild.after(UiKitSystems::Scroll));
    app.update();
    app
}

fn scroller(app: &mut App) -> Entity {
    let w = app.world_mut();
    w.query_filtered::<Entity, With<ScrollKey>>().single(w).expect("exactly one keyed scroll area")
}

fn offset(app: &mut App) -> f32 {
    let e = scroller(app);
    app.world().get::<ScrollPosition>(e).expect("a scroll position").y
}

#[test]
fn the_helper_wires_the_bar_to_the_scroller() {
    let mut app = panel_app(ScrollKey::new("list"));
    let body = scroller(&mut app);
    let bar = check_scroll_wiring(app.world(), body).expect("wired");
    let sb = app.world().get::<Scrollbar>(bar).unwrap();
    assert_eq!(sb.target, body, "the raw Entity in Scrollbar must be the scroller");
    assert_eq!(sb.orientation, ControlOrientation::Vertical);
    assert_eq!(sb.min_thumb_length, 12.0);
    // Siblings in one row: the bar reserves its own width instead of covering the rows.
    assert_eq!(app.world().get::<ChildOf>(bar).map(ChildOf::parent), app.world().get::<ChildOf>(body).map(ChildOf::parent));
    // The kit added only behaviour to the scroller's node.
    let node = app.world().get::<Node>(body).unwrap();
    assert_eq!(node.overflow.y, OverflowAxis::Scroll);
    assert_eq!(node.min_height, Val::Px(0.0));
    assert_eq!(node.max_height, Val::Px(100.0), "the game's layout is kept");
    // And NO look anywhere: nothing it spawned has a colour, border colour or image.
    let mut q = app.world_mut().query::<(Option<&BackgroundColor>, Option<&BorderColor>, Option<&ImageNode>)>();
    let styled =
        q.iter(app.world()).filter(|(bg, bc, img)| bg.is_some_and(|c| c.0 != Color::NONE) || bc.is_some_and(|b| b.top != Color::NONE) || img.is_some()).count();
    assert_eq!(styled, 0, "the kit must not add a colour, border or image of its own");
    println!(">>> scroll wiring: bar {bar} -> scroller {body}");
}

#[test]
fn a_bar_aimed_at_the_wrong_entity_is_caught() {
    let mut app = app();
    let wrong = app.world_mut().spawn(Node::default()).id();
    let body = app.world_mut().spawn((Node::default(), Scroller)).id();
    app.world_mut().spawn((Node::default(), Scrollbar::new(wrong, ControlOrientation::Vertical, 8.0))).with_children(|b| {
        b.spawn(ScrollbarThumb::default());
    });
    assert_eq!(check_scroll_wiring(app.world(), body), Err(ScrollWiringError::NoScrollbar(1)));
    // Aim it right, then give it a thumb that is a grandchild instead of a child.
    let bar = app.world_mut().spawn((Node::default(), Scrollbar::new(body, ControlOrientation::Vertical, 8.0))).id();
    let _ = app.world_mut().spawn((Node::default(), ChildOf(bar))).with_children(|g| {
        g.spawn(ScrollbarThumb::default());
    });
    assert_eq!(check_scroll_wiring(app.world(), body), Err(ScrollWiringError::NoThumb));
    let plain = app.world_mut().spawn(Node::default()).id();
    assert_eq!(check_scroll_wiring(app.world(), plain), Err(ScrollWiringError::NotScrollable));
}

#[test]
fn the_wheel_moves_the_offset_and_never_above_the_top() {
    let mut app = panel_app(ScrollKey::new("list"));
    let line = app.world().resource::<ScrollConfig>().line_px;
    wheel(&mut app, -3.0); // wheel DOWN
    assert_eq!(offset(&mut app), 3.0 * line);
    wheel(&mut app, 10.0); // far back up
    assert_eq!(offset(&mut app), 0.0, "never above the top");
}

#[test]
fn the_wheel_clamps_at_the_bottom_once_laid_out() {
    let mut app = panel_app(ScrollKey::new("list"));
    let body = scroller(&mut app);
    app.world_mut().entity_mut(body).insert(laid_out(Vec2::new(100.0, 200.0), Vec2::new(100.0, 500.0)));
    wheel(&mut app, -1000.0);
    assert_eq!(offset(&mut app), 300.0, "content 500 - visible 200");
    assert_eq!(max_scroll_offset(&laid_out(Vec2::new(100.0, 200.0), Vec2::new(100.0, 500.0))), Some(Vec2::new(0.0, 300.0)));
    assert_eq!(max_scroll_offset(&ComputedNode::default()), None, "not laid out: no clamp yet");
}

#[test]
fn the_wheel_scrolls_the_hovered_area_only() {
    let mut app = app();
    let a = app.world_mut().spawn((Node::default(), Scroller)).id();
    let b = app.world_mut().spawn((Node::default(), Scroller)).id();
    let pos = |app: &App, e: Entity| app.world().get::<ScrollPosition>(e).unwrap().y;

    wheel(&mut app, -2.0);
    assert_eq!((pos(&app, a), pos(&app, b)), (0.0, 0.0), "two areas, none hovered: neither scrolls");

    app.world_mut().get_mut::<RelativeCursorPosition>(b).unwrap().cursor_over = true;
    wheel(&mut app, -2.0);
    assert_eq!(pos(&app, a), 0.0, "not the hovered one");
    assert!(pos(&app, b) > 0.0, "the hovered one");
}

#[test]
fn nested_hovered_areas_scroll_only_the_innermost() {
    let mut app = app();
    let outer = app.world_mut().spawn((Node::default(), Scroller)).id();
    let inner = app.world_mut().spawn((Node::default(), Scroller, ChildOf(outer))).id();
    for e in [outer, inner] {
        app.world_mut().get_mut::<RelativeCursorPosition>(e).unwrap().cursor_over = true;
    }
    wheel(&mut app, -1.0);
    assert_eq!(app.world().get::<ScrollPosition>(outer).unwrap().y, 0.0);
    assert!(app.world().get::<ScrollPosition>(inner).unwrap().y > 0.0);
}

#[test]
fn a_lone_area_takes_the_wheel_unless_configured_off() {
    let mut app = app();
    let only = app.world_mut().spawn((Node::default(), Scroller)).id();
    wheel(&mut app, -1.0);
    assert!(app.world().get::<ScrollPosition>(only).unwrap().y > 0.0, "the only area on screen scrolls");

    let mut app = app_with(UiKitPlugin { scroll: ScrollConfig { lone_area_takes_wheel: false, ..default() }, ..default() });
    let only = app.world_mut().spawn((Node::default(), Scroller)).id();
    wheel(&mut app, -1.0);
    assert_eq!(app.world().get::<ScrollPosition>(only).unwrap().y, 0.0);
}

#[test]
fn the_offset_survives_a_rebuild_with_the_same_key() {
    let mut app = panel_app(ScrollKey::page("inv", 0));
    let first = scroller(&mut app);
    wheel(&mut app, -4.0);
    let before = offset(&mut app);
    assert!(before > 0.0);

    // Despawn + respawn in place (like a click that rebuilds the panel).
    app.world_mut().resource_mut::<Shown>().set_changed();
    app.update();
    let second = scroller(&mut app);
    assert_ne!(first, second, "the panel really was rebuilt");
    assert_eq!(offset(&mut app), before, "a rebuild threw the list back to the top");
    println!(">>> scroll memory: offset {before} kept across {first} -> {second}");
}

#[test]
fn a_new_page_or_a_reopen_starts_at_the_top() {
    let mut app = panel_app(ScrollKey::page("inv", 0));
    wheel(&mut app, -4.0);
    assert!(offset(&mut app) > 0.0);

    app.world_mut().resource_mut::<Shown>().0 = Some(ScrollKey::page("inv", 1));
    app.update();
    assert_eq!(offset(&mut app), 0.0, "a new page starts at the top");

    // Close for a frame (the key is gone when the memory is refreshed), then reopen page 1.
    wheel(&mut app, -2.0);
    app.world_mut().resource_mut::<Shown>().0 = None;
    app.update();
    app.update();
    assert!(app.world().resource::<ScrollMemory>().is_empty(), "a closed area is forgotten");
    app.world_mut().resource_mut::<Shown>().0 = Some(ScrollKey::page("inv", 1));
    app.update();
    assert_eq!(offset(&mut app), 0.0, "a fresh open starts at the top");
}

#[test]
fn a_seeded_offset_is_restored_and_bad_values_are_sanitized() {
    let mut app = app();
    let key = ScrollKey::new("seeded");
    app.world_mut().resource_mut::<ScrollMemory>().set(key.clone(), Vec2::new(0.0, 42.0));
    let e = app.world_mut().spawn((Node::default(), key.clone())).id();
    assert_eq!(app.world().get::<ScrollPosition>(e).unwrap().y, 42.0);
    assert!(app.world().get::<Scroller>(e).is_some(), "a key brings the scroller with it");

    let mut memory = ScrollMemory::default();
    memory.set(key.clone(), Vec2::new(f32::NAN, -5.0));
    assert_eq!(memory.offset(&key), Vec2::ZERO);
}

#[test]
fn auto_hide_hides_a_bar_with_nothing_to_scroll() {
    let mut app = app();
    let body = app.world_mut().spawn((Node::default(), Scroller, laid_out(Vec2::new(100.0, 200.0), Vec2::new(100.0, 150.0)))).id();
    let bar = app.world_mut().spawn((Node::default(), Scrollbar::new(body, ControlOrientation::Vertical, 8.0), AutoHideScrollbar)).id();
    app.update();
    assert_eq!(*app.world().get::<Visibility>(bar).unwrap(), Visibility::Hidden);
    app.world_mut().entity_mut(body).insert(laid_out(Vec2::new(100.0, 200.0), Vec2::new(100.0, 900.0)));
    app.update();
    assert_eq!(*app.world().get::<Visibility>(bar).unwrap(), Visibility::Inherited);
}
