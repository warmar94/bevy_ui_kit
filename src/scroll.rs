//! Scroll areas: a scroller node, a draggable scrollbar as its flex sibling, the mouse wheel, and
//! the scroll offset kept across rebuilds.
//!
//! Bevy lays out `Overflow::scroll_y()` and honours [`ScrollPosition`], but nothing in Bevy writes
//! that position from the mouse wheel unless you opt into its picking-based `ScrollArea` marker.
//! This module adds the wheel, the bar wiring and a memory, and nothing else: every node it spawns
//! gets its look from the bundles you pass in.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;

use bevy_camera::visibility::Visibility;
use bevy_ecs::hierarchy::{ChildOf, ChildSpawnerCommands, Children};
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::world::DeferredWorld;
use bevy_input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy_math::Vec2;
use bevy_ui::{ComputedNode, Display, Node, OverflowAxis, RelativeCursorPosition, ScrollPosition, Val};
use bevy_ui_widgets::{ControlOrientation, Scrollbar, ScrollbarThumb};

/// How the wheel scrolls. Inserted by [`UiKitPlugin`](crate::UiKitPlugin) from its `scroll` field;
/// change the resource at runtime to change the behaviour.
#[derive(Resource, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize", serde(default))]
pub struct ScrollConfig {
    /// Logical pixels scrolled per wheel "line" (a notched mouse wheel reports lines; a touchpad
    /// reports pixels, which are used as they are). Default: Bevy's own
    /// `MouseScrollUnit::SCROLL_UNIT_CONVERSION_FACTOR`, the same step Bevy's `ScrollArea` uses.
    pub line_px: f32,
    /// Multiplier for pixel-unit deltas (touchpads). Default `1.0`.
    pub pixel_scale: f32,
    /// With the cursor over no scroller and exactly ONE shown scroller in the world, the wheel
    /// scrolls that one (a full-screen list or a single settings page). "Shown" means the
    /// scroller's OWN `Visibility` is not `Hidden` and its `display` is not `None`; a scroller
    /// inside a hidden parent still counts, so despawn closed panels (or turn this off) if you
    /// keep several around. Default `true`.
    pub lone_area_takes_wheel: bool,
}

impl Default for ScrollConfig {
    fn default() -> Self {
        Self { line_px: MouseScrollUnit::SCROLL_UNIT_CONVERSION_FACTOR, pixel_scale: 1.0, lone_area_takes_wheel: true }
    }
}

/// Marks a node as a scroll area driven by this crate: the wheel scrolls it while the cursor is
/// over it, and the offset is clamped to its laid-out content.
///
/// Inserting it (with the node's `Node` in the same bundle, or already on the entity) sets
/// `overflow.y` to [`OverflowAxis::Scroll`] and, when `min_height` is `Auto`, sets it to zero so
/// the node can shrink below its content inside a flex layout (without both, nothing ever
/// scrolls). Nothing visual is touched. Requires [`ScrollPosition`] and
/// [`RelativeCursorPosition`] (how the wheel knows the cursor is over it).
///
/// A node whose `overflow.x` is `Scroll` also scrolls sideways from a horizontal wheel delta.
///
/// Do not add Bevy's own `bevy::ui_widgets::ScrollArea` marker to the same node: both would
/// scroll it and every notch would move twice.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[require(ScrollPosition, RelativeCursorPosition)]
#[component(on_insert = scroller_on_insert)]
pub struct Scroller;

fn scroller_on_insert(mut world: DeferredWorld, ctx: HookContext) {
    if let Some(mut node) = world.get_mut::<Node>(ctx.entity) {
        if node.overflow.y != OverflowAxis::Scroll {
            node.overflow.y = OverflowAxis::Scroll;
        }
        if node.min_height == Val::Auto {
            node.min_height = Val::Px(0.0);
        }
    }
}

/// Identifies one scroll area so its offset survives the area being despawned and spawned again
/// (a panel that rebuilds its whole body when its data changes).
///
/// `name` names the surface (`"inventory"`), `page` what it shows (a tab index, a container id):
/// a different page is a different area, so switching tabs starts the new one at the top. The key
/// is yours; nothing in this crate interprets it. Inserting it restores the remembered offset
/// from [`ScrollMemory`] (if there is one) and requires [`Scroller`].
#[derive(Component, Clone, Debug, PartialEq, Eq, Hash)]
#[require(Scroller)]
#[component(on_insert = scroll_key_on_insert)]
pub struct ScrollKey {
    /// The surface.
    pub name: Cow<'static, str>,
    /// Which page / tab / instance of that surface.
    pub page: u64,
}

impl ScrollKey {
    /// A single-page area.
    pub fn new(name: impl Into<Cow<'static, str>>) -> Self {
        Self { name: name.into(), page: 0 }
    }

    /// One page (tab, instance) of a surface.
    pub fn page(name: impl Into<Cow<'static, str>>, page: u64) -> Self {
        Self { name: name.into(), page }
    }
}

impl fmt::Display for ScrollKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#{}", self.name, self.page)
    }
}

fn scroll_key_on_insert(mut world: DeferredWorld, ctx: HookContext) {
    let Some(key) = world.get::<ScrollKey>(ctx.entity) else { return };
    let Some(offset) = world.get_resource::<ScrollMemory>().and_then(|m| m.get(key)) else { return };
    if let Some(mut pos) = world.get_mut::<ScrollPosition>(ctx.entity) {
        pos.0 = offset;
    }
}

/// Where every live [`ScrollKey`]ed area is scrolled to.
///
/// [`remember_scroll`] mirrors the offset of every area that exists into this map each frame and
/// drops the keys that no longer exist, so:
///
/// - a panel that despawns and respawns its area within one frame (a rebuild) keeps its place;
/// - an area whose key was absent when [`remember_scroll`] ran (the panel was closed) is
///   forgotten, so opening it again starts at the top;
/// - a new `page` is a new key, so a new tab starts at the top.
///
/// You can also read it, or seed an offset with [`ScrollMemory::set`] before spawning.
#[derive(Resource, Default, Debug, Clone)]
pub struct ScrollMemory {
    offsets: HashMap<ScrollKey, Vec2>,
}

impl ScrollMemory {
    /// The remembered offset of `key`, if it has one. Never returns a non-finite or negative value.
    pub fn get(&self, key: &ScrollKey) -> Option<Vec2> {
        self.offsets.get(key).map(|v| sanitize_offset(*v))
    }

    /// The remembered offset of `key`, or the top (`Vec2::ZERO`).
    pub fn offset(&self, key: &ScrollKey) -> Vec2 {
        self.get(key).unwrap_or(Vec2::ZERO)
    }

    /// Remember `offset` for `key` (applied the next time an area with that key is inserted).
    /// Non-finite components are stored as zero.
    pub fn set(&mut self, key: ScrollKey, offset: Vec2) {
        self.offsets.insert(key, sanitize_offset(offset));
    }

    /// Forget `key`.
    pub fn forget(&mut self, key: &ScrollKey) {
        self.offsets.remove(key);
    }

    /// Forget everything.
    pub fn clear(&mut self) {
        self.offsets.clear();
    }

    /// How many areas are remembered.
    pub fn len(&self) -> usize {
        self.offsets.len()
    }

    /// Nothing remembered?
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }

    /// Every remembered key and offset (in no particular order).
    pub fn iter(&self) -> impl Iterator<Item = (&ScrollKey, Vec2)> {
        self.offsets.iter().map(|(k, v)| (k, sanitize_offset(*v)))
    }
}

fn sanitize_offset(v: Vec2) -> Vec2 {
    let f = |x: f32| if x.is_finite() { x.max(0.0) } else { 0.0 };
    Vec2::new(f(v.x), f(v.y))
}

/// Put on a [`Scrollbar`] entity: the bar gets `Visibility::Hidden` while its target has nothing
/// to scroll (its content fits), and `Visibility::Inherited` otherwise. The bar keeps its space in
/// the layout either way. Optional.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AutoHideScrollbar;

/// The bundles that give a scroll area its look, for [`spawn_scroll_area`]. Every field is
/// yours: pass a `Node` for layout plus whatever colours, borders or images you want.
///
/// - `row`: the flex row holding the scroller and the bar (a `Node` with the default
///   `FlexDirection::Row` works). A default `Node` is added if the bundle has none.
/// - `scroller`: the scrolling node. **Give it a bounded height** (a `max_height`, or a fixed or
///   flex-shrunk height): a node that grows to fit its content never scrolls. [`Scroller`] adds
///   the overflow and the zero `min_height` itself.
/// - `bar`: the scrollbar track (its `Node` sets the bar's width and so on).
/// - `thumb`: the draggable thumb. It has no `Node`: Bevy's scrollbar sizes and places it. Include
///   your own [`ScrollbarThumb`] to set its border radius / border; a default one is added if not.
/// - `min_thumb_length`: the shortest the thumb may get, in logical pixels, so it stays
///   grabbable on a long list.
pub struct ScrollAreaLook<R: Bundle, S: Bundle, B: Bundle, T: Bundle> {
    /// The row holding scroller and bar.
    pub row: R,
    /// The scrolling node.
    pub scroller: S,
    /// The scrollbar track.
    pub bar: B,
    /// The scrollbar thumb.
    pub thumb: T,
    /// Shortest thumb, logical pixels.
    pub min_thumb_length: f32,
}

/// The entities [`spawn_scroll_area`] created.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollAreaParts {
    /// The row (a child of the parent you passed).
    pub row: Entity,
    /// The scroller (first child of the row). Your content is spawned under it.
    pub scroller: Entity,
    /// The scrollbar (second child of the row), targeting `scroller`.
    pub bar: Entity,
    /// The thumb (the bar's only child).
    pub thumb: Entity,
}

/// Spawn a vertical scroll area under `parent`: a row of `[scroller | scrollbar]`, the scrollbar
/// wired to the scroller, the wheel and the offset memory attached, and `fill` spawning your
/// content inside the scroller.
///
/// The offset is restored from [`ScrollMemory`] for `key` (see there for when it is kept).
///
/// ```
/// # use bevy::prelude::*;
/// # use bevy_ui_kit::*;
/// fn build(mut commands: Commands) {
///     commands.spawn(Node::default()).with_children(|p| {
///         let look = ScrollAreaLook {
///             row: Node { column_gap: Val::Px(4.0), ..default() },
///             scroller: Node { flex_direction: FlexDirection::Column, max_height: Val::Px(300.0), flex_grow: 1.0, ..default() },
///             bar: (Node { width: Val::Px(8.0), ..default() }, BackgroundColor(Color::srgb(0.2, 0.2, 0.2))),
///             thumb: BackgroundColor(Color::srgb(0.7, 0.7, 0.7)),
///             min_thumb_length: 24.0,
///         };
///         spawn_scroll_area(p, ScrollKey::new("log"), look, |list| {
///             for i in 0..100 {
///                 list.spawn(Text::new(format!("line {i}")));
///             }
///         });
///     });
/// }
/// # bevy::ecs::system::assert_is_system(build);
/// ```
pub fn spawn_scroll_area<R: Bundle, S: Bundle, B: Bundle, T: Bundle>(
    parent: &mut ChildSpawnerCommands,
    key: ScrollKey,
    look: ScrollAreaLook<R, S, B, T>,
    fill: impl FnOnce(&mut ChildSpawnerCommands),
) -> ScrollAreaParts {
    let ScrollAreaLook { row, scroller, bar, thumb, min_thumb_length } = look;
    let min_thumb_length = if min_thumb_length.is_finite() { min_thumb_length.max(0.0) } else { 0.0 };

    let mut row_cmd = parent.spawn(row);
    row_cmd.entry::<Node>().or_default();
    let row_id = row_cmd.id();

    let mut scroller_id = Entity::PLACEHOLDER;
    let mut bar_id = Entity::PLACEHOLDER;
    let mut thumb_id = Entity::PLACEHOLDER;
    row_cmd.with_children(|r| {
        let mut s = r.spawn(scroller);
        // Node first, so `Scroller`'s insert hook finds it to patch.
        s.entry::<Node>().or_default();
        s.insert((Scroller, key));
        s.with_children(fill);
        scroller_id = s.id();

        // `Scrollbar` holds a raw `Entity`: a bar aimed at the wrong node compiles, renders and
        // drags while moving nothing. This is the one place it is set.
        let mut b = r.spawn(bar);
        b.entry::<Node>().or_default();
        b.insert(Scrollbar::new(scroller_id, ControlOrientation::Vertical, min_thumb_length));
        b.with_children(|b| {
            let mut t = b.spawn(thumb);
            t.entry::<ScrollbarThumb>().or_default();
            thumb_id = t.id();
        });
        bar_id = b.id();
    });
    ScrollAreaParts { row: row_id, scroller: scroller_id, bar: bar_id, thumb: thumb_id }
}

/// How far a laid-out node can scroll on each axis, in logical pixels (`None` before layout).
pub fn max_scroll_offset(node: &ComputedNode) -> Option<Vec2> {
    if node.size.x <= 0.0 && node.size.y <= 0.0 {
        return None;
    }
    let max = (node.content_size - node.size + node.scrollbar_size) * node.inverse_scale_factor;
    if !max.is_finite() {
        return None;
    }
    Some(max.max(Vec2::ZERO))
}

type WheelArea<'a> = (Entity, &'a mut ScrollPosition, &'a RelativeCursorPosition, Option<&'a ComputedNode>, Option<&'a Node>, Option<&'a Visibility>);

/// Scroll the hovered [`Scroller`] from the mouse wheel.
///
/// With the cursor over nested scrollers, only the innermost one scrolls. With the cursor over
/// none and exactly one shown scroller in the world, that one scrolls (see
/// [`ScrollConfig::lone_area_takes_wheel`]). The offset never goes above the top, and once the
/// node is laid out never past the end (Bevy clamps only what it draws and never writes the value
/// back, so an unclamped wheel would pile up offset past the end).
pub fn wheel_scroll(
    config: Res<ScrollConfig>,
    wheel: Option<Res<AccumulatedMouseScroll>>,
    mut areas: Query<WheelArea, With<Scroller>>,
    parents: Query<&ChildOf>,
) {
    let Some(wheel) = wheel else { return };
    let delta = wheel.delta;
    if delta == Vec2::ZERO || !delta.is_finite() {
        return;
    }
    let factor = match wheel.unit {
        MouseScrollUnit::Line => config.line_px,
        MouseScrollUnit::Pixel => config.pixel_scale,
    };
    if !factor.is_finite() {
        return;
    }
    let step = delta * factor;

    // Hover comes from Bevy's focus system, which already ignores hidden nodes.
    let hovered: Vec<Entity> = areas.iter().filter(|(_, _, rel, ..)| rel.cursor_over).map(|(e, ..)| e).collect();
    let targets: Vec<Entity> = if hovered.is_empty() {
        let shown = |node: Option<&Node>, vis: Option<&Visibility>| vis != Some(&Visibility::Hidden) && node.is_none_or(|n| n.display != Display::None);
        let mut shown = areas.iter().filter(|(_, _, _, _, node, vis)| shown(*node, *vis)).map(|(e, ..)| e);
        match (config.lone_area_takes_wheel, shown.next(), shown.next()) {
            (true, Some(only), None) => vec![only],
            _ => Vec::new(),
        }
    } else {
        // Innermost only: drop every hovered area that is an ancestor of another hovered one.
        let outer: Vec<Entity> = hovered.iter().flat_map(|e| parents.iter_ancestors(*e)).filter(|a| hovered.contains(a)).collect();
        hovered.into_iter().filter(|e| !outer.contains(e)).collect()
    };

    for target in targets {
        let Ok((_, mut pos, _, computed, node, _)) = areas.get_mut(target) else { continue };
        let max = computed.and_then(max_scroll_offset);
        let mut next = pos.0;
        next.y = scroll_axis(pos.0.y, step.y, max.map(|m| m.y));
        if node.is_some_and(|n| n.overflow.x == OverflowAxis::Scroll) {
            next.x = scroll_axis(pos.0.x, step.x, max.map(|m| m.x));
        }
        if pos.0 != next {
            pos.0 = next;
        }
    }
}

/// One axis of a wheel step: wheel up (positive delta) moves toward the top.
fn scroll_axis(current: f32, step: f32, max: Option<f32>) -> f32 {
    let current = if current.is_finite() { current } else { 0.0 };
    let mut v = (current - step).max(0.0);
    if let Some(max) = max {
        v = v.min(max);
    }
    v
}

/// Mirror every live [`ScrollKey`]ed area's offset into [`ScrollMemory`], forgetting the keys
/// that are gone. If two live areas share a key, one of them wins; give them different pages.
pub fn remember_scroll(areas: Query<(&ScrollKey, &ScrollPosition)>, mut memory: ResMut<ScrollMemory>) {
    let alive: HashMap<ScrollKey, Vec2> = areas.iter().map(|(k, p)| (k.clone(), sanitize_offset(p.0))).collect();
    if memory.offsets != alive {
        memory.offsets = alive;
    }
}

/// Show or hide every [`AutoHideScrollbar`] by whether its target overflows on the bar's axis.
pub fn auto_hide_scrollbars(mut bars: Query<(&Scrollbar, &mut Visibility), With<AutoHideScrollbar>>, nodes: Query<&ComputedNode>) {
    for (bar, mut vis) in &mut bars {
        let overflows = nodes.get(bar.target).is_ok_and(|n| {
            let (content, size) = match bar.orientation {
                ControlOrientation::Vertical => (n.content_size.y, n.size.y - n.scrollbar_size.y),
                ControlOrientation::Horizontal => (n.content_size.x, n.size.x - n.scrollbar_size.x),
            };
            // Half a logical pixel of slack: rounding must not flicker the bar.
            (content - size) * n.inverse_scale_factor > 0.5
        });
        let want = if overflows { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
    }
}

/// What is wrong with a scroll area's wiring (see [`check_scroll_wiring`]).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScrollWiringError {
    /// The entity does not exist.
    Missing,
    /// No `Node`, or its `overflow.y` is not `Scroll`: nothing will scroll.
    NotScrollable,
    /// No [`Scroller`] (or no `ScrollPosition` / `RelativeCursorPosition`): the wheel ignores it.
    NoWheel,
    /// No scrollbar targets it. The `usize` is how many scrollbars exist at all (if there are
    /// some, one of them probably targets the wrong entity).
    NoScrollbar(usize),
    /// More than one scrollbar targets it (the count).
    SeveralScrollbars(usize),
    /// The bar has no `ScrollbarThumb` as a DIRECT child (Bevy only moves a direct child).
    NoThumb,
    /// The bar has more than one `ScrollbarThumb` child (the count).
    SeveralThumbs(usize),
}

impl fmt::Display for ScrollWiringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => write!(f, "the scroller entity does not exist"),
            Self::NotScrollable => write!(f, "the scroller has no Node with overflow.y = Scroll"),
            Self::NoWheel => write!(f, "the scroller has no Scroller / ScrollPosition / RelativeCursorPosition"),
            Self::NoScrollbar(n) => write!(f, "no Scrollbar targets the scroller ({n} scrollbar(s) exist - is one aimed at the wrong entity?)"),
            Self::SeveralScrollbars(n) => write!(f, "{n} Scrollbars target the scroller"),
            Self::NoThumb => write!(f, "the scrollbar has no ScrollbarThumb as a direct child"),
            Self::SeveralThumbs(n) => write!(f, "the scrollbar has {n} ScrollbarThumb children"),
        }
    }
}

impl std::error::Error for ScrollWiringError {}

/// Check that `scroller` is a working scroll area: scrollable, wheel-driven, targeted by exactly
/// one [`Scrollbar`] whose thumb is a direct child. Returns the bar entity.
///
/// Meant for tests: a scrollbar aimed at the wrong entity compiles, renders and drags smoothly
/// while moving nothing, and only a check like this catches it.
pub fn check_scroll_wiring(world: &World, scroller: Entity) -> Result<Entity, ScrollWiringError> {
    let Ok(e) = world.get_entity(scroller) else { return Err(ScrollWiringError::Missing) };
    if e.get::<Node>().is_none_or(|n| n.overflow.y != OverflowAxis::Scroll) {
        return Err(ScrollWiringError::NotScrollable);
    }
    if !(e.contains::<Scroller>() && e.contains::<ScrollPosition>() && e.contains::<RelativeCursorPosition>()) {
        return Err(ScrollWiringError::NoWheel);
    }
    let mut total = 0;
    let mut bars = Vec::new();
    if let Some(mut q) = world.try_query::<(Entity, &Scrollbar)>() {
        for (bar, sb) in q.iter(world) {
            total += 1;
            if sb.target == scroller {
                bars.push(bar);
            }
        }
    }
    let bar = match bars.as_slice() {
        [] => return Err(ScrollWiringError::NoScrollbar(total)),
        [one] => *one,
        many => return Err(ScrollWiringError::SeveralScrollbars(many.len())),
    };
    let thumbs = world.get::<Children>(bar).map(|c| c.iter().filter(|t| world.get::<ScrollbarThumb>(*t).is_some()).count()).unwrap_or(0);
    match thumbs {
        0 => Err(ScrollWiringError::NoThumb),
        1 => Ok(bar),
        n => Err(ScrollWiringError::SeveralThumbs(n)),
    }
}
