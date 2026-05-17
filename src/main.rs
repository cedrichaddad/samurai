use bevy::prelude::*;

mod game;

use game::audio::{
    load_audio_assets, play_audio_on_hit_events_system, play_boss_footstep_system,
    play_combat_swoosh_system, play_footstep_system, play_posture_break_system,
    play_stage_outcome_stinger_system, switch_bgm_on_state_change_system,
    BossFootstepTimer, FootstepTimer, LastBgmState,
};
use game::camera::{
    camera_follow, drive_slowmo_from_kill_events_system, tick_fov_tween_system,
    tick_slowmo_system, SlowMo,
};
use game::combat::{combat_collision, tick_frame_windows_system};
use game::dossier::Dossier;
use game::hitstop::{
    apply_hit_stop_from_events_system, no_hit_stop, tick_hit_stop_system, HitEvent, HitStop,
};
use game::hud::{
    cleanup_hud, drive_damage_flash_system, drive_low_hp_vignette_system, spawn_hud,
    toggle_execute_prompt, toggle_lockon_reticle, update_boss_hud, update_player_hud,
};
use game::input::{read_input_to_buffer_system, InputBuffer};
use game::instinct::Instinct;
use game::lockon::{face_target_system, toggle_lock_on_system, LockOn};
use game::memory::{BossMemoryDb, PredictedPlayerWindow};
use game::memory_systems::{identify_player_pattern_system, ingest_player_memory_system};
use game::posture::decay_posture_system;
use game::rush::{
    cleanup_rush_ui, current_boss, enter_stage_setup, handle_continue_button,
    handle_start_button, spawn_gameover, spawn_intermission, spawn_main_menu, spawn_victory,
    watch_stage_outcome, AppState, CurrentBossConfig, RunState,
};
use game::shake::{
    decay_trauma_system, ingest_hit_events_into_trauma_system, Trauma,
};
use game::spectacle::{
    cleanup_intro_overlay, enter_stage_start_intro, fade_out_intro_overlay, no_intro,
    punch_fov_on_kill_system, spawn_intro_overlay, tick_intro_system, StageIntro,
};
use game::vfx::{
    drive_stunned_glow_system, emit_slash_trail_system, pulse_unblockable_outline_system,
    spawn_dust_on_dodge_system, spawn_material_flash_on_event_system,
    spawn_parry_sparks_on_event_system, tick_material_flash_system, tick_particles_system,
};

fn main() {
    let dossier = match Dossier::open_or_create() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("dossier init failed: {e}; starting in volatile mode");
            let path = std::path::PathBuf::from(".samurai_local");
            std::fs::create_dir_all(&path).ok();
            Dossier {
                root: path.clone(),
                vdb_path: path.join("dossier.vdb"),
                meta_path: path.join("dossier.meta.json"),
                meta: Default::default(),
            }
        }
    };
    let memory = BossMemoryDb::open_or_create(dossier.vdb_path.clone());
    let mut run_state = RunState::default();
    let mut current_cfg = CurrentBossConfig(current_boss(&run_state));
    if current_cfg.0.is_none() {
        run_state = RunState::default();
        current_cfg = CurrentBossConfig(current_boss(&run_state));
    }

    App::new()
        .add_plugins(DefaultPlugins)
        .insert_state(AppState::default())
        .add_event::<HitEvent>()
        .insert_resource(dossier)
        .insert_resource(memory)
        .insert_resource(run_state)
        .insert_resource(current_cfg)
        .init_resource::<Instinct>()
        .init_resource::<PredictedPlayerWindow>()
        .init_resource::<game::polish::Difficulty>()
        .init_resource::<game::stats::PlayerStats>()
        .init_resource::<InputBuffer>()
        .init_resource::<HitStop>()
        .init_resource::<Trauma>()
        .init_resource::<LockOn>()
        .init_resource::<SlowMo>()
        .init_resource::<StageIntro>()
        .init_resource::<FootstepTimer>()
        .init_resource::<BossFootstepTimer>()
        .init_resource::<LastBgmState>()
        .insert_resource(Time::<Fixed>::from_hz(60.0))
        // ─── Asset / scene bootstrap ───
        .add_systems(Startup, (
            game::boss::load_boss_assets,
            game::player::load_player_assets,
            load_audio_assets,
        ))
        .add_systems(Startup, (
            game::setup::setup_scene,
            game::player::spawn_player,
        ).after(game::boss::load_boss_assets).after(game::player::load_player_assets))
        .add_systems(Startup, (game::ai::load_boss_model, game::polish::setup_ui))
        // ─── State transitions ───
        .add_systems(OnEnter(AppState::MainMenu), spawn_main_menu)
        .add_systems(OnExit(AppState::MainMenu), cleanup_rush_ui)
        .add_systems(Update, handle_start_button.run_if(in_state(AppState::MainMenu)))

        .add_systems(OnEnter(AppState::Stage), (
            enter_stage_setup,
            game::boss::spawn_boss_for_stage,
            game::boss::restore_player_hp_for_stage,
            spawn_hud,
            enter_stage_start_intro,
            spawn_intro_overlay,
        ).chain())
        .add_systems(OnExit(AppState::Stage), (cleanup_hud, cleanup_intro_overlay))

        .add_systems(OnEnter(AppState::Intermission), spawn_intermission)
        .add_systems(OnExit(AppState::Intermission), cleanup_rush_ui)
        .add_systems(Update, handle_continue_button.run_if(in_state(AppState::Intermission)))

        .add_systems(OnEnter(AppState::Victory), spawn_victory)
        .add_systems(OnExit(AppState::Victory), cleanup_rush_ui)

        .add_systems(OnEnter(AppState::GameOver), (
            game::spectacle::spawn_death_fade,
            spawn_gameover,
        ))
        .add_systems(Update, game::spectacle::tick_death_fade.run_if(in_state(AppState::GameOver)))
        .add_systems(OnExit(AppState::GameOver), (
            game::spectacle::cleanup_death_fade,
            cleanup_rush_ui,
        ))

        // ─── Always-on Update systems (camera, HUD, animation, VFX) ───
        .add_systems(Update, (
            camera_follow,
            tick_slowmo_system,
            tick_fov_tween_system,
            tick_intro_system,
            fade_out_intro_overlay,
            game::polish::apply_combat_materials,
            game::boss::boss_animation_controller,
            game::boss::link_animation_components,
            game::player::player_animation_controller,
            game::player::link_animation_components,
            close_on_esc,
        ))
        .add_systems(Update, (
            update_player_hud,
            update_boss_hud,
            toggle_lockon_reticle,
            toggle_execute_prompt,
            tick_particles_system,
            tick_material_flash_system,
            pulse_unblockable_outline_system,
            drive_stunned_glow_system,
            drive_damage_flash_system,
            drive_low_hp_vignette_system,
            decay_trauma_system,
            switch_bgm_on_state_change_system,
            play_combat_swoosh_system,
            play_footstep_system,
            play_boss_footstep_system,
            play_posture_break_system,
        ).run_if(in_state(AppState::Stage)))
        // Stage-outcome stinger runs in every state (transition-detector)
        .add_systems(Update, play_stage_outcome_stinger_system)

        // ─── Combat tick (FixedUpdate). Split into chained groups because
        //     Bevy's tuple bundle limit is 20 systems per add_systems call.
        //
        //     IMPORTANT: each system function may appear ONCE in the schedule.
        //     Bevy uses the function-pointer identity as a SystemTypeSet and
        //     ordering against a duplicate function panics at startup with
        //     "more than one SystemTypeSet instance". Cross-group ordering
        //     therefore references `player_combat` (last system of group 1)
        //     rather than re-using update_action_timers as a sync point. ───
        // Group 1: input + intent + state maintenance + action initiation.
        .add_systems(FixedUpdate, (
            game::combat::update_action_timers,
            tick_frame_windows_system,
            read_input_to_buffer_system,
            toggle_lock_on_system,
            face_target_system,
            game::player::auto_snap_attack_aim_system,
            game::player::maintain_block_system,
            game::player::try_execute_broken_boss_system,
            game::player::player_movement,
            game::player::player_combat,
        ).chain().run_if(in_state(AppState::Stage)).run_if(no_hit_stop).run_if(no_intro))
        // Group 2: collision resolution + event-driven feedback (audio,
        // shake, sparks, slash trails, hit-stop, slow-mo). Runs after the
        // last system of group 1 so any newly-spawned action timers and
        // frame windows are visible to combat_collision.
        .add_systems(FixedUpdate, (
            combat_collision,
            apply_hit_stop_from_events_system,
            ingest_hit_events_into_trauma_system,
            spawn_parry_sparks_on_event_system,
            spawn_material_flash_on_event_system,
            spawn_dust_on_dodge_system,
            emit_slash_trail_system,
            play_audio_on_hit_events_system,
            drive_slowmo_from_kill_events_system,
            punch_fov_on_kill_system,
        ).chain()
         .after(game::player::player_combat)
         .run_if(in_state(AppState::Stage)).run_if(no_hit_stop).run_if(no_intro))
        // Group 3: physics + memory + AI + stats. Runs after group 2.
        .add_systems(FixedUpdate, (
            game::combat::prevent_character_overlap,
            game::combat::apply_velocity,
            ingest_player_memory_system,
            identify_player_pattern_system,
            game::ai::boss_ai_system,
            decay_posture_system,
            game::polish::dda_system,
            game::polish::apply_difficulty,
            game::stats::track_player_stats,
        ).chain()
         .after(punch_fov_on_kill_system)
         .run_if(in_state(AppState::Stage)).run_if(no_hit_stop).run_if(no_intro))

        // Hit-stop ticker always runs so it can clear; trauma decays in Update.
        .add_systems(FixedUpdate, tick_hit_stop_system)

        // Stage outcome watcher.
        .add_systems(Update, watch_stage_outcome.run_if(in_state(AppState::Stage)))
        .run();
}

fn close_on_esc(
    mut commands: Commands,
    focused_windows: Query<(Entity, &Window)>,
    input: Res<ButtonInput<KeyCode>>,
) {
    for (window, focus) in focused_windows.iter() {
        if !focus.focused {
            continue;
        }

        if input.just_pressed(KeyCode::Escape) {
            commands.entity(window).despawn();
        }
    }
}
