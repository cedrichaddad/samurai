import gymnasium as gym
from stable_baselines3 import PPO
from stable_baselines3.common.env_util import make_vec_env
from stable_baselines3.common.vec_env import VecFrameStack
from stable_baselines3.common.callbacks import CheckpointCallback
import torch
import torch.nn as nn
from env import SamuraiEnv

def train():
    # Create environment
    # Vectorized environment for faster training
    env = make_vec_env(SamuraiEnv, n_envs=8)
    env = VecFrameStack(env, n_stack=4) # 4 frames of history

    # Hyperparameters from spec
    # Policy: Feedforward with 2 hidden layers of 128 neurons
    # Gamma: 0.99
    # GAE Lambda: 0.95
    # Learning Rate: 3e-4
    # Clip Range: 0.2
    # Batch Size: 64 (minibatch)
    # n_steps: 256 (per env) -> 2048 total buffer size
    # Ent coef: 0.01
    
    policy_kwargs = dict(
        activation_fn=nn.ReLU,
        net_arch=dict(pi=[128, 128], vf=[128, 128])
    )

    model = PPO(
        "MlpPolicy",
        env,
        learning_rate=3e-4,
        n_steps=256,
        batch_size=64,
        n_epochs=10,
        gamma=0.99,
        gae_lambda=0.95,
        clip_range=0.2,
        ent_coef=0.01,
        policy_kwargs=policy_kwargs,
        verbose=1,
        device="cpu" # Force CPU for now to ensure compatibility
    )

    # Train
    print("Starting training...")
    # Train for a short duration for testing purposes (e.g. 100k steps)
    # In real scenario, would be millions.
    model.learn(total_timesteps=100000)
    
    print("Training finished.")
    
    # Save SB3 model
    model.save("ppo_samurai")
    
    # Export to TorchScript
    # We need to trace the policy network
    print("Exporting to TorchScript...")
    
    class PolicyNetwork(nn.Module):
        def __init__(self, policy):
            super().__init__()
            self.policy = policy
            
        def forward(self, obs):
            # SB3 policy forward returns (actions, values, log_probs)
            # We just want actions for inference
            # obs needs to be tensor
            return self.policy.predict(obs, deterministic=True)[0]

    # Actually, better to trace the underlying torch module directly if possible.
    # SB3's predict method does some preprocessing.
    # Let's try to trace the actor network directly.
    
    # The actor network takes observations and outputs action distribution.
    # But for deterministic inference we just want the argmax or sample.
    
    # Let's wrap it simply:
    # We want a model that takes [1, 8] float tensor and outputs [1] int tensor (action)
    
    # However, SB3's predict is convenient.
    # But we can't trace the whole SB3 object.
    # We can trace the policy.
    
    cpu_model = model.policy.to("cpu")
    
    # Create dummy input
    # 13 features * 4 frames = 52
    dummy_input = torch.zeros(1, 52)
    
    # We can trace the `get_distribution` -> `mode` path?
    # Or just use the `predict` wrapper logic.
    
    # Let's define a wrapper module that does exactly what we want in Rust:
    # Input: Observation Tensor
    # Output: Action Tensor
    
    class ExportModel(nn.Module):
        def __init__(self, policy):
            super().__init__()
            self.actor = policy.mlp_extractor.policy_net
            self.action_net = policy.action_net
            self.value_net = policy.mlp_extractor.value_net
            self.value_head = policy.value_net
            
        def forward(self, obs):
            # Forward pass through actor
            features = self.actor(obs)
            action_logits = self.action_net(features)
            
            # Deterministic action: argmax
            action = torch.argmax(action_logits, dim=1)
            return action

    export_model = ExportModel(cpu_model)
    export_model.eval()
    
    # Trace
    traced_script_module = torch.jit.trace(export_model, dummy_input)
    traced_script_module.save("samurai_model.pt")
    
    print("Model exported to samurai_model.pt")

if __name__ == "__main__":
    train()
