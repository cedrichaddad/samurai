use bevy::prelude::*;

#[derive(Resource, Default)]
pub struct PlayerStats {
    pub parry_count: u32,
    pub dodge_count: u32,
    pub attack_count: u32,
    pub last_action_time: f32,
}

pub fn track_player_stats(
    mut stats: ResMut<PlayerStats>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
) {
    if keyboard_input.just_pressed(KeyCode::Space) {
        stats.attack_count += 1;
        stats.last_action_time = time.elapsed_secs();
    } else if keyboard_input.just_pressed(KeyCode::KeyQ) {
        stats.parry_count += 1;
        stats.last_action_time = time.elapsed_secs();
    } else if keyboard_input.just_pressed(KeyCode::ShiftLeft) {
        stats.dodge_count += 1;
        stats.last_action_time = time.elapsed_secs();
    }
    
    // Decay stats over time? Or just keep raw count?
    // Spec says "frequency ... in the last N seconds".
    // For simplicity, we just track total for now, or maybe reset every 10s?
    
    if time.elapsed_secs() % 10.0 < time.delta_secs() {
        // Reset every ~10 seconds to keep it "recent"
        stats.parry_count = 0;
        stats.dodge_count = 0;
        stats.attack_count = 0;
    }
}
