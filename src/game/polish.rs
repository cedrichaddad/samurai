//! Per-character materials and dynamic-difficulty adjustment. The old text-
//! only HUD lived here too; that moved to `hud.rs`. We keep `setup_ui` as a
//! no-op shim for backward compatibility with the existing main.rs Startup
//! call order.

use crate::game::boss::Boss;
use crate::game::combat::{Health, Hitbox};
use crate::game::fusion::BossStyle;
use crate::game::player::Player;
use crate::game::rush::CurrentBossConfig;
use bevy::prelude::*;

/// Tag to mark entities whose materials we've already swapped, so we only
/// touch each mesh once.
#[derive(Component)]
pub struct VisualsFixed;

/// Per-boss material colors, picked by `BossStyle`. Each tint includes a
/// healthy emissive lift so the duelists stay readable inside the dim arena
/// even before the directional lights hit them.
fn boss_material_for(style: BossStyle) -> StandardMaterial {
    // (base_color, metallic, roughness, emissive_linear_multiplier)
    let (base, metallic, roughness, emissive_mul) = match style {
        // Tutorial Sentinel — warm bronze.
        BossStyle::None => (Color::srgb(0.62, 0.30, 0.20), 0.10, 0.65, 0.7),
        // The Mimic — silver / chrome.
        BossStyle::Mimic => (Color::srgb(0.78, 0.80, 0.86), 0.90, 0.25, 0.6),
        // Counter-Sage — burnished gold.
        BossStyle::CounterSage => (Color::srgb(0.85, 0.55, 0.22), 0.65, 0.40, 0.8),
        // Pattern-Breaker — deep violet.
        BossStyle::PatternBreaker => (Color::srgb(0.55, 0.30, 0.70), 0.55, 0.50, 1.0),
        // Memory-Eater — obsidian black with hot red-ember glow.
        BossStyle::MemoryEater => (Color::srgb(0.20, 0.10, 0.12), 0.85, 0.35, 1.6),
    };
    StandardMaterial {
        base_color: base,
        metallic,
        perceptual_roughness: roughness,
        reflectance: 0.5,
        emissive: base.to_linear() * emissive_mul,
        cull_mode: None,
        ..default()
    }
}

fn player_material() -> StandardMaterial {
    // Bright indigo-blue. Strong emissive ensures the player reads against
    // any lighting condition the arena lands in.
    let base = Color::srgb(0.42, 0.55, 0.95);
    StandardMaterial {
        base_color: base,
        metallic: 0.30,
        perceptual_roughness: 0.40,
        reflectance: 0.55,
        emissive: base.to_linear() * 0.55,
        cull_mode: None,
        ..default()
    }
}

/// Assign proper PBR materials to every mesh under the player and boss GLBs.
/// Walks *down* from each Player/Boss root using the `Children` graph and
/// stamps a material on any descendant carrying `Mesh3d`. Walking down is
/// more reliable than walking up from each mesh: deep GLB armatures can have
/// chains of 20+ entities, and `Parent` is only set on entities that came
/// from a parent-child spawn — root-of-scene entities may not have one.
pub fn apply_combat_materials(
    mut commands: Commands,
    children_q: Query<&Children>,
    mesh_q: Query<Entity, (With<Mesh3d>, Without<VisualsFixed>)>,
    player_q: Query<Entity, With<Player>>,
    boss_q: Query<Entity, With<Boss>>,
    cfg: Option<Res<CurrentBossConfig>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut diag: Local<MaterialDiag>,
) {
    let boss_style = cfg
        .as_ref()
        .and_then(|c| c.0.as_ref().map(|b| b.style))
        .unwrap_or(BossStyle::None);

    let mut matched_player = 0;
    let mut matched_boss = 0;

    // Player meshes.
    let mut player_mat: Option<Handle<StandardMaterial>> = None;
    for player_e in &player_q {
        let mut stack = vec![player_e];
        while let Some(cur) = stack.pop() {
            if let Ok(children) = children_q.get(cur) {
                for &c in children {
                    stack.push(c);
                }
            }
            if mesh_q.contains(cur) {
                let h = player_mat
                    .get_or_insert_with(|| materials.add(player_material()))
                    .clone();
                commands
                    .entity(cur)
                    .insert(MeshMaterial3d(h))
                    .insert(VisualsFixed);
                matched_player += 1;
                if matched_player <= 4 {
                    println!("Material applied: Player mesh {cur:?}");
                }
            }
        }
    }

    // Boss meshes.
    let mut boss_mat: Option<Handle<StandardMaterial>> = None;
    for boss_e in &boss_q {
        let mut stack = vec![boss_e];
        while let Some(cur) = stack.pop() {
            if let Ok(children) = children_q.get(cur) {
                for &c in children {
                    stack.push(c);
                }
            }
            if mesh_q.contains(cur) {
                let h = boss_mat
                    .get_or_insert_with(|| materials.add(boss_material_for(boss_style)))
                    .clone();
                commands
                    .entity(cur)
                    .insert(MeshMaterial3d(h))
                    .insert(VisualsFixed);
                matched_boss += 1;
                if matched_boss <= 4 {
                    println!("Material applied: Boss mesh {cur:?}");
                }
            }
        }
    }

    diag.frames += 1;
}

#[derive(Default)]
pub struct MaterialDiag {
    pub frames: u64,
}

// ─── Backward-compatibility shim ───────────────────────────────────────────
// main.rs calls `setup_ui` at Startup; the real HUD now lives in hud.rs and
// is spawned on `OnEnter(AppState::Stage)`. We keep this empty so existing
// schedules don't crash.
pub fn setup_ui(_commands: Commands) {}

// ─── Dynamic Difficulty Adjustment (unchanged from v1) ────────────────────

#[derive(Resource, Default)]
pub struct Difficulty {
    pub level: f32, // 0.0 to 1.0
}

pub fn dda_system(
    mut difficulty: ResMut<Difficulty>,
    player_query: Query<&Health, With<Player>>,
    boss_query: Query<&Health, With<Boss>>,
) {
    let Ok(player_health) = player_query.get_single().map(|h| h.current) else {
        return;
    };
    let Ok(boss_health) = boss_query.get_single().map(|h| h.current) else {
        return;
    };
    if player_health > boss_health + 20.0 {
        difficulty.level = (difficulty.level + 0.01).min(1.0);
    } else if boss_health > player_health + 20.0 {
        difficulty.level = (difficulty.level - 0.01).max(0.0);
    }
}

pub fn apply_difficulty(
    difficulty: Res<Difficulty>,
    mut boss_query: Query<&mut Hitbox, With<Boss>>,
) {
    for mut hitbox in &mut boss_query {
        hitbox.damage = 10.0 + (difficulty.level * 10.0);
    }
}

