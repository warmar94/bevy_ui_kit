//! A strict headless test app: `MinimalPlugins` + input + the kit, ambiguity detection at `Error`
//! on every schedule the kit adds systems to, time advanced by a fixed step.

#![allow(dead_code)]

use std::time::Duration;

use bevy::ecs::schedule::{LogLevel, ScheduleBuildSettings};
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::input::touch::TouchPhase;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy_ui_kit::UiKitPlugin;

/// The fixed frame time of every test app.
pub const STEP: Duration = Duration::from_millis(10);

/// `MinimalPlugins` + `InputPlugin` + `UiKitPlugin`, strict ambiguity detection, fixed time step.
pub fn app() -> App {
    app_with(UiKitPlugin::default())
}

/// [`app`] with a configured plugin.
pub fn app_with(plugin: UiKitPlugin) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, bevy::input::InputPlugin, plugin)).insert_resource(TimeUpdateStrategy::ManualDuration(STEP));
    strict(&mut app);
    app
}

/// Ambiguity detection at `Error` on the schedules the kit adds systems to.
pub fn strict(app: &mut App) {
    let settings = || ScheduleBuildSettings { ambiguity_detection: LogLevel::Error, ..default() };
    app.edit_schedule(Update, |s| {
        s.set_build_settings(settings());
    });
    app.edit_schedule(PostUpdate, |s| {
        s.set_build_settings(settings());
    });
}

/// Write one wheel message (positive `lines` = wheel up) and run a frame.
pub fn wheel(app: &mut App, lines: f32) {
    let window = Entity::PLACEHOLDER;
    app.world_mut().write_message(MouseWheel { unit: MouseScrollUnit::Line, x: 0.0, y: lines, window, phase: TouchPhase::Moved });
    app.update();
}

/// A laid-out node: `size` and `content` in logical pixels at scale factor 1.
pub fn laid_out(size: Vec2, content: Vec2) -> ComputedNode {
    ComputedNode { size, unrounded_size: size, content_size: content, inverse_scale_factor: 1.0, ..default() }
}
