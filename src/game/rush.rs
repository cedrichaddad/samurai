//! Boss-rush state machine, stage roster, run state, and intermission UI.

use crate::game::boss::Boss;
use crate::game::combat::Health;
use crate::game::dossier::Dossier;
use crate::game::fusion::BossStyle;
use crate::game::memory::BossMemoryDb;
use crate::game::player::Player;
use bevy::prelude::*;

#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    MainMenu,
    Stage,
    Intermission,
    Victory,
    GameOver,
}

#[derive(Resource, Debug, Clone)]
pub struct RunState {
    pub current_stage: u8,
    pub player_hp_carry: f32,
    pub player_max_hp: f32,
    pub stages_cleared: u8,
}

impl Default for RunState {
    fn default() -> Self {
        Self {
            current_stage: 1,
            player_hp_carry: 100.0,
            player_max_hp: 100.0,
            stages_cleared: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BossConfig {
    pub name: &'static str,
    pub flavor: &'static str,
    pub style: BossStyle,
    pub max_hp: f32,
    pub damage_mult: f32,
    pub reaction_delay_ticks: u64,
    pub mistake_rate: f32,
    pub posture_max: f32,
    pub unblockable_chance: f32,
}

pub const BOSS_ROSTER: [BossConfig; 5] = [
    BossConfig {
        name: "Tutorial Sentinel",
        flavor: "A practice dummy with a sword. It does not yet know you.",
        style: BossStyle::None,
        max_hp: 80.0,
        damage_mult: 1.0,
        reaction_delay_ticks: 12, // ~200ms
        mistake_rate: 0.10,
        posture_max: 110.0,
        unblockable_chance: 0.0,
    },
    BossConfig {
        name: "The Mimic",
        flavor: "It has read your last fight. It will move as you moved.",
        style: BossStyle::Mimic,
        max_hp: 100.0,
        damage_mult: 1.0,
        reaction_delay_ticks: 10,
        mistake_rate: 0.07,
        posture_max: 130.0,
        unblockable_chance: 0.0,
    },
    BossConfig {
        name: "Counter-Sage",
        flavor: "Knows when you will parry. Knows when you will run.",
        style: BossStyle::CounterSage,
        max_hp: 110.0,
        damage_mult: 1.1,
        reaction_delay_ticks: 8, // ~133ms
        mistake_rate: 0.05,
        posture_max: 150.0,
        unblockable_chance: 0.10,
    },
    BossConfig {
        name: "Pattern-Breaker",
        flavor: "Refuses the shape of your habits.",
        style: BossStyle::PatternBreaker,
        max_hp: 120.0,
        damage_mult: 1.15,
        reaction_delay_ticks: 7,
        mistake_rate: 0.05,
        posture_max: 175.0,
        unblockable_chance: 0.15,
    },
    BossConfig {
        name: "Memory-Eater",
        flavor: "Has read every match you have ever fought.",
        style: BossStyle::MemoryEater,
        max_hp: 150.0,
        damage_mult: 1.25,
        reaction_delay_ticks: 6, // ~100ms
        mistake_rate: 0.03,
        posture_max: 220.0,
        unblockable_chance: 0.20,
    },
];

#[derive(Resource, Default)]
pub struct CurrentBossConfig(pub Option<BossConfig>);

pub fn current_boss(run: &RunState) -> Option<BossConfig> {
    let idx = (run.current_stage as usize).saturating_sub(1);
    BOSS_ROSTER.get(idx).cloned()
}

#[derive(Component)]
pub struct RushUiRoot;

#[derive(Component)]
pub struct StartButton;

#[derive(Component)]
pub struct ContinueButton;

pub fn spawn_main_menu(mut commands: Commands) {
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.85)),
            RushUiRoot,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("SAMURAI :: BOSS RUSH"),
                TextFont {
                    font_size: 56.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
            parent.spawn((
                Text::new("five duels. one ledger. each boss has read more of you."),
                TextFont {
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::srgba(0.85, 0.85, 0.9, 1.0)),
                Node {
                    margin: UiRect::all(Val::Px(16.0)),
                    ..default()
                },
            ));
            parent
                .spawn((
                    Button,
                    Node {
                        width: Val::Px(220.0),
                        height: Val::Px(60.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        margin: UiRect::all(Val::Px(20.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.20, 0.40, 0.60, 1.0)),
                    StartButton,
                ))
                .with_children(|p| {
                    p.spawn((
                        Text::new("BEGIN"),
                        TextFont {
                            font_size: 28.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });
        });
}

pub fn handle_start_button(
    interactions: Query<&Interaction, (Changed<Interaction>, With<StartButton>)>,
    mut next: ResMut<NextState<AppState>>,
    mut run: ResMut<RunState>,
) {
    for interaction in interactions.iter() {
        if *interaction == Interaction::Pressed {
            *run = RunState::default();
            next.set(AppState::Stage);
        }
    }
}

pub fn spawn_intermission(
    mut commands: Commands,
    run: Res<RunState>,
    cfg: Res<CurrentBossConfig>,
) {
    let next_boss = cfg.0.clone();
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::srgba(0.05, 0.05, 0.1, 0.92)),
            RushUiRoot,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new(format!(
                    "STAGE {} CLEAR",
                    run.stages_cleared.max(1)
                )),
                TextFont {
                    font_size: 44.0,
                    ..default()
                },
                TextColor(Color::srgba(0.9, 0.95, 0.7, 1.0)),
            ));
            parent.spawn((
                Text::new(format!(
                    "Carrying {:.0} / {:.0} HP into the next duel",
                    run.player_hp_carry, run.player_max_hp
                )),
                TextFont {
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                Node {
                    margin: UiRect::all(Val::Px(8.0)),
                    ..default()
                },
            ));
            if let Some(b) = next_boss {
                parent.spawn((
                    Text::new(format!("Next: {}", b.name)),
                    TextFont {
                        font_size: 26.0,
                        ..default()
                    },
                    TextColor(Color::srgba(0.95, 0.85, 0.4, 1.0)),
                    Node {
                        margin: UiRect::all(Val::Px(8.0)),
                        ..default()
                    },
                ));
                parent.spawn((
                    Text::new(b.flavor),
                    TextFont {
                        font_size: 16.0,
                        ..default()
                    },
                    TextColor(Color::srgba(0.85, 0.85, 0.95, 1.0)),
                ));
            }
            parent
                .spawn((
                    Button,
                    Node {
                        width: Val::Px(240.0),
                        height: Val::Px(60.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        margin: UiRect::all(Val::Px(20.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.15, 0.55, 0.25, 1.0)),
                    ContinueButton,
                ))
                .with_children(|p| {
                    p.spawn((
                        Text::new("ADVANCE"),
                        TextFont {
                            font_size: 26.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });
        });
}

pub fn handle_continue_button(
    interactions: Query<&Interaction, (Changed<Interaction>, With<ContinueButton>)>,
    mut next: ResMut<NextState<AppState>>,
) {
    for interaction in interactions.iter() {
        if *interaction == Interaction::Pressed {
            next.set(AppState::Stage);
        }
    }
}

pub fn spawn_victory(mut commands: Commands) {
    commands.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            flex_direction: FlexDirection::Column,
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.05, 0.1, 0.92)),
        RushUiRoot,
    ))
    .with_children(|parent| {
        parent.spawn((
            Text::new("VICTORY"),
            TextFont {
                font_size: 80.0,
                ..default()
            },
            TextColor(Color::srgba(1.0, 0.95, 0.6, 1.0)),
        ));
        parent.spawn((
            Text::new("The Memory-Eater has been read in turn."),
            TextFont {
                font_size: 18.0,
                ..default()
            },
            TextColor(Color::WHITE),
        ));
    });
}

pub fn spawn_gameover(mut commands: Commands) {
    commands.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            flex_direction: FlexDirection::Column,
            ..default()
        },
        BackgroundColor(Color::srgba(0.10, 0.0, 0.0, 0.92)),
        RushUiRoot,
    ))
    .with_children(|parent| {
        parent.spawn((
            Text::new("DEFEAT"),
            TextFont {
                font_size: 80.0,
                ..default()
            },
            TextColor(Color::srgba(0.95, 0.4, 0.4, 1.0)),
        ));
        parent.spawn((
            Text::new("Your dossier has grown. Try again."),
            TextFont {
                font_size: 18.0,
                ..default()
            },
            TextColor(Color::WHITE),
        ));
    });
}

pub fn cleanup_rush_ui(mut commands: Commands, q: Query<Entity, With<RushUiRoot>>) {
    for e in &q {
        commands.entity(e).despawn_recursive();
    }
}

/// Watch for boss/player death and drive transitions out of Stage state.
/// Snapshots the in-session memory into the persistent dossier on win.
pub fn watch_stage_outcome(
    state: Res<State<AppState>>,
    mut next: ResMut<NextState<AppState>>,
    bosses: Query<&Health, With<Boss>>,
    players: Query<&Health, With<Player>>,
    mut run: ResMut<RunState>,
    mut memory: ResMut<BossMemoryDb>,
    mut dossier: ResMut<Dossier>,
    mut current_cfg: ResMut<CurrentBossConfig>,
    slowmo: Res<crate::game::camera::SlowMo>,
) {
    if *state.get() != AppState::Stage {
        return;
    }

    // Pacing: hold the AppState transition while the kill-cam slow-mo is
    // playing so the player gets to *see* the killing blow. The slow-mo
    // timer was started from a HitKind::Killed event in the same frame the
    // boss/player died, so checking `slowmo.active()` keeps us in Stage
    // until the cinematic finishes.
    if slowmo.active() {
        return;
    }

    // Player death dominates over boss death.
    let player_alive = players
        .iter()
        .any(|h| h.current > 0.0)
        || players.iter().count() > 0
            && players.iter().all(|h| h.current > 0.0);
    let boss_alive = bosses.iter().count() > 0
        && bosses.iter().any(|h| h.current > 0.0);

    if !player_alive && players.iter().count() == 0 {
        // Player despawned. Game over.
        let _ = memory.snapshot_stage_end();
        next.set(AppState::GameOver);
        return;
    }
    if !boss_alive && bosses.iter().count() == 0 {
        // Boss despawned. Stage clear: snapshot, heal, advance.
        if let Some(player_health) = players.iter().next() {
            let missing = (player_health.max - player_health.current).max(0.0);
            let healed = (player_health.current + missing * 0.25).min(player_health.max);
            run.player_hp_carry = healed;
            run.player_max_hp = player_health.max;
        }
        if let Err(e) = memory.snapshot_stage_end() {
            warn!("dossier snapshot failed: {e}");
        }
        let _ = dossier.advance_tick(memory.tick);

        run.stages_cleared = run.stages_cleared.saturating_add(1);
        if run.current_stage as usize >= BOSS_ROSTER.len() {
            current_cfg.0 = None;
            next.set(AppState::Victory);
        } else {
            run.current_stage = run.current_stage.saturating_add(1);
            current_cfg.0 = current_boss(&run);
            next.set(AppState::Intermission);
        }
    }
}

pub fn enter_stage_setup(
    mut current_cfg: ResMut<CurrentBossConfig>,
    run: Res<RunState>,
) {
    current_cfg.0 = current_boss(&run);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roster_has_five_bosses() {
        assert_eq!(BOSS_ROSTER.len(), 5);
    }

    #[test]
    fn current_boss_indexes_into_roster() {
        let mut run = RunState::default();
        for stage in 1..=5u8 {
            run.current_stage = stage;
            let cfg = current_boss(&run).unwrap();
            assert_eq!(cfg.name, BOSS_ROSTER[(stage as usize) - 1].name);
        }
    }

    #[test]
    fn current_boss_returns_none_past_roster() {
        let mut run = RunState::default();
        run.current_stage = 6;
        assert!(current_boss(&run).is_none());
    }

    #[test]
    fn hp_heal_25_percent_of_missing() {
        // 60/100 → missing=40 → +10 = 70
        let max = 100.0f32;
        let cur = 60.0f32;
        let missing = max - cur;
        let healed = (cur + missing * 0.25).min(max);
        assert!((healed - 70.0).abs() < 1e-4);
    }
}
