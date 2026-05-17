//! Audio: load handles, play one-shots on combat events, footsteps, BGM per
//! AppState. Asset files live under `assets/audio/`. Missing files emit a
//! warning but don't break the game.

use crate::game::boss::Boss;
use crate::game::combat::CharacterState;
use crate::game::hitstop::{HitEvent, HitKind};
use crate::game::player::Player;
use crate::game::rush::AppState;
use bevy::prelude::*;

#[derive(Resource)]
pub struct AudioAssets {
    pub swoosh_light: Handle<AudioSource>,
    pub swoosh_heavy: Handle<AudioSource>,
    pub clang_parry: Handle<AudioSource>,
    pub clang_perfect: Handle<AudioSource>,
    pub thud_hit: Handle<AudioSource>,
    pub thud_kill: Handle<AudioSource>,
    pub footstep: Handle<AudioSource>,
    pub dodge_whoosh: Handle<AudioSource>,
    pub posture_break: Handle<AudioSource>,
    pub stage_clear: Handle<AudioSource>,
    pub stage_fail: Handle<AudioSource>,
    pub bgm_menu: Handle<AudioSource>,
    pub bgm_stage: Handle<AudioSource>,
    pub bgm_intermission: Handle<AudioSource>,
    pub bgm_victory: Handle<AudioSource>,
    pub bgm_gameover: Handle<AudioSource>,
}

pub fn load_audio_assets(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(AudioAssets {
        swoosh_light: asset_server.load("audio/swoosh_light.ogg"),
        swoosh_heavy: asset_server.load("audio/swoosh_heavy.ogg"),
        clang_parry: asset_server.load("audio/clang_parry.ogg"),
        clang_perfect: asset_server.load("audio/clang_perfect.ogg"),
        thud_hit: asset_server.load("audio/thud_hit.ogg"),
        thud_kill: asset_server.load("audio/thud_kill.ogg"),
        footstep: asset_server.load("audio/footstep.ogg"),
        dodge_whoosh: asset_server.load("audio/dodge_whoosh.ogg"),
        posture_break: asset_server.load("audio/posture_break.ogg"),
        stage_clear: asset_server.load("audio/stage_clear.ogg"),
        stage_fail: asset_server.load("audio/stage_fail.ogg"),
        bgm_menu: asset_server.load("audio/bgm_menu.ogg"),
        bgm_stage: asset_server.load("audio/bgm_stage.ogg"),
        bgm_intermission: asset_server.load("audio/bgm_intermission.ogg"),
        bgm_victory: asset_server.load("audio/bgm_victory.ogg"),
        bgm_gameover: asset_server.load("audio/bgm_gameover.ogg"),
    });
}

#[derive(Component)]
pub struct OneShotAudio;

#[derive(Component)]
pub struct CurrentBgm;

fn one_shot(commands: &mut Commands, source: Handle<AudioSource>, volume: f32) {
    commands.spawn((
        AudioPlayer(source),
        PlaybackSettings {
            mode: bevy::audio::PlaybackMode::Despawn,
            volume: bevy::audio::Volume::new(volume),
            ..default()
        },
        OneShotAudio,
    ));
}

pub fn play_audio_on_hit_events_system(
    mut commands: Commands,
    audio: Res<AudioAssets>,
    mut events: EventReader<HitEvent>,
) {
    for ev in events.read() {
        match ev.kind {
            HitKind::Connected => one_shot(&mut commands, audio.thud_hit.clone(), 0.7),
            HitKind::Blocked => one_shot(&mut commands, audio.clang_parry.clone(), 0.45),
            HitKind::Killed => one_shot(&mut commands, audio.thud_kill.clone(), 0.9),
            HitKind::Parried => one_shot(&mut commands, audio.clang_parry.clone(), 0.7),
            HitKind::PerfectParried => one_shot(&mut commands, audio.clang_perfect.clone(), 0.9),
        }
    }
}

#[derive(Resource, Default)]
pub struct FootstepTimer(pub f32);

#[derive(Resource, Default)]
pub struct BossFootstepTimer(pub f32);

pub fn play_footstep_system(
    time: Res<Time>,
    mut timer: ResMut<FootstepTimer>,
    audio: Option<Res<AudioAssets>>,
    mut commands: Commands,
    q: Query<&CharacterState, With<Player>>,
) {
    let Some(audio) = audio else { return };
    let Ok(state) = q.get_single() else { return };
    let dt = time.delta_secs();
    if *state == CharacterState::Move {
        timer.0 += dt;
        if timer.0 >= 0.30 {
            timer.0 = 0.0;
            one_shot(&mut commands, audio.footstep.clone(), 0.4);
        }
    } else {
        timer.0 = 0.25;
    }
}

/// Boss footsteps — slightly slower cadence and slightly lower volume so the
/// player still tracks their own movement first.
pub fn play_boss_footstep_system(
    time: Res<Time>,
    mut timer: ResMut<BossFootstepTimer>,
    audio: Option<Res<AudioAssets>>,
    mut commands: Commands,
    q: Query<&CharacterState, With<Boss>>,
) {
    let Some(audio) = audio else { return };
    let Ok(state) = q.get_single() else { return };
    let dt = time.delta_secs();
    if *state == CharacterState::Move {
        timer.0 += dt;
        if timer.0 >= 0.36 {
            timer.0 = 0.0;
            one_shot(&mut commands, audio.footstep.clone(), 0.30);
        }
    } else {
        timer.0 = 0.30;
    }
}

#[derive(Resource, Default)]
pub struct LastBgmState(pub Option<AppState>);

pub fn switch_bgm_on_state_change_system(
    mut commands: Commands,
    audio: Option<Res<AudioAssets>>,
    state: Res<State<AppState>>,
    mut last: ResMut<LastBgmState>,
    existing: Query<Entity, With<CurrentBgm>>,
) {
    let Some(audio) = audio else { return };
    let cur = state.get().clone();
    if last.0.as_ref() == Some(&cur) {
        return;
    }
    for e in &existing {
        commands.entity(e).despawn();
    }
    let source = match cur {
        AppState::MainMenu => audio.bgm_menu.clone(),
        AppState::Stage => audio.bgm_stage.clone(),
        AppState::Intermission => audio.bgm_intermission.clone(),
        AppState::Victory => audio.bgm_victory.clone(),
        AppState::GameOver => audio.bgm_gameover.clone(),
    };
    commands.spawn((
        AudioPlayer(source),
        PlaybackSettings {
            mode: bevy::audio::PlaybackMode::Loop,
            volume: bevy::audio::Volume::new(0.35),
            ..default()
        },
        CurrentBgm,
    ));
    last.0 = Some(cur);
}

/// Played as a one-shot stinger from `watch_stage_outcome` when the player
/// clears or fails a stage. The stage transition itself fires after this.
pub fn play_stage_outcome_stinger(commands: &mut Commands, audio: &AudioAssets, win: bool) {
    let source = if win {
        audio.stage_clear.clone()
    } else {
        audio.stage_fail.clone()
    };
    one_shot(commands, source, 0.8);
}

/// Posture break stinger.
pub fn play_posture_break(commands: &mut Commands, audio: &AudioAssets) {
    one_shot(commands, audio.posture_break.clone(), 0.8);
}

/// Player attack swoosh — wired by the `player_combat` path indirectly: a
/// `Changed<CharacterState>` listener triggers swoosh on Attack-state entry.
/// FrameWindow.startup distinguishes light (~6) from heavy (>=12), so we can
/// pick the correct swoosh asset.
pub fn play_combat_swoosh_system(
    mut commands: Commands,
    audio: Option<Res<AudioAssets>>,
    q: Query<
        (&CharacterState, &crate::game::combat::FrameWindow),
        (Changed<CharacterState>, With<Player>),
    >,
) {
    let Some(audio) = audio else { return };
    for (state, win) in &q {
        match state {
            CharacterState::Attack => {
                let handle = if win.startup >= 12 {
                    audio.swoosh_heavy.clone()
                } else {
                    audio.swoosh_light.clone()
                };
                one_shot(&mut commands, handle, 0.55);
            }
            CharacterState::Dodge => one_shot(&mut commands, audio.dodge_whoosh.clone(), 0.5),
            _ => {}
        }
    }
}

/// Stinger on posture break: monitors all entities for a posture transition
/// from "not broken" → "broken" via the `Changed<Posture>` filter combined
/// with a per-entity sentinel.
pub fn play_posture_break_system(
    mut commands: Commands,
    audio: Option<Res<AudioAssets>>,
    q: Query<&crate::game::posture::Posture, Changed<crate::game::posture::Posture>>,
    mut last_broken: Local<bevy::utils::HashMap<u32, bool>>,
) {
    let Some(audio) = audio else { return };
    let mut any_new_break = false;
    for p in &q {
        // Treat the resource as scalar — only emit once per fresh break across
        // the whole arena. We don't track per-entity here because we only have
        // two combatants in the current scope.
        let key = 0u32;
        let was = last_broken.get(&key).copied().unwrap_or(false);
        let now = p.is_broken();
        if now && !was {
            any_new_break = true;
        }
        last_broken.insert(key, now);
    }
    if any_new_break {
        play_posture_break(&mut commands, &audio);
    }
}

/// Play a stinger when the AppState transitions out of `Stage` into either
/// Intermission (clear) or GameOver (fail).
pub fn play_stage_outcome_stinger_system(
    mut commands: Commands,
    audio: Option<Res<AudioAssets>>,
    state: Res<State<crate::game::rush::AppState>>,
    mut last_state: Local<Option<crate::game::rush::AppState>>,
) {
    let Some(audio) = audio else { return };
    let current = state.get().clone();
    if last_state.as_ref() == Some(&current) {
        return;
    }
    let was_in_stage = matches!(*last_state, Some(crate::game::rush::AppState::Stage));
    *last_state = Some(current.clone());
    if !was_in_stage {
        return;
    }
    match current {
        crate::game::rush::AppState::Intermission | crate::game::rush::AppState::Victory => {
            play_stage_outcome_stinger(&mut commands, &audio, true);
        }
        crate::game::rush::AppState::GameOver => {
            play_stage_outcome_stinger(&mut commands, &audio, false);
        }
        _ => {}
    }
}
