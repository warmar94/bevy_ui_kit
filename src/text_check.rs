//! Glyph coverage check for tests: find every character in your UI text that your font cannot
//! draw.
//!
//! A game whose font covers only some glyphs (ASCII only, say) draws a missing glyph as a blank
//! box, silently. Build a panel headlessly in a test, then run [`check_ascii`] or
//! [`check_glyphs`] on its root: the report names every offending character and the entity whose
//! text contains it, wherever that string was written.

use std::fmt;

use bevy_ecs::hierarchy::Children;
use bevy_ecs::prelude::*;
use bevy_text::TextSpan;
use bevy_ui::widget::Text;

/// Printable ASCII (`' '..='~'`) plus `'\n'` and `'\t'`: the default allowed set.
pub fn is_printable_ascii(c: char) -> bool {
    matches!(c, ' '..='~' | '\n' | '\t')
}

/// One character outside the allowed set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlyphIssue {
    /// The entity holding the text (a [`Text`] or a [`TextSpan`]).
    pub entity: Entity,
    /// The character.
    pub ch: char,
    /// The whole string it appeared in.
    pub text: String,
}

/// The result of a glyph check.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GlyphReport {
    /// How many [`Text`] / [`TextSpan`] entities were read. Assert it is above zero: a check that
    /// read nothing passes trivially (a panel built on another tab, a root that was despawned).
    pub texts_checked: usize,
    /// Every offending character, one entry per distinct character per entity.
    pub issues: Vec<GlyphIssue>,
}

impl GlyphReport {
    /// No issues?
    pub fn is_ok(&self) -> bool {
        self.issues.is_empty()
    }
}

impl fmt::Display for GlyphReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.issues.is_empty() {
            return write!(f, "{} text(s) checked, every character allowed", self.texts_checked);
        }
        writeln!(f, "{} text(s) checked, {} disallowed character(s):", self.texts_checked, self.issues.len())?;
        for i in &self.issues {
            writeln!(f, "  {:?} (U+{:04X}) in {:?} on {}", i.ch, i.ch as u32, i.text, i.entity)?;
        }
        Ok(())
    }
}

fn check_one(entity: Entity, s: &str, allowed: &impl Fn(char) -> bool, report: &mut GlyphReport) {
    report.texts_checked += 1;
    let mut seen: Vec<char> = Vec::new();
    for ch in s.chars() {
        if !allowed(ch) && !seen.contains(&ch) {
            seen.push(ch);
            report.issues.push(GlyphIssue { entity, ch, text: s.to_owned() });
        }
    }
}

/// Check the [`Text`] and [`TextSpan`] of `root` and all its descendants against `allowed`.
pub fn check_glyphs(world: &World, root: Entity, allowed: impl Fn(char) -> bool) -> GlyphReport {
    let mut report = GlyphReport::default();
    let mut stack = vec![root];
    // A hierarchy cannot cycle, but a bound costs nothing and a test must never hang.
    let mut budget = 1_000_000usize;
    while let Some(e) = stack.pop() {
        budget = budget.saturating_sub(1);
        if budget == 0 {
            tracing::warn!("bevy_ui_kit: glyph check stopped after 1,000,000 entities");
            break;
        }
        if let Some(t) = world.get::<Text>(e) {
            check_one(e, &t.0, &allowed, &mut report);
        }
        if let Some(t) = world.get::<TextSpan>(e) {
            check_one(e, &t.0, &allowed, &mut report);
        }
        if let Some(children) = world.get::<Children>(e) {
            stack.extend(children.iter().rev());
        }
    }
    report
}

/// [`check_glyphs`] with [`is_printable_ascii`].
pub fn check_ascii(world: &World, root: Entity) -> GlyphReport {
    check_glyphs(world, root, is_printable_ascii)
}

/// Check EVERY [`Text`] and [`TextSpan`] in the world against `allowed`.
pub fn check_all_glyphs(world: &World, allowed: impl Fn(char) -> bool) -> GlyphReport {
    let mut report = GlyphReport::default();
    if let Some(mut q) = world.try_query::<(Entity, &Text)>() {
        for (e, t) in q.iter(world) {
            check_one(e, &t.0, &allowed, &mut report);
        }
    }
    if let Some(mut q) = world.try_query::<(Entity, &TextSpan)>() {
        for (e, t) in q.iter(world) {
            check_one(e, &t.0, &allowed, &mut report);
        }
    }
    report
}
