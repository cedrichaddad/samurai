use bevy::prelude::*;

mod game;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_systems(Startup, (game::setup::setup_scene, game::player::spawn_player, game::boss::spawn_boss))
        .add_systems(Update, (
            game::camera::camera_follow,
            game::polish::update_health_ui,
            game::polish::update_boss_visuals,
            close_on_esc
        ))
        .add_systems(FixedUpdate, (
            game::combat::update_action_timers,
            game::combat::prevent_character_overlap,
            game::player::player_movement,
            game::player::player_combat,
            game::combat::update_action_timers,
            game::combat::combat_collision,
            game::ai::boss_ai_system,
            game::polish::dda_system,
            game::polish::apply_difficulty,
            game::stats::track_player_stats,
        ).chain())
        .init_resource::<game::polish::Difficulty>()
        .init_resource::<game::stats::PlayerStats>()
        .insert_resource(Time::<Fixed>::from_hz(60.0))
        .add_systems(Startup, (game::ai::load_boss_model, game::polish::setup_ui))
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
