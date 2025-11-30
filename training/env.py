import gymnasium as gym
from gymnasium import spaces
import numpy as np
import math

# Constants (MUST MATCH RUST src/game/combat.rs)
FPS = 60
MAX_STEPS = 60 * 60 # 60 seconds
ARENA_SIZE = 10.0
ATTACK_DURATION = 0.5
PARRY_DURATION = 0.2
STUN_DURATION = 0.3
DODGE_DURATION = 0.3

# Actions
ACTION_WAIT = 0
ACTION_MOVE_FORWARD = 1
ACTION_MOVE_BACKWARD = 2
ACTION_STRAFE_LEFT = 3
ACTION_STRAFE_RIGHT = 4
ACTION_ATTACK = 5
ACTION_PARRY = 6
ACTION_DODGE = 7

# States
STATE_IDLE = 0
STATE_MOVE = 1
STATE_ATTACK = 2
STATE_PARRY = 3
STATE_DODGE = 4
STATE_STUNNED = 5

class SamuraiEnv(gym.Env):
    def __init__(self):
        super(SamuraiEnv, self).__init__()
        
        # Action space: 8 discrete actions
        self.action_space = spaces.Discrete(8)
        
        # Observation space:
        # 0: Distance to opponent
        # 1: Cos(Angle)
        # 2: Sin(Angle)
        # 3: Rel Vel X
        # 4: Rel Vel Z
        # 5: Self Health (0-1)
        # 6: Opponent Health (0-1)
        # 7: Self State
        # 8: Opponent State
        # 9: Self Action Timer
        # 10: Opponent Action Timer
        # 11: Previous Action
        # 12: Boss Attack Cooldown
        # Total: 13 floats
        self.observation_space = spaces.Box(low=-np.inf, high=np.inf, shape=(13,), dtype=np.float32)
        
        self.reset()
        
    def reset(self, seed=None, options=None):
        super().reset(seed=seed)
        
        # Init positions (2D for simplicity in logic, representing X/Z plane)
        self.boss_pos = np.array([2.0, 0.0])
        self.player_pos = np.array([-2.0, 0.0])
        self.boss_prev_pos = self.boss_pos.copy()
        self.player_prev_pos = self.player_pos.copy()
        
        self.boss_health = 100.0
        self.player_health = 100.0
        
        self.boss_state = STATE_IDLE
        self.player_state = STATE_IDLE
        
        self.boss_timer = 0.0
        self.player_timer = 0.0
        self.boss_last_action = ACTION_WAIT
        self.boss_attack_cooldown = 0.0
        
        # Randomize opponent type
        # 0: Aggressive (Rush + Attack)
        # 1: Defensive (Wait + Parry)
        # 2: Random
        # 3: Evader (Backdash + Wait)
        self.opponent_type = np.random.randint(0, 4)
        
        self.steps = 0
        self.steps = 0
        self.boss_consecutive_dodges = 0
        
        # Combat State Flags (One-shot logic)
        self.boss_attack_resolved = False
        self.player_attack_resolved = False
        
        return self._get_obs(), {}
        
    def _get_obs(self):
        dist = np.linalg.norm(self.boss_pos - self.player_pos)
        
        # Velocity
        boss_vel = (self.boss_pos - self.boss_prev_pos) * FPS
        player_vel = (self.player_pos - self.player_prev_pos) * FPS
        rel_vel = player_vel - boss_vel
        
        # Angle (Assuming auto-facing for now, so angle is 0)
        # In a real 3D env, we'd calculate dot product of forward vec and dir to player
        cos_angle = 1.0
        sin_angle = 0.0
        
        obs = np.array([
            dist / ARENA_SIZE,
            cos_angle,
            sin_angle,
            rel_vel[0] / 10.0, # Normalize roughly
            rel_vel[1] / 10.0,
            self.boss_health / 100.0,
            self.player_health / 100.0,
            float(self.boss_state) / 5.0,
            float(self.player_state) / 5.0,
            self.boss_timer,
            self.player_timer,
            float(self.boss_last_action) / 7.0,
            self.boss_attack_cooldown # <--- NEW FEATURE
        ], dtype=np.float32)
        return obs
        
    def step(self, action):
        self.steps += 1
        
        # Capture prev pos for velocity calc
        self.boss_prev_pos = self.boss_pos.copy()
        self.player_prev_pos = self.player_pos.copy()
        
        # Decrement Cooldown
        if self.boss_attack_cooldown > 0:
            self.boss_attack_cooldown -= 1.0 / FPS
        
        # 1. Apply Boss Action
        # Fix: Force Wait if locked in animation (match Rust behavior)
        if self.boss_timer > 0:
            action = ACTION_WAIT
            
        self._apply_action(action, is_boss=True)
        
        # 2. Simulate Player (Synthetic Opponent)
        # Simple logic: If close, attack. If attacked, parry/dodge. Else move closer.
        player_action = self._get_scripted_player_action()
        self._apply_action(player_action, is_boss=False)
        
        # 3. Update Physics & Timers
        self._update_physics(is_boss=True)
        self._update_physics(is_boss=False)
        
        # 4. Resolve Collisions
        reward = self._resolve_combat()
        
        # Time penalty (encourage speed)
        reward -= 0.01

        # 5. Check Done
        terminated = False
        truncated = False
        
        if self.boss_health <= 0:
            reward -= 100.0
            terminated = True
        elif self.player_health <= 0:
            reward += 100.0
            terminated = True
            
        if self.steps >= MAX_STEPS:
            truncated = True
            # Truncation penalty if fight not finished
            if not terminated:
                reward -= 20.0
            
        return self._get_obs(), reward, terminated, truncated, {}
        
    def _apply_action(self, action, is_boss):
        state = self.boss_state if is_boss else self.player_state
        timer = self.boss_timer if is_boss else self.player_timer
        
        if state != STATE_IDLE and state != STATE_MOVE:
            # Locked in animation
            return
            
        # State transition
        new_state = STATE_IDLE
        new_timer = 0.0
        
        if action == ACTION_ATTACK:
            # Check Cooldown
            if is_boss and self.boss_attack_cooldown > 0:
                # Forced Wait (Tired)
                # reward -= 0.1 # Optional penalty?
                pass
            else:
                new_state = STATE_ATTACK
                new_timer = ATTACK_DURATION # seconds
                if is_boss: 
                    self.boss_attack_resolved = False
                    self.boss_attack_cooldown = ATTACK_DURATION + 0.5 # 0.5s recovery
                else: self.player_attack_resolved = False
        elif action == ACTION_PARRY:
            new_state = STATE_PARRY
            new_timer = PARRY_DURATION
        elif action == ACTION_DODGE:
            new_state = STATE_DODGE
            new_timer = DODGE_DURATION
        elif action in [ACTION_MOVE_FORWARD, ACTION_MOVE_BACKWARD, ACTION_STRAFE_LEFT, ACTION_STRAFE_RIGHT]:
            new_state = STATE_MOVE
            # Movement is applied in update_physics
            
        if is_boss:
            if action == ACTION_DODGE:
                self.boss_consecutive_dodges += 1
            else:
                self.boss_consecutive_dodges = 0
                
            self.boss_state = new_state
            self.boss_timer = new_timer
            # self.boss_last_action = action # Store for movement -> Moved to step start or kept?
            # Actually we need it for next frame's observation, so keep it.
            self.boss_last_action = action 
        else:
            self.player_state = new_state
            self.player_timer = new_timer
            self.player_last_action = action

    def _update_physics(self, is_boss):
        # Update timer
        timer = self.boss_timer if is_boss else self.player_timer
        state = self.boss_state if is_boss else self.player_state
        
        if timer > 0:
            timer -= 1.0 / FPS
            if timer <= 0:
                timer = 0.0
                state = STATE_IDLE
        
        if is_boss:
            self.boss_timer = timer
            self.boss_state = state
        else:
            self.player_timer = timer
            self.player_state = state
            
        # Decrement Cooldown
        if self.boss_attack_cooldown > 0:
            self.boss_attack_cooldown -= 1.0 / FPS
            
        # 1. Collision Resolution (Invisible Wall)
        dist_vec = self.player_pos - self.boss_pos
        dist = np.linalg.norm(dist_vec)
        min_dist = 1.6 # 0.8 radius * 2
        
        if dist < min_dist and dist > 0.001:
            overlap = min_dist - dist
            push = (dist_vec / dist) * (overlap / 2.0)
            # Push apart
            self.player_pos += push
            self.boss_pos -= push
            
        # MOVEMENT LOGIC
        pos = self.boss_pos if is_boss else self.player_pos
        target = self.player_pos if is_boss else self.boss_pos
        action = self.boss_last_action if is_boss else self.player_last_action
        
        # 1. Standard Movement (Idle/Move)
        if state == STATE_MOVE or state == STATE_IDLE:
            if state == STATE_MOVE:
                speed = 5.0
                if action == ACTION_MOVE_FORWARD:
                    direction = target - pos
                    dist = np.linalg.norm(direction)
                    if dist > 0:
                        direction /= dist
                        pos += direction * speed / FPS
                elif action == ACTION_MOVE_BACKWARD:
                    direction = pos - target
                    dist = np.linalg.norm(direction)
                    if dist > 0:
                        direction /= dist
                        pos += direction * speed / FPS
                elif action == ACTION_STRAFE_LEFT:
                    # Simplified strafe
                    pass
                elif action == ACTION_STRAFE_RIGHT:
                    pass
        
        # 2. Lunge Mechanics (Attack Root Motion)
        elif state == STATE_ATTACK and timer > (ATTACK_DURATION * 0.3):
             dist = np.linalg.norm(target - pos)
             
             # ONLY Lunge if gap exists (> 1.2)
             if dist > 1.2:
                 direction = target - pos
                 if np.linalg.norm(direction) > 0:
                     direction /= np.linalg.norm(direction)
                 pos += direction * (2.0 / FPS)

        # Boundary check
        if pos[0] < -ARENA_SIZE/2: pos[0] = -ARENA_SIZE/2
        if pos[0] > ARENA_SIZE/2: pos[0] = ARENA_SIZE/2
        if pos[1] < -ARENA_SIZE/2: pos[1] = -ARENA_SIZE/2
        if pos[1] > ARENA_SIZE/2: pos[1] = ARENA_SIZE/2
             
        if is_boss: self.boss_pos = pos
        else: self.player_pos = pos

    def _get_scripted_player_action(self):
        dist = np.linalg.norm(self.boss_pos - self.player_pos)
        
        if self.opponent_type == 0: # Aggressive
            if dist < 1.5:
                # Nerf: Reaction delay / Randomness
                # Only attack 10% of the time per frame when in range
                if np.random.rand() < 0.1:
                    return ACTION_ATTACK
                else:
                    return ACTION_WAIT
            else:
                return ACTION_MOVE_FORWARD
        elif self.opponent_type == 1: # Defensive
            if dist < 1.5:
                # If boss attacking, parry?
                if self.boss_state == STATE_ATTACK:
                    # Nerf: Only parry 30% of the time, otherwise block/wait or take hit
                    if np.random.rand() < 0.3:
                        return ACTION_PARRY
                    else:
                        return ACTION_WAIT
                else:
                    # Occasional attack
                    if np.random.rand() < 0.05: return ACTION_ATTACK
                    return ACTION_WAIT
            else:
                # Maintain distance?
                return ACTION_WAIT
        elif self.opponent_type == 3: # Evader
             if dist < 2.5:
                 return ACTION_MOVE_BACKWARD
             else:
                 return ACTION_WAIT
        else: # Random
            return self.action_space.sample()

    def _resolve_combat(self):
        reward = 0.0
        
        # Check Boss Hit Player
        # Check Boss Hit Player
        if self.boss_state == STATE_ATTACK and self.boss_timer > 0.2: # Active frames
             if not self.boss_attack_resolved:
                 dist = np.linalg.norm(self.boss_pos - self.player_pos)
                 if dist < 2.0:
                     if self.player_state == STATE_PARRY:
                         reward += 6.0 # Successful parry (Utility)
                         self.boss_attack_resolved = True
                     elif self.player_state == STATE_DODGE:
                         pass # Miss
                     else:
                         # Hit
                         self.player_health -= 10.0 # Big chunk damage
                         reward += 12.0 # Attack Reward (Primary Goal)
                         self.boss_attack_resolved = True
                     
        # Check Player Hit Boss
        # Check Player Hit Boss
        if self.player_state == STATE_ATTACK and self.player_timer > 0.2:
             if not self.player_attack_resolved:
                 dist = np.linalg.norm(self.boss_pos - self.player_pos)
                 if dist < 2.0:
                     if self.boss_state == STATE_PARRY:
                         reward += 10.0 # Successful parry (One-shot) - Bumped to 10
                         self.player_attack_resolved = True
                     elif self.boss_state == STATE_DODGE:
                         pass
                     else:
                         self.boss_health -= 10.0
                         reward -= 30.0 # INCREASED PENALTY (Was -15.0)
                         self.player_attack_resolved = True
                     
        # Penalties for whiffing/spamming
        # If boss is attacking but not hitting anything
        # Penalties for whiffing/spamming
        # If boss is attacking but not hitting anything
        if self.boss_state == STATE_ATTACK and self.boss_timer > 0.2:
             if not self.boss_attack_resolved:
                 dist = np.linalg.norm(self.boss_pos - self.player_pos)
                 if dist >= 1.5: # Range (Synced with Rust)
                     # reward -= 0.1 # Whiff penalty REMOVED for now
                     pass
                     # Actually, let's make it small per frame so it adds up if you whiff a lot.

        # If boss is parrying but player is not attacking
        if self.boss_state == STATE_PARRY:
             if self.player_state != STATE_ATTACK:
                 reward -= 0.1 # Reduced parry spam penalty
             # ELSE: DO NOTHING. Reward is handled in collision logic above.

        # Dodge Logic
        if self.boss_state == STATE_DODGE:
            # Base cost
            reward -= 0.05
            # Consecutive penalty
            reward -= 0.02 * self.boss_consecutive_dodges
            
            # Reward successful dodge
            # If player is attacking and close, and we are dodging
            if self.player_state == STATE_ATTACK and self.player_timer > 0.2:
                dist = np.linalg.norm(self.boss_pos - self.player_pos)
                if dist < 2.0:
                    reward += 0.5 # Successful dodge! (Per frame, so adds up)

        # Engagement Reward (Aggressive)
        dist = np.linalg.norm(self.boss_pos - self.player_pos)
        if dist < 2.0:
            reward += 0.05 * (2.0 - dist) # Increased proximity reward
                 
        return reward

if __name__ == "__main__":
    env = SamuraiEnv()
    obs, _ = env.reset()
    print("Obs:", obs)
    for _ in range(10):
        action = env.action_space.sample()
        obs, reward, term, trunc, _ = env.step(action)
        print(f"Step: A={action} R={reward} HP={env.boss_health}")
