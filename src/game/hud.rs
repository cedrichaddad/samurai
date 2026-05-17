//! In-game HUD: HP and posture bars for player and boss, lock-on reticle,
//! boss name banner, execute prompt, dossier indicator. Replaces the text-only
//! `polish.rs` UI for `AppState::Stage`.

use crate::game::boss::Boss;
use crate::game::combat::Health;
use crate::game::hitstop::{HitEvent, HitKind};
use crate::game::lockon::LockOn;
use crate::game::memory::BossMemoryDb;
use crate::game::player::Player;
use crate::game::posture::Posture;
use crate::game::rush::CurrentBossConfig;
use bevy::prelude::*;

#[derive(Component)]
pub struct HudRoot;

#[derive(Component)]
pub struct PlayerHpFill;
#[derive(Component)]
pub struct PlayerPostureFill;
#[derive(Component)]
pub struct BossHpFill;
#[derive(Component)]
pub struct BossPostureFill;
#[derive(Component)]
pub struct BossNameText;
#[derive(Component)]
pub struct BossStyleText;
#[derive(Component)]
pub struct LockOnReticle;
#[derive(Component)]
pub struct ExecutePrompt;
#[derive(Component)]
pub struct DossierLabel;

/// Full-screen red flash that pulses when the player takes damage.
/// Alpha decays exponentially each frame; tick system bumps alpha on HitEvents.
#[derive(Component, Default)]
pub struct DamageFlash {
    pub alpha: f32,
}

/// Full-screen red vignette that intensifies + pulses as player HP drops.
/// Visibility is purely a function of HP %.
#[derive(Component)]
pub struct LowHpVignette;

pub fn spawn_hud(mut commands: Commands) {
    // Root container (full-screen, transparent).
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            HudRoot,
        ))
        .with_children(|root| {
            // ─── Player bottom-left ─────────────────────────────────────
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    bottom: Val::Px(28.0),
                    left: Val::Px(28.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
                    width: Val::Px(280.0),
                    ..default()
                },
            ))
            .with_children(|c| {
                spawn_bar(c, PlayerHpFill, Color::srgb(0.85, 0.2, 0.25), 14.0);
                spawn_bar(c, PlayerPostureFill, Color::srgb(0.95, 0.85, 0.4), 8.0);
            });

            // ─── Boss top-center ────────────────────────────────────────
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(28.0),
                    left: Val::Percent(50.0),
                    margin: UiRect {
                        left: Val::Px(-220.0),
                        ..default()
                    },
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
                    width: Val::Px(440.0),
                    align_items: AlignItems::Center,
                    ..default()
                },
            ))
            .with_children(|c| {
                c.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: 22.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.95, 0.85, 0.5)),
                    BossNameText,
                ));
                c.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: 13.0,
                        ..default()
                    },
                    TextColor(Color::srgba(0.8, 0.8, 0.85, 0.85)),
                    BossStyleText,
                    Node {
                        margin: UiRect {
                            bottom: Val::Px(4.0),
                            ..default()
                        },
                        ..default()
                    },
                ));
                spawn_bar(c, BossHpFill, Color::srgb(0.8, 0.15, 0.15), 14.0);
                spawn_bar(c, BossPostureFill, Color::srgb(0.95, 0.85, 0.4), 8.0);
                c.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: 11.0,
                        ..default()
                    },
                    TextColor(Color::srgba(0.6, 0.7, 0.85, 0.7)),
                    DossierLabel,
                    Node {
                        margin: UiRect {
                            top: Val::Px(4.0),
                            ..default()
                        },
                        ..default()
                    },
                ));
            });

            // ─── Lock-on reticle (centered when active) ─────────────────
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Percent(48.0),
                    left: Val::Percent(50.0),
                    width: Val::Px(18.0),
                    height: Val::Px(18.0),
                    margin: UiRect::all(Val::Px(-9.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    display: Display::None,
                    ..default()
                },
                BorderColor(Color::srgba(1.0, 1.0, 1.0, 0.85)),
                LockOnReticle,
            ));

            // ─── Damage flash + low-HP vignette (full-screen overlays) ──
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.85, 0.05, 0.05, 0.0)),
                DamageFlash::default(),
            ));
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    border: UiRect::all(Val::Px(80.0)),
                    ..default()
                },
                BackgroundColor(Color::NONE),
                BorderColor(Color::srgba(0.7, 0.0, 0.0, 0.0)),
                LowHpVignette,
            ));

            // ─── Execute prompt ─────────────────────────────────────────
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Percent(40.0),
                    left: Val::Percent(50.0),
                    margin: UiRect::all(Val::Px(-30.0)),
                    width: Val::Px(60.0),
                    height: Val::Px(60.0),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    display: Display::None,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
                ExecutePrompt,
            ))
            .with_children(|c| {
                c.spawn((
                    Text::new("E"),
                    TextFont {
                        font_size: 36.0,
                        ..default()
                    },
                    TextColor(Color::srgba(1.0, 1.0, 0.7, 1.0)),
                ));
            });
        });
}

fn spawn_bar<C: Component>(parent: &mut ChildBuilder, marker: C, fill_color: Color, height: f32) {
    parent
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(height),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BorderColor(Color::srgba(0.0, 0.0, 0.0, 0.65)),
            BackgroundColor(Color::srgba(0.05, 0.05, 0.07, 0.85)),
        ))
        .with_children(|bar| {
            bar.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(fill_color),
                marker,
            ));
        });
}

pub fn cleanup_hud(mut commands: Commands, q: Query<Entity, With<HudRoot>>) {
    for e in &q {
        commands.entity(e).despawn_recursive();
    }
}

pub fn update_player_hud(
    player: Query<(&Health, &Posture), With<Player>>,
    mut hp: Query<&mut Node, (With<PlayerHpFill>, Without<PlayerPostureFill>)>,
    mut po: Query<&mut Node, (With<PlayerPostureFill>, Without<PlayerHpFill>)>,
) {
    let Ok((h, p)) = player.get_single() else { return };
    if let Ok(mut n) = hp.get_single_mut() {
        n.width = Val::Percent((h.current / h.max * 100.0).clamp(0.0, 100.0));
    }
    if let Ok(mut n) = po.get_single_mut() {
        n.width = Val::Percent((p.fill_ratio() * 100.0).clamp(0.0, 100.0));
    }
}

pub fn update_boss_hud(
    boss: Query<(&Health, &Posture), With<Boss>>,
    cfg: Option<Res<CurrentBossConfig>>,
    db: Option<Res<BossMemoryDb>>,
    mut hp: Query<&mut Node, (With<BossHpFill>, Without<BossPostureFill>)>,
    mut po: Query<&mut Node, (With<BossPostureFill>, Without<BossHpFill>)>,
    mut name_q: Query<&mut Text, (With<BossNameText>, Without<BossStyleText>, Without<DossierLabel>)>,
    mut style_q: Query<&mut Text, (With<BossStyleText>, Without<BossNameText>, Without<DossierLabel>)>,
    mut dossier_q: Query<&mut Text, (With<DossierLabel>, Without<BossNameText>, Without<BossStyleText>)>,
) {
    let Ok((h, p)) = boss.get_single() else {
        if let Ok(mut n) = hp.get_single_mut() {
            n.width = Val::Percent(0.0);
        }
        if let Ok(mut n) = po.get_single_mut() {
            n.width = Val::Percent(0.0);
        }
        return;
    };
    if let Ok(mut n) = hp.get_single_mut() {
        n.width = Val::Percent((h.current / h.max * 100.0).clamp(0.0, 100.0));
    }
    if let Ok(mut n) = po.get_single_mut() {
        n.width = Val::Percent((p.fill_ratio() * 100.0).clamp(0.0, 100.0));
    }

    let (name, style) = cfg
        .as_ref()
        .and_then(|c| c.0.as_ref())
        .map(|b| (b.name.to_string(), format!("{:?}", b.style)))
        .unwrap_or((String::new(), String::new()));
    if let Ok(mut t) = name_q.get_single_mut() {
        **t = name;
    }
    if let Ok(mut t) = style_q.get_single_mut() {
        **t = style;
    }
    if let Ok(mut t) = dossier_q.get_single_mut() {
        let count = db.as_ref().map(|d| d.dossier_base_count()).unwrap_or(0);
        **t = if count > 0 {
            format!("Studied: {count} of you")
        } else {
            String::new()
        };
    }
}

pub fn toggle_lockon_reticle(
    lock: Res<LockOn>,
    mut q: Query<&mut Node, With<LockOnReticle>>,
) {
    let Ok(mut n) = q.get_single_mut() else { return };
    n.display = if lock.engaged { Display::Flex } else { Display::None };
}

pub fn toggle_execute_prompt(
    boss_q: Query<&Posture, With<Boss>>,
    mut prompt_q: Query<&mut Node, With<ExecutePrompt>>,
) {
    let Ok(mut n) = prompt_q.get_single_mut() else { return };
    let visible = boss_q.get_single().map(|p| p.is_broken()).unwrap_or(false);
    n.display = if visible { Display::Flex } else { Display::None };
}

/// Bumps the damage-flash alpha on every connected hit landing on the player.
/// Decays exponentially each frame; the player should see a brief red wash on
/// impact that fades over ~0.4s.
pub fn drive_damage_flash_system(
    time: Res<Time<Real>>,
    mut events: EventReader<HitEvent>,
    player_q: Query<Entity, With<Player>>,
    mut flash_q: Query<(&mut DamageFlash, &mut BackgroundColor)>,
) {
    let player_entity = player_q.get_single().ok();
    let mut bump: f32 = 0.0;
    for ev in events.read() {
        if Some(ev.victim) != player_entity {
            continue;
        }
        match ev.kind {
            HitKind::Connected => bump = bump.max(0.45),
            HitKind::Killed => bump = bump.max(0.80),
            HitKind::Blocked => bump = bump.max(0.15),
            _ => {}
        }
    }
    let Ok((mut flash, mut bg)) = flash_q.get_single_mut() else {
        return;
    };
    flash.alpha = (flash.alpha + bump).min(0.85);
    // Decay: ~3.0 per second so a peak alpha lasts ~0.27s before disappearing.
    let dt = time.delta_secs();
    flash.alpha = (flash.alpha - dt * 3.0).max(0.0);
    bg.0 = Color::srgba(0.85, 0.05, 0.05, flash.alpha);
}

/// Drives the low-HP vignette: invisible above 35% HP, visible-and-pulsing
/// below. Uses BorderColor so we get an edge-only red glow instead of
/// blacking out the whole screen.
pub fn drive_low_hp_vignette_system(
    time: Res<Time<Real>>,
    player_q: Query<&Health, With<Player>>,
    mut vignette_q: Query<&mut BorderColor, With<LowHpVignette>>,
) {
    let Ok(mut border) = vignette_q.get_single_mut() else {
        return;
    };
    let Ok(h) = player_q.get_single() else {
        border.0 = Color::NONE;
        return;
    };
    let ratio = (h.current / h.max).clamp(0.0, 1.0);
    if ratio > 0.35 {
        border.0 = Color::srgba(0.7, 0.0, 0.0, 0.0);
        return;
    }
    // 0.0 at ratio=0.35, 1.0 at ratio=0.0.
    let urgency = ((0.35 - ratio) / 0.35).clamp(0.0, 1.0);
    // Slow heartbeat — 2.4 Hz feels alarming without being seizure-inducing.
    let pulse = (time.elapsed_secs() * 2.4 * std::f32::consts::TAU).sin() * 0.5 + 0.5;
    let alpha = (0.25 + 0.55 * urgency) * (0.6 + 0.4 * pulse);
    border.0 = Color::srgba(0.85, 0.0, 0.0, alpha);
}
