//! Spectacle moments: boss intro choreography, player death fade, stage-clear
//! pause. The execute kill-cam is driven by `camera::drive_slowmo_from_kill_events_system`
//! plus FOV punch-in here.

use crate::game::camera::FovTween;
use crate::game::feel::{
    BOSS_INTRO_S, KILLCAM_DURATION_S, KILLCAM_TIME_SCALE, PLAYER_DEATH_FADE_S,
    STAGE_CLEAR_SLOWMO_S,
};
use crate::game::hitstop::{HitEvent, HitKind};
use crate::game::rush::CurrentBossConfig;
use bevy::prelude::*;

#[derive(Resource, Default)]
pub struct StageIntro {
    pub remaining_s: f32,
    pub total_s: f32,
}

impl StageIntro {
    pub fn active(&self) -> bool {
        self.remaining_s > 0.0
    }
}

pub fn no_intro(intro: Res<StageIntro>) -> bool {
    !intro.active()
}

pub fn enter_stage_start_intro(mut intro: ResMut<StageIntro>) {
    intro.remaining_s = BOSS_INTRO_S;
    intro.total_s = BOSS_INTRO_S;
}

pub fn tick_intro_system(
    real_time: Res<Time<Real>>,
    mut intro: ResMut<StageIntro>,
) {
    if intro.active() {
        intro.remaining_s = (intro.remaining_s - real_time.delta_secs()).max(0.0);
    }
}

#[derive(Component)]
pub struct IntroOverlay;

pub fn spawn_intro_overlay(
    mut commands: Commands,
    cfg: Res<CurrentBossConfig>,
) {
    let (name, flavor) = cfg
        .0
        .as_ref()
        .map(|b| (b.name.to_string(), b.flavor.to_string()))
        .unwrap_or_default();
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
            IntroOverlay,
        ))
        .with_children(|c| {
            c.spawn((
                Text::new(name),
                TextFont {
                    font_size: 60.0,
                    ..default()
                },
                TextColor(Color::srgba(0.95, 0.85, 0.5, 1.0)),
            ));
            c.spawn((
                Text::new(flavor),
                TextFont {
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::srgba(0.85, 0.85, 0.95, 1.0)),
                Node {
                    margin: UiRect::all(Val::Px(8.0)),
                    ..default()
                },
            ));
        });
}

pub fn fade_out_intro_overlay(
    intro: Res<StageIntro>,
    mut q: Query<&mut BackgroundColor, With<IntroOverlay>>,
) {
    if !intro.active() {
        for mut c in &mut q {
            c.0 = c.0.with_alpha(0.0);
        }
        return;
    }
    let t = intro.remaining_s / intro.total_s.max(0.001);
    let alpha = 0.45 * t.clamp(0.0, 1.0);
    for mut c in &mut q {
        c.0 = c.0.with_alpha(alpha);
    }
}

pub fn cleanup_intro_overlay(mut commands: Commands, q: Query<Entity, With<IntroOverlay>>) {
    for e in &q {
        commands.entity(e).despawn_recursive();
    }
}

/// On `HitKind::Killed`, punch the camera FOV in (zoom) for kill-cam
/// emphasis. The slow-mo itself is driven by `SlowMo` in `camera.rs`.
pub fn punch_fov_on_kill_system(
    mut events: EventReader<HitEvent>,
    mut q: Query<&mut FovTween, With<crate::game::camera::MainCamera>>,
) {
    let killed = events.read().any(|e| matches!(e.kind, HitKind::Killed));
    if !killed {
        return;
    }
    if let Ok(mut tween) = q.get_single_mut() {
        let base = tween.base_fov;
        // Zoom in by 12° for the duration of the kill-cam.
        tween.pulse(base, -12.0, KILLCAM_DURATION_S);
    }
    // Use the constant so the linker keeps this knob exported.
    let _ = STAGE_CLEAR_SLOWMO_S;
    let _ = KILLCAM_TIME_SCALE;
}

#[derive(Component)]
pub struct DeathFade;

pub fn spawn_death_fade(mut commands: Commands) {
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
        DeathFade,
    ));
}

pub fn tick_death_fade(
    real_time: Res<Time<Real>>,
    mut q: Query<&mut BackgroundColor, With<DeathFade>>,
    mut elapsed: Local<f32>,
) {
    *elapsed += real_time.delta_secs();
    let t = (*elapsed / PLAYER_DEATH_FADE_S).clamp(0.0, 1.0);
    for mut c in &mut q {
        c.0 = c.0.with_alpha(t);
    }
}

pub fn cleanup_death_fade(
    mut commands: Commands,
    mut elapsed: Local<f32>,
    q: Query<Entity, With<DeathFade>>,
) {
    *elapsed = 0.0;
    for e in &q {
        commands.entity(e).despawn_recursive();
    }
}
