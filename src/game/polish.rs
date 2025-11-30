use bevy::prelude::*;
use crate::game::combat::{Health, Hitbox};
use crate::game::player::Player;
use crate::game::boss::Boss;


#[derive(Component)]
pub struct PlayerHealthText;

#[derive(Component)]
pub struct BossHealthText;

#[derive(Component)]
pub struct BossStateText;

pub fn setup_ui(mut commands: Commands) {
    // Player Health (Top Left)
    commands.spawn((
        Text::new("Player HP: 100"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            left: Val::Px(10.0),
            ..default()
        },
        PlayerHealthText,
        TextColor(Color::WHITE),
    ));

    // Boss Health (Top Right)
    commands.spawn((
        Text::new("Boss HP: 100"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            right: Val::Px(10.0),
            ..default()
        },
        BossHealthText,
        TextColor(Color::WHITE),
    ));

    // Boss State (Above Boss - actually just Top Center for now for simplicity)
    commands.spawn((
        Text::new("State: Idle"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(50.0),
            right: Val::Px(10.0),
            ..default()
        },
        BossStateText,
        TextColor(Color::srgb(1.0, 1.0, 0.0)),
    ));
}

pub fn update_health_ui(
    player_health_query: Query<&Health, With<Player>>,
    boss_health_query: Query<&Health, With<Boss>>,
    mut player_text_query: Query<&mut Text, (With<PlayerHealthText>, Without<BossHealthText>)>,
    mut boss_text_query: Query<&mut Text, (With<BossHealthText>, Without<PlayerHealthText>)>,
) {
    if let Ok(health) = player_health_query.get_single() {
        for mut text in &mut player_text_query {
            **text = format!("Player HP: {:.0}", health.current);
        }
    }

    if let Ok(health) = boss_health_query.get_single() {
        for mut text in &mut boss_text_query {
            **text = format!("Boss HP: {:.0}", health.current);
        }
    }
}

pub fn update_boss_visuals(
    boss_query: Query<(&crate::game::combat::CharacterState, &MeshMaterial3d<StandardMaterial>), With<Boss>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut text_query: Query<&mut Text, With<BossStateText>>,
) {
    if let Ok((state, mat_handle)) = boss_query.get_single() {
        // Update Text
        for mut text in &mut text_query {
            **text = format!("State: {:?}", state);
        }

        // Update Color
        if let Some(material) = materials.get_mut(mat_handle) {
            material.base_color = match state {
                crate::game::combat::CharacterState::Idle => Color::WHITE,
                crate::game::combat::CharacterState::Move => Color::srgb(0.8, 0.8, 0.8), // Light Grey
                crate::game::combat::CharacterState::Attack => Color::srgb(1.0, 0.0, 0.0), // Red
                crate::game::combat::CharacterState::Parry => Color::srgb(0.0, 0.0, 1.0), // Blue
                crate::game::combat::CharacterState::Dodge => Color::srgb(0.0, 1.0, 0.0), // Green
                crate::game::combat::CharacterState::Stunned => Color::srgb(1.0, 1.0, 0.0), // Yellow
            };
        }
    }
}

// Dynamic Difficulty Adjustment
// If player is winning too easily (high health diff), make boss more aggressive?
// Since our model is fixed, we can't change policy easily.
// But we can adjust stats (damage, speed) or reaction time.

#[derive(Resource, Default)]
pub struct Difficulty {
    pub level: f32, // 0.0 to 1.0
}

pub fn dda_system(
    mut difficulty: ResMut<Difficulty>,
    player_query: Query<&Health, With<Player>>,
    boss_query: Query<&Health, With<Boss>>,
) {
    let player_health = if let Ok(h) = player_query.get_single() { h.current } else { return };
    let boss_health = if let Ok(h) = boss_query.get_single() { h.current } else { return };
    
    // If player has much more health than boss, increase difficulty
    if player_health > boss_health + 20.0 {
        difficulty.level = (difficulty.level + 0.01).min(1.0);
    } else if boss_health > player_health + 20.0 {
        difficulty.level = (difficulty.level - 0.01).max(0.0);
    }
    
    // Use difficulty to scale boss damage?
    // We need to access boss hitbox and modify damage.
}

pub fn apply_difficulty(
    difficulty: Res<Difficulty>,
    mut boss_query: Query<&mut Hitbox, With<Boss>>,
) {
    for mut hitbox in &mut boss_query {
        // Base damage 10.0
        hitbox.damage = 10.0 + (difficulty.level * 10.0); // Up to 20.0 damage
    }
}
