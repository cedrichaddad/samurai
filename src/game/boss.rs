use bevy::prelude::*;

#[derive(Component)]
pub struct Boss;

#[derive(Component, Default)]
pub struct BossAttackCooldown {
    pub timer: Timer,
}

pub fn spawn_boss(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Boss,
        crate::game::combat::CharacterState::default(),
        crate::game::combat::ActionTimer::default(),
        crate::game::combat::Health::default(),
        crate::game::combat::Hitbox::default(),
        crate::game::combat::Hurtbox::default(),
        Mesh3d(meshes.add(Capsule3d::new(0.6, 2.0))),
        MeshMaterial3d(materials.add(Color::srgb(1.0, 0.0, 0.0))),
        Transform::from_xyz(2.0, 1.0, 0.0),
    ))
    .insert(crate::game::aggression::BossAggressionTimer::default())
    .insert(crate::game::ai::PreviousPosition::default())
    .insert(crate::game::ai::BossMemory::default())
    .insert(BossAttackCooldown::default());
}
