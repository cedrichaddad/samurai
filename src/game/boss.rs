use bevy::prelude::*;
use crate::game::combat::CharacterState;
use crate::game::feel::{COMBAT_BLEND_MS, NAV_BLEND_MS, STUN_BLEND_MS};

// --- RESOURCES ---
#[derive(Resource)]
pub struct BossAssets {
    pub scene: Handle<Scene>,
    // Store animations by logical name for safety
    pub anim_idle: Handle<AnimationClip>,
    pub anim_run: Handle<AnimationClip>,
    pub anim_attack: Handle<AnimationClip>,
    pub anim_parry: Handle<AnimationClip>,
    pub anim_dodge: Handle<AnimationClip>,
    pub anim_hit: Handle<AnimationClip>,
}

pub fn load_boss_assets(mut commands: Commands, asset_server: Res<AssetServer>) {
    // CAUTION: Ensure these indices match your Blender Script config order!
    // 0: Idle, 1: Attack, 2: Parry, 3: Dodge, 4: Run, 5: Hit
    commands.insert_resource(BossAssets {
        scene: asset_server.load("boss.glb#Scene0"),
        anim_idle: asset_server.load("boss.glb#Animation0"),
        anim_attack: asset_server.load("boss.glb#Animation1"),
        anim_parry: asset_server.load("boss.glb#Animation2"),
        anim_dodge: asset_server.load("boss.glb#Animation3"),
        anim_run: asset_server.load("boss.glb#Animation4"),
        anim_hit: asset_server.load("boss.glb#Animation5"),
    });
}

// --- COMPONENTS ---
#[derive(Component)]
pub struct Boss;

#[derive(Component, Default)]
pub struct BossAttackCooldown {
    pub timer: Timer,
}

#[derive(Component, Default)]
pub struct BossFeintTimer {
    pub timer: Timer,
    pub active: bool,
}

#[derive(Component)]
pub struct BossAnimationIndices {
    pub idle: AnimationNodeIndex,
    pub run: AnimationNodeIndex,
    pub attack: AnimationNodeIndex,
    pub parry: AnimationNodeIndex,
    pub dodge: AnimationNodeIndex,
    pub hit: AnimationNodeIndex,
}

// --- SPAWNING ---
pub fn spawn_boss_for_stage(
    mut commands: Commands,
    assets: Option<Res<BossAssets>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    existing: Query<Entity, With<Boss>>,
    cfg: Option<Res<crate::game::rush::CurrentBossConfig>>,
) {
    // Already alive? Don't spawn a duplicate.
    if existing.iter().next().is_some() {
        return;
    }
    let Some(assets) = assets else {
        // BossAssets hasn't loaded yet (we're in early Startup); skip silently.
        return;
    };

    let (max_hp, damage_mult) = cfg
        .as_ref()
        .and_then(|c| c.0.as_ref().map(|b| (b.max_hp, b.damage_mult)))
        .unwrap_or((100.0, 1.0));

    println!("Spawning Boss with hp={max_hp}, dmg×{damage_mult}");
    let mut graph = AnimationGraph::new();
    let idle = graph.add_clip(assets.anim_idle.clone(), 1.0, graph.root);
    let run = graph.add_clip(assets.anim_run.clone(), 1.0, graph.root);
    let attack = graph.add_clip(assets.anim_attack.clone(), 1.0, graph.root);
    let parry = graph.add_clip(assets.anim_parry.clone(), 1.0, graph.root);
    let dodge = graph.add_clip(assets.anim_dodge.clone(), 1.0, graph.root);
    let hit = graph.add_clip(assets.anim_hit.clone(), 1.0, graph.root);

    let graph_handle = graphs.add(graph);

    let posture_max = cfg
        .as_ref()
        .and_then(|c| c.0.as_ref().map(|b| b.posture_max))
        .unwrap_or(130.0);

    commands
        .spawn((
            Boss,
            SceneRoot(assets.scene.clone()),
            Transform::from_xyz(2.0, 0.0, 0.0),
            BossAnimationIndices { idle, run, attack, parry, dodge, hit },
            crate::game::combat::CharacterState::Idle,
            crate::game::combat::ActionTimer::default(),
            crate::game::combat::FrameWindow::default(),
            crate::game::combat::Health { current: max_hp, max: max_hp },
            crate::game::combat::Hitbox {
                radius: 1.0,
                offset: Vec3::new(0.0, 1.5, 1.0),
                damage: 10.0 * damage_mult,
                ..default()
            },
            crate::game::combat::Hurtbox::default(),
            crate::game::combat::Velocity::default(),
            crate::game::posture::Posture::new(posture_max),
        ))
        .insert((
            crate::game::aggression::BossAggressionTimer::default(),
            crate::game::ai::PreviousPosition::default(),
            crate::game::ai::BossMemory::default(),
            BossAttackCooldown::default(),
            BossFeintTimer {
                timer: Timer::from_seconds(5.0, TimerMode::Once),
                active: false,
            },
            crate::game::vfx::DodgeDustOnce::default(),
        ))
        .insert(AnimationGraphHandle(graph_handle))
        .insert(AnimationTransitions::default())
        .with_children(|c| {
            // Per-style colored capsule placeholder. Same intent as the player's
            // child mesh: guaranteed-visible body that moves with the boss
            // entity even if the GLB scene loader doesn't unpack here.
            let style = cfg.as_ref().and_then(|c| c.0.as_ref().map(|b| b.style));
            let body_color = match style {
                Some(crate::game::fusion::BossStyle::Mimic) => Color::srgb(0.80, 0.82, 0.88),
                Some(crate::game::fusion::BossStyle::CounterSage) => Color::srgb(0.95, 0.62, 0.20),
                Some(crate::game::fusion::BossStyle::PatternBreaker) => Color::srgb(0.60, 0.32, 0.80),
                Some(crate::game::fusion::BossStyle::MemoryEater) => Color::srgb(0.95, 0.18, 0.18),
                _ => Color::srgb(0.95, 0.40, 0.25),
            };
            c.spawn((
                Mesh3d(meshes.add(Capsule3d::new(0.40, 1.1))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: body_color,
                    emissive: body_color.to_linear() * 0.80,
                    metallic: 0.35,
                    perceptual_roughness: 0.40,
                    cull_mode: None,
                    ..default()
                })),
                Transform::from_xyz(0.0, 0.95, 0.0),
            ));
        });
}

/// Restore the player's HP to the carry-over value from `RunState` when a
/// stage starts.
pub fn restore_player_hp_for_stage(
    run: Res<crate::game::rush::RunState>,
    mut players: Query<&mut crate::game::combat::Health, With<crate::game::player::Player>>,
) {
    if let Ok(mut hp) = players.get_single_mut() {
        hp.current = run.player_hp_carry.min(hp.max).max(1.0);
    }
}

// --- ANIMATION CONTROLLER ---
pub fn boss_animation_controller(
    // Trigger only when state changes to prevent restarting animations every frame
    boss_query: Query<(Entity, &CharacterState, &BossAnimationIndices), (With<Boss>, Changed<CharacterState>)>,
    children_query: Query<&Children>,
    mut animation_players: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    for (entity, state, indices) in boss_query.iter() {
        for descendant in children_query.iter_descendants(entity) {
            if let Ok((mut player, mut transitions)) = animation_players.get_mut(descendant) {
                let (index, should_repeat, blend_ms) = match state {
                    CharacterState::Idle => (indices.idle, true, NAV_BLEND_MS),
                    CharacterState::Move => (indices.run, true, NAV_BLEND_MS),
                    CharacterState::Attack => (indices.attack, false, COMBAT_BLEND_MS),
                    CharacterState::Parry | CharacterState::Block => {
                        (indices.parry, false, COMBAT_BLEND_MS)
                    }
                    CharacterState::Dodge => (indices.dodge, false, COMBAT_BLEND_MS),
                    CharacterState::Stunned => (indices.hit, false, STUN_BLEND_MS),
                };

                transitions.play(&mut player, index, std::time::Duration::from_millis(blend_ms));

                if should_repeat {
                    player.animation_mut(index).unwrap().repeat();
                }
                break;
            }
        }
    }
}

pub fn link_animation_components(
    mut commands: Commands,
    // Query for the Root entity that has the Graph but hasn't found its child yet
    root_query: Query<(Entity, &AnimationGraphHandle), With<Boss>>,
    children_query: Query<&Children>,
    // Query to find which child has the Player
    player_query: Query<Entity, With<AnimationPlayer>>,
) {
    for (root_entity, graph_handle) in &root_query {
        // Recursive search for the child with AnimationPlayer
        for descendant in children_query.iter_descendants(root_entity) {
            if player_query.contains(descendant) {
                // Found the child! Give it the necessary components
                commands.entity(descendant).insert((
                    graph_handle.clone(),
                    AnimationTransitions::default(),
                ));
                
                // Remove handle from root so we don't process this again
                commands.entity(root_entity).remove::<AnimationGraphHandle>();
                println!("Animation system linked for Boss!");
                break;
            }
        }
    }
}
