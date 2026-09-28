//! The motion helpers: cards pop in with an overshoot and pop out when clicked, buttons squash
//! while pressed, a locked button wobbles "no", and a bar eases toward new targets.
//!
//! Keys: `Space` adds a card, `B` gives the bar a new target, `M` toggles reduced motion.
//!
//! The kit has no look and no default timings: every colour, size and number below is this
//! example's own.

use bevy::prelude::*;
use bevy_ui_kit::*;

const INK: Color = Color::srgb(0.9, 0.9, 0.9);
const CARD: Color = Color::srgb(0.3, 0.3, 0.3);
const BUTTON: Color = Color::srgb(0.22, 0.22, 0.22);
const TRACK: Color = Color::srgb(0.18, 0.18, 0.18);
const BAR: Color = Color::srgb(0.6, 0.6, 0.6);

// This example's "theme": the motion numbers a game would load from its own config.
const POP: PopSpec = PopSpec::new(0.22, 0.6, 1.1);
const SQUASH: SquashSpec = SquashSpec { pressed_scale: Vec2::new(1.06, 0.9), rate: 30.0 };
const NO: WobbleSpec = WobbleSpec::horizontal(0.35, 8.0, 14.0);
const BAR_EASE: EaseSpec = EaseSpec { rise: Easing::Exponential { rate: 6.0 }, fall: Easing::Linear { per_sec: 0.8 } };

#[derive(Component)]
struct Cards;

#[derive(Component)]
struct Card;

#[derive(Component)]
enum Action {
    AddCard,
    Locked,
}

#[derive(Component)]
struct Bar;

fn main() {
    App::new().add_plugins((DefaultPlugins, UiKitPlugin::default())).add_systems(Startup, setup).add_systems(Update, (keys, clicks)).run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            row_gap: Val::Px(16.0),
            ..default()
        })
        .with_children(|root| {
            root.spawn((Text::new("Space: card - B: bar target - M: reduced motion - click a card to dismiss it"), TextColor(INK)));
            root.spawn(Node { column_gap: Val::Px(12.0), ..default() }).with_children(|row| {
                button(row, "Add card", Action::AddCard);
                button(row, "Locked", Action::Locked);
            });
            root.spawn((Node { width: Val::Px(300.0), height: Val::Px(14.0), ..default() }, BackgroundColor(TRACK))).with_children(|track| {
                track.spawn((
                    Bar,
                    EasedValue::new(0.3, BAR_EASE),
                    EasedFill::default(),
                    Node { height: Val::Percent(100.0), ..default() },
                    BackgroundColor(BAR),
                ));
            });
            root.spawn((Cards, Node { column_gap: Val::Px(10.0), flex_wrap: FlexWrap::Wrap, max_width: Val::Px(600.0), ..default() }));
        });
}

fn button(parent: &mut ChildSpawnerCommands, label: &str, action: Action) {
    parent
        .spawn((Button, action, PressSquash::new(SQUASH), Node { padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)), ..default() }, BackgroundColor(BUTTON)))
        .with_children(|b| {
            b.spawn((Text::new(label), TextColor(INK)));
        });
}

fn keys(
    input: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    cards: Single<Entity, With<Cards>>,
    mut bar: Single<&mut EasedValue, With<Bar>>,
    mut config: ResMut<MotionConfig>,
    mut next: Local<u32>,
) {
    if input.just_pressed(KeyCode::Space) {
        add_card(&mut commands, *cards);
    }
    if input.just_pressed(KeyCode::KeyB) {
        *next += 1;
        // A fixed cycle of targets, so the example needs no random numbers.
        let targets = [0.9, 0.15, 0.6, 1.0, 0.0, 0.45];
        bar.set_target(targets[*next as usize % targets.len()]);
    }
    if input.just_pressed(KeyCode::KeyM) {
        config.enabled = !config.enabled;
        info!("motion {}", if config.enabled { "on" } else { "reduced" });
    }
}

fn add_card(commands: &mut Commands, cards: Entity) {
    commands.entity(cards).with_children(|c| {
        c.spawn((
            Card,
            Button,
            PopIn::new(POP),
            Node { width: Val::Px(90.0), height: Val::Px(60.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
            BackgroundColor(CARD),
        ))
        .with_children(|card| {
            card.spawn((Text::new("card"), TextColor(INK)));
        });
    });
}

/// A button's parts.
type Clicked<'a> = (Entity, &'a Interaction, Option<&'a Action>, Has<Card>);

/// Only buttons whose interaction changed, skipping cards already on their way out.
type JustChanged = (Changed<Interaction>, Without<PopOut>);

fn clicks(mut commands: Commands, buttons: Query<Clicked, JustChanged>, cards: Single<Entity, With<Cards>>) {
    for (e, interaction, action, is_card) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match (action, is_card) {
            (Some(Action::AddCard), _) => add_card(&mut commands, *cards),
            // Re-inserting restarts the wobble.
            (Some(Action::Locked), _) => {
                commands.entity(e).insert(Wobble::new(NO));
            }
            (None, true) => {
                commands.entity(e).insert(PopOut::new(POP, PopOutEnd::Despawn));
            }
            _ => {}
        }
    }
}
