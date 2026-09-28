//! Plugin-level checks: the glyph helper, strict scheduling, configuration.

mod common;

use bevy::prelude::*;
use bevy::ui::ScrollPosition;
use bevy_ui_kit::*;
use common::*;

fn panel(world: &mut World, lines: &[&str]) -> Entity {
    world
        .spawn(Node::default())
        .with_children(|p| {
            for l in lines {
                p.spawn(Text::new(*l)).with_children(|t| {
                    t.spawn(TextSpan::new(" (span)"));
                });
            }
        })
        .id()
}

#[test]
fn the_glyph_check_finds_a_non_ascii_character() {
    let mut app = app();
    let root = panel(app.world_mut(), &["Volume", "Scale \u{00d7}2", "Master \u{2013} 80%"]);
    let report = check_ascii(app.world(), root);
    println!(">>> glyph check:\n{report}");
    assert_eq!(report.texts_checked, 6, "3 texts + 3 spans read");
    let bad: Vec<char> = report.issues.iter().map(|i| i.ch).collect();
    assert_eq!(bad, vec!['\u{00d7}', '\u{2013}']);
    assert!(report.issues[0].text.contains("Scale"));
    assert!(!report.is_ok());
}

#[test]
fn the_glyph_check_passes_clean_text_and_takes_any_set() {
    let mut app = app();
    let root = panel(app.world_mut(), &["Resume", "Settings\nand more"]);
    let report = check_ascii(app.world(), root);
    assert!(report.is_ok(), "{report}");
    assert!(report.texts_checked > 0, "a check that read nothing proves nothing");

    let digits_only = check_glyphs(app.world(), root, |c| c.is_ascii_digit());
    assert!(!digits_only.is_ok());
    let everywhere = check_all_glyphs(app.world(), is_printable_ascii);
    assert_eq!(everywhere.texts_checked, 4);
    let nothing = check_ascii(app.world(), Entity::PLACEHOLDER);
    assert_eq!(nothing.texts_checked, 0);
}

/// Proof that the strict test harness would catch an ordering bug: an unordered second writer of
/// `ScrollPosition` next to the kit's wheel fails the schedule build.
#[test]
#[should_panic(expected = "conflicting data access")]
fn strict_mode_rejects_an_unordered_scroll_writer() {
    fn rogue(mut q: Query<&mut ScrollPosition>) {
        for mut p in &mut q {
            p.y += 0.0;
        }
    }
    let mut app = app();
    app.add_systems(Update, rogue);
    app.update();
}

/// The documented way to add that writer: order it against the kit's set.
#[test]
fn an_ordered_scroll_writer_is_accepted() {
    fn ordered(mut q: Query<&mut ScrollPosition>) {
        for mut p in &mut q {
            p.y += 0.0;
        }
    }
    let mut app = app();
    app.add_systems(Update, ordered.after(UiKitSystems::Scroll));
    app.update();
}

#[test]
fn the_plugin_config_reaches_the_resources() {
    let plugin = UiKitPlugin {
        scroll: ScrollConfig { line_px: 40.0, ..default() },
        slider: SliderInputConfig { page_steps: 4.0, ..default() },
        motion: MotionConfig { max_step_secs: 0.1, ..default() },
    };
    let app = app_with(plugin);
    assert_eq!(app.world().resource::<ScrollConfig>().line_px, 40.0);
    assert_eq!(app.world().resource::<SliderInputConfig>().page_steps, 4.0);
    assert_eq!(app.world().resource::<MotionConfig>().max_step_secs, 0.1);
}

/// Nothing breaks without input or time: the kit fails closed in a bare app.
#[test]
fn a_bare_app_does_not_panic() {
    let mut app = App::new();
    app.add_plugins(UiKitPlugin::default());
    app.world_mut().spawn((Node::default(), ScrollKey::new("a"), PopIn::new(PopSpec::new(0.2, 0.8, 1.1))));
    for _ in 0..3 {
        app.update();
    }
}

/// With `serialize`, every parameter struct loads from the game's own RON theme.
#[cfg(feature = "serialize")]
#[test]
fn parameters_load_from_ron() {
    #[derive(serde::Deserialize)]
    struct Theme {
        pop: PopSpec,
        wobble: WobbleSpec,
        squash: SquashSpec,
        bar: EaseSpec,
        scroll: ScrollConfig,
        motion: MotionConfig,
    }
    let src = r#"(
        pop: (secs: 0.18, from: 0.85, overshoot: 1.06, peak_at: 0.6),
        wobble: (secs: 0.3, amplitude_px: 5.0, hz: 11.0, direction: (1.0, 0.0)),
        squash: (pressed_scale: (1.05, 0.92), rate: 25.0),
        bar: (rise: Instant, fall: Linear(per_sec: 0.5)),
        scroll: (line_px: 32.0),
        motion: (enabled: false),
    )"#;
    let theme: Theme = ron::from_str(src).expect("the theme parses");
    assert_eq!(theme.pop.secs, 0.18);
    assert_eq!(theme.wobble.direction, Vec2::X);
    assert_eq!(theme.squash.pressed_scale, Vec2::new(1.05, 0.92));
    assert_eq!(theme.bar.fall, Easing::Linear { per_sec: 0.5 });
    assert_eq!(theme.scroll.line_px, 32.0);
    assert!(theme.scroll.lone_area_takes_wheel, "missing fields keep their defaults");
    assert!(!theme.motion.enabled);
}
