use bevy::prelude::*;
use crate::game::player::Player;
use crate::game::boss::Boss;

#[derive(Component)]
pub struct MainCamera;

pub fn camera_follow(
    mut camera_query: Query<&mut Transform, With<MainCamera>>,
    player_query: Query<&Transform, (With<Player>, Without<Boss>, Without<MainCamera>)>,
    boss_query: Query<&Transform, (With<Boss>, Without<Player>, Without<MainCamera>)>,
) {
    let mut camera_transform = camera_query.single_mut();
    
    let player_pos = if let Ok(t) = player_query.get_single() {
        t.translation
    } else {
        return;
    };

    let boss_pos = if let Ok(t) = boss_query.get_single() {
        t.translation
    } else {
        // If boss is dead or not spawned, just look at player
        player_pos
    };

    let midpoint = (player_pos + boss_pos) / 2.0;
    
    // Position camera at a fixed offset from midpoint, but maybe rotate around?
    // For now, simple fixed offset relative to midpoint
    let offset = Vec3::new(0.0, 5.0, 10.0);
    let target_pos = midpoint + offset;

    // Smoothly interpolate? For now, direct set
    camera_transform.translation = camera_transform.translation.lerp(target_pos, 0.1);
    camera_transform.look_at(midpoint, Vec3::Y);
}
