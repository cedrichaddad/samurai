Adaptive Samurai Boss – Design and Architecture 

Overview and Concept 

This project is a 1v1 samurai dueling game featuring an AI-controlled boss that learns and adapts to the player’s behavior over time. The core idea is to replicate a “boss is learning me” experience – the longer and more repetitively you fight the boss, the smarter and more counter-adaptive it becomes. The gameplay is inspired by titles like Sekiro, emphasizing fluid, responsive combat at a high frame-rate (60+ FPS) to achieve tight, satisfying controls. The boss’s AI is powered by a combination of reinforcement learning (RL) and traditional game AI techniques, optimized for both performance and adaptivity. We outline below the full end-to-end design: from gameplay mechanics and system architecture to the machine learning pipeline (with tuned hyperparameters) that produces the boss’s behavior. 

Core Gameplay Mechanics 

The game’s combat system is simple but deep, focusing on timing and player skill. Both the player and the boss share a basic moveset to keep the duel fair and skill-based: 

Light Attack: A quick melee strike with a samurai sword. It deals moderate damage and can be chained in short combos. Light attacks are telegraphed but fast. 

Parry/Block: A defensive move that, if timed correctly just as an attack lands, will parry the incoming strike. A successful parry negates damage and briefly staggers the attacker, creating an opening. (If timing is off, the move acts as a block, reducing damage but not negating it.) 

Dodge: A quick evasive maneuver (sidestep or roll) to avoid attacks. Dodging has invulnerability frames if timed well, allowing the player or boss to evade a strike completely. 

Movement: Free movement around the arena (either 2D plane or 3D space depending on implementation). In a 3D scenario, the camera is locked onto the opponent (Z-targeting style) for intuitive circle-strafing and distance control. In 2D, movement would be left/right (and possibly jump), but we favor a 3D arena for impressiveness. Movement speed is balanced so that neither character can simply run away indefinitely without consequences. 

Combat Design Notes: Both fighters have a health bar. There may be stamina or cooldown considerations to prevent spamming (e.g. successive dodges could incur a brief invulnerability cooldown). Attacks and parries have well-defined animation durations and recovery times, crucial for the Sekiro-like fluidity – e.g. parry animations are very quick to allow instantaneous counterattacks if successful. Hitboxes and hurtboxes are implemented for accurate collision detection on attacks (sword swings, bodies). We ensure the game updates at a fixed 60 Hz tick for physics and input, so timing and reactions are consistent. High responsiveness (minimal input lag) and animation cancel rules (e.g. allow cancelling an attack into a parry or dodge at specific frames) are tuned to make the controls snappy and combat reactive like modern action games. 

Engine and Technical Architecture 

Language & Engine: Given the developer’s strength in Rust, we choose to implement the game using Rust for reliability and performance. To minimize headaches and leverage existing capabilities, we plan to use a Rust game engine or framework. A strong candidate is Bevy (Rust ECS engine), which offers a flexible Entity-Component-System architecture and can handle 2D or 3D rendering, input, and physics. Bevy allows writing game logic in pure Rust and takes advantage of Rust’s fearless concurrency for parallelizing systems (useful for AI, physics, rendering on separate threads). An alternative is using Godot 4 with Rust (GDNative) or Unreal Engine via C++, but those would require mixing languages or learning new APIs. Sticking to Rust/Bevy provides a consistent development experience. 

Game Loop & Systems: The game loop will follow a typical structure: - Fixed Update (60 FPS): Handle input, AI decisions, physics, and game logic in discrete ticks. Using a fixed timestep ensures consistent behavior (important for timing-sensitive parries). - Render Update: Decoupled rendering at as high FPS as possible, interpolating between physics updates if needed for smooth visuals. This separation keeps simulation logic deterministic and avoids tying AI to fluctuating frame rates. 

We break the game logic into systems: - Input System (Player): Reads keyboard/controller input, updates the player character’s intended actions (move, attack, etc.), subject to cooldowns or animation locks. - AI System (Boss): Runs the boss decision-making logic. This is where our ML model (or behavior tree) will output the boss’s next action given the current state. We ensure this system runs within the fixed tick budget – the model inference is optimized to be fast (more on this below). - Physics & Collision System: Moves characters according to inputs, checks collisions for attacks (sword hitboxes vs hurtboxes), resolves hits (apply damage, play hit reactions). We can leverage Bevy’s 2D/3D physics or implement a lightweight custom collision for simplicity (e.g., distance checks for sword swings if using simple geometry). - Animation System: Plays the appropriate animations on characters (blending between idle, run, attack, dodge, etc.). Animations are tied to the state of the character (for example, when an attack action is decided, trigger the attack animation and mark the character as “in attack animation” for a certain number of frames during which other actions are limited). - Audio System: (If included) plays sound effects for slashes, parries (with a nice clang sound), and maybe a dynamic music track that intensifies as the fight progresses. While not core to AI, good audio-visual feedback enhances the perceived intelligence and weight of the boss’s actions. 

Performance Considerations: Rust’s efficiency ensures minimal overhead in the game loop. We use ECS parallelism to update different systems concurrently (e.g., physics and some aspects of AI can update in parallel if no data conflicts). The boss AI inference will be done via a optimized native call (no Python at runtime). We will use a TorchScript or ONNX model loaded in Rust so that the neural network forward pass runs in C++/Rust with no GIL overhead. For example, using the tch-rs library (Rust bindings for libtorch) we can load a traced PyTorch model and run inference in Rust efficiently[1]. This approach has been demonstrated by loading a TorchScript CModule in Rust and applying it to inputs in real-time[1]. The model is kept lightweight (small number of parameters) so that a forward pass takes only a fraction of a millisecond on a CPU – ensuring the AI decision doesn’t become a bottleneck in a 16ms frame budget. We will also structure the AI system such that if needed, it can run asynchronously (e.g., predict the next action slightly ahead of time) or at a lower tick rate than rendering. However, for a truly fluid experience, the boss will likely decide and react at the full tick rate (60 Hz), especially for frame-sensitive actions like parries. 

Scalability: While initially a single-player local game, the architecture keeps the door open for a PC release or even online features. All game logic is deterministic and isolated, which would facilitate net-code (lockstep or delay-based rollback if ever needed for multiplayer). Also, using cross-platform libraries (Rust, Bevy, Vulkan/Metal for rendering) means we can compile to Windows, Linux, etc., easily. The design avoids any hard ties to mobile or web, focusing on desktop-class hardware. 

Boss AI Design – Hybrid ML and Behavior Logic 

To maximize the adaptive, learning feel of the boss, we employ a hybrid AI approach: a deep reinforcement learning model drives the boss’s moment-to-moment decisions (the “brain”), while a minimal behavior logic or state machine provides high-level structure and ensures no truly unsportsmanlike behavior. This is inspired by recent trends where game AI moves from complex scripted behavior trees to ML-driven policies[2]. Traditionally, developers would hand-craft behavior trees with numerous branches and condition checks for all scenarios. Here, we replace much of that with a neural network policy that observes the game state and chooses actions directly[2]. The environment query (positions, states, etc.) serves as the AI’s “eyes and ears,” and the learned model outputs the “brain’s” decision[2]. This drastically simplifies the AI logic and allows the boss to exhibit nuanced, non-repetitive tactics that emerge from learning rather than explicit scripting. 

State Representation (Observations): We design the input to the neural network carefully so that it has all relevant information: - The relative positions of the player and boss (e.g. distance between them, angle between facing directions in 3D, or a simpler 1D distance in a 2D fight). We normalize distances (e.g., divide by some maximum range) so inputs are in a consistent scale. - Velocities or movement direction of each (so the AI knows if the player is rushing in or retreating). - Current action states: e.g., a boolean or flag indicating if the player is currently in an attack animation, in a dodge, or neutral. This is critical for the boss to time counter-moves; for instance, if the player is mid-attack, a well-trained policy might choose to dodge or parry. - Health values of both sides (normalized 0-1). The boss could act differently when low on health (for example, become more desperate or defensive). - Last move used by each side: We can feed a one-hot or small encoding of what the player did last (or in the last few ticks). This gives the network a short-term memory of immediate past actions, useful for reacting (e.g., if the player just attempted a heavy attack, the best response might differ than if they just dodged). - Optionally, historical aggregates: To facilitate the boss adapting to patterns, we may include features like “frequency of player’s attacks vs defense in the last N seconds” or a moving average of the player’s aggressiveness. Another technique is to use an LSTM layer in the network so it can build an internal memory of the sequence of actions, rather than feeding handcrafted history features. A recurrent policy (PPO-LSTM) can allow the boss to pick up on temporal patterns (like repeated double-attack then dodge patterns from the player). 

All these observation features are combined into a vector (for an MLP input) or multiple tensors (for a multi-input network). We scale/normalize inputs appropriately (zero-mean, unit-variance where applicable) to speed up training convergence. 

Action Space: The boss’s action output is discretized for simplicity and reliability. We define a set of discrete action choices the boss can take on each decision tick: - Attack moves: e.g., “light attack toward player”. (Since movement and facing are continuous in 3D, we assume the boss auto-orients toward the player for attacks if in range. For out-of-range, it might move in first.) - Defensive moves: “Parry now” or “Dodge” (the dodge direction could be incorporated – e.g., dodge left, dodge right, backward – as separate actions for variety). - Movement decisions: e.g., “approach player”, “back away”, or strafe to a side. We can include these as actions which result in a brief movement in that direction for a few frames. - Do nothing / wait: sometimes the optimal move is to briefly pause (e.g., to bait the player). So a “no-op” action is included. 

We can model the action space as a single categorical output over all meaningful combinations. However, to keep it manageable, we might factor it into two outputs (which the network can handle as multi-discrete outputs[3][4]): one for attack/defense (none, light attack, parry, dodge) and one for movement (none, move toward, move away, strafe left, strafe right). This factorization allows the boss to, say, choose to move and attack at the same time if desired. In practice, we will likely restrict some combinations to avoid impossible inputs (for example, if the boss chooses to light attack and move forward simultaneously, that could be interpreted as a running attack or simply an attack that also causes forward motion). The game logic will handle the details (e.g., triggering an attack animation inherently moves the boss forward a bit as part of the animation). 

Behavior Tree / High-Level Logic: While the RL policy will handle most decisions, we incorporate a light behavior layer for structure: - Phases: It’s common for bosses to have phases (e.g., 100-50% health = Phase 1, 50-0% = Phase 2) where they alter tactics. We can implement a simple state machine: e.g., below 50% health, the boss enters an “Enraged” phase. In this phase, we might increase its aggression or unlock a new combo. We can either train the RL model to also handle this (by including “phase” or health as an input and expecting it to naturally change behavior), and/or run a separate policy or modified parameters in phase 2. A straightforward approach is to have phase as part of observation, so one neural network handles both, but the change in health triggers it to behave differently (which it can learn). - Scripted specials: If we want cinematic or unique moves (like a big area attack or a throw after some time), a BT node could trigger those under certain conditions (like every 30 seconds or when the player has been turtling too long). These could override the RL policy briefly. However, to avoid interfering with the learned behavior too much, such moves should be used sparingly. For the scope of this project, we might skip complex special moves to focus on the core learned behavior. - Fallbacks and Constraints: A BT can also enforce constraints to avoid known failure modes of pure RL. For example, if the boss’s policy ever outputs an obviously bad action (like trying to attack when the player is clearly out of range), a guard condition could override that (though ideally the policy is trained not to do that). We can also enforce cooldowns: e.g., don’t allow the boss to spam parry constantly. This can be done via a simple timer check in the BT or in the environment logic (action masking). In training, we will apply action masking to forbid illegal actions (like parrying when no attack is incoming, or dodging when not needed) so that the policy learns valid behavior faster[5][6]. For instance, one might mask the “parry” action unless the opponent is in the middle of an attack swing, etc., to replicate how a human would only parry reactively. This prevents the agent from wasting actions and focuses learning on meaningful choices. 

In summary, the boss AI is largely an RL-driven agent, but we frame it within light behavior constraints to ensure it remains fun and fair. The combination of ML for nuanced tactics and a touch of scripting for game-design constraints is a powerful approach that has been advocated in recent research and demos[2][7]. It maximizes the boss’s apparent intelligence while keeping it behaving within the bounds of enjoyable gameplay. 

Training the Boss AI (Offline RL Pipeline) 

To create the boss’s learning brain, we will use offline deep reinforcement learning training (i.e., in a simulated environment prior to releasing the game). We choose Proximal Policy Optimization (PPO) as our RL algorithm, as it is a state-of-the-art policy gradient method known for stable performance in continuous control and game domains[8]. PPO has been proven effective in complex environments (including fighting games) and is comparatively straightforward to tune[8]. We will leverage an existing implementation (such as Stable-Baselines3 PPO) to speed up development, but ensure we configure it with the best hyperparameters for our scenario. 

Environment for Training: We build a simplified simulation of the duel in Python (for integration with PyTorch). This environment replicates the core mechanics: movement, attack collisions, health, etc. We won’t render graphics here; it’s purely logic. The environment will take a boss action (from the RL agent) and a player action (from a scripted or other agent) at each time step, update the state (positions, health, etc.), and return observations and rewards. Each episode is one duel, which ends when either side’s health reaches zero or a time limit (to prevent infinite stalemates). 

Synthetic Opponents: To train the boss effectively, it needs diverse and challenging sparring partners. Relying on random or a single scripted opponent could lead to the boss overfitting or learning to exploit specific quirks. Instead, we employ a variety of synthetic player agents: - We create a handful of scripted AI fighters with distinct styles: - Aggressive Attacker: this bot constantly moves towards the boss and uses a lot of light attacks, seldom defending. - Turtle/Defensive: this bot primarily blocks/parries and only occasionally counter-attacks after a successful block. - Evader: this bot dodges frequently and tries to bait the boss, only attacking opportunistically. - Random: a baseline that picks random (but valid) actions, to introduce some stochastic behavior. - During training, we randomly pit the boss against one of these opponents each episode. This ensures the boss encounters a heterogeneous set of strategies, forcing it to generalize. This approach is akin to a mini “league training” concept used in AlphaStar: training with a mixture of opponents to broaden the policy’s skills[9]. By playing against past versions and diverse policies, RL agents avoid over-specializing[9]. Our version is simpler but follows the spirit – a randomized opponent each episode (possibly with equal probability or a curriculum). - We can gradually increase the difficulty of these opponents as training progresses (a form of curriculum learning). For instance, start with the random and a mild attacker, and once the boss learns to beat those consistently, introduce the stronger defensive bot which requires more complex tactics (parry break, feints, etc.). 

Reward Design: We craft the reward function to guide the boss towards intelligent, player-like combat behavior: - The primary reward is winning the duel (e.g., +100 reward for reducing player’s health to 0, and a symmetric large negative for the boss’s own defeat). This focuses the agent on accomplishing the goal. - Intermediate rewards can help it learn finer skills: - Small positive reward for damaging the player (e.g., +1 per point of damage dealt) and small negative for taking damage. This encourages the boss to both be aggressive and defensive appropriately. - A bonus for successful parry (because parrying perfectly is a high-skill move and makes the fight interesting). - A penalty for whiffing attacks (attacking and missing) or for wasting actions (like parrying at air) to discourage random flailing. - We incorporate a “fun” penalty to avoid degenerate tactics that would make the fight boring or unfair. For example, if the boss simply runs away too long or “camping” in a corner doing the same move, we issue a slight penalty over time. This aligns with avoiding strategies that, while optimal, ruin the player’s experience (akin to the orb-camping scenario described in a Pac-Man-like demo[10]). By tweaking rewards in this way, we align the agent’s behavior with human expectations of a fair fight[11]. - We ensure that the difficulty is not just maxed out; a perfect play might be optimal for winning but not fun. So, we might even give a small negative reward for each time the boss performs an exact repeat of a move sequence it did recently, nudging it toward variety. This encourages a more dynamic, less predictable boss. All these reward shaping choices are done carefully to not confuse the agent, but to bias it towards human-like, entertaining behavior[11]. The concept of using “meticulously designed rewards” to align AI behavior with human-like play has been successfully used in prior work[11]. 

RL Algorithm and Hyperparameters: We use PPO with the following parameters (chosen based on both default proven settings and domain-specific tuning): - Policy network architecture: a feedforward neural network with two hidden layers of 128 neurons each (ReLU activations), followed by an LSTM layer with 128 hidden units (if we use recurrence). The output of the LSTM goes into two heads: one for the action probability distribution (multinomial over discrete actions) and one for state-value estimation (for critic). PPO will share the lower layers between actor and critic to expedite learning. This architecture is small enough for real-time inference, but large enough to capture the complexity of combat tactics. - Discount factor (
𝛾
𝛾
 
): 0.99. This value balances immediate rewards vs long-term planning. A slightly higher value (0.995) was used in a commercial fighting AI study[12] to account for longer episodes, but 0.99 is a standard starting point that works well for episodic games and ensures the agent focuses on winning the current fight[13]. - GAE lambda: 0.95 for Generalized Advantage Estimation, which is standard and helps reduce variance[13]. This combination of 
𝛾=0.99,𝜆=0.95
𝛾
=
0
.
99
,
𝜆
=
0
.
95
 
 is known to yield good bias-variance tradeoff in advantage estimates[13]. - Learning Rate: ~3e-4 (0.0003) with an Adam optimizer[13]. This is a common default for PPO; we may use a learning rate schedule (e.g., linear decay) to stabilize later training. The Naruto Mobile project found 2e-4 effective in their large-scale setup[14], and we are in the same order of magnitude. - PPO clipping 
𝜖
𝜖
 
: 0.2 (20% clip range)[13], as per the original PPO paper and stable-baselines defaults. This prevents too-large policy updates. - Batch size / rollout length: We will run with a fairly large number of environment steps per update to ensure good learning batches. For example, 2048 time steps per update (across all parallel environments) with minibatch size 64 and 10 optimization epochs, which are stable-baselines3 defaults[13]. We can adjust these depending on performance (Naruto’s project used very large scale parallelism, but on a single machine we might use ~8 environments × 256 steps each = 2048). - Entropy coefficient: A small value (e.g., 0.01 initially, decaying to ~0) to encourage exploration early on. Actually, stable-baselines default ent_coef is 0.0 (no entropy bonus)[13], but adding a tiny entropy bonus can help the agent try varied moves. We will monitor this; one experienced RL developer noted that reducing the entropy coefficient helped stabilize training for game-playing agents[15]. Thus, we might start with 0.01 and then reduce to 0 as training progresses, allowing convergence to a deterministic strategy once sufficient exploration has happened[15]. - Advantage normalization: enabled (standard practice in PPO). - Gradient clipping: 0.5 (to prevent extreme updates)[13]. - Parallel training: We leverage multiple CPU cores by running several fight simulations in parallel (e.g., 8 or 16) to collect experience faster. PPO’s multi-worker sampling will thus utilize our machine fully[16]. In the Naruto Mobile AI paper, they scaled up to thousands of parallel actors for massive training[14] – our needs are smaller but the principle of parallelism holds. 

We will train until the boss demonstrates consistently strong performance against all synthetic opponents (e.g., >80% win rate across them) but not absolute perfection (to ensure it has some exploitable patterns for a skilled player to overcome). This training might require on the order of millions of timesteps. With the above setup, we expect training to be on the order of a few hours to a day on a single high-end GPU or multi-core CPU. If needed, we can speed up via vectorized environments and ensuring the environment code is efficient (possibly in Rust for simulation as well, though Python should suffice given the relatively simple logic). 

During training, we will also perform evaluation fights against some held-out behaviors to verify generalization. We could, for example, have a “mix-style” bot that randomly switches strategy mid-fight, to see if the boss can adapt on the fly. 

Adaptive Learning Behavior in Gameplay 

Once trained, the boss’s policy network will be embedded in the game. At runtime, the boss will make decisions based on the learned policy, which inherently reacts to player tendencies (because it was trained against various tactics). However, to maximize the feeling of adaptation, we incorporate additional runtime adaptation mechanisms: - Dynamic Difficulty Adjustment (DDA): The boss will gauge the player’s skill over multiple attempts. If the player loses repeatedly in a frustrating manner, the boss can be configured to ease off slightly (for example, by adding random delays to its attacks or choosing a sub-optimal action occasionally). Conversely, if the player is consistently defeating the boss, the boss can use the full strength of its policy (or even be allowed to perform more advanced combos that we otherwise limit). This ensures a close, exciting fight for a wide range of player skill levels, aligning with the principle that AI competitiveness should match the player’s ability for enjoyment[17]. Concretely, we might maintain an internal “difficulty level” that nudges the boss’s behavior: e.g., at lower difficulty the boss takes longer to adapt and might not use all its repertoire, at higher difficulty it exploits player mistakes immediately. We can implement this by sampling the action from the policy with different levels of randomness (at easy difficulty, sample with a higher temperature or epsilon-greedy noise, causing occasional errors; at hard, take the max-probability action consistently, making it razor-sharp). - On-the-fly Learning: For the scope of this project, we will not do heavy online learning (no neural network weight updates during gameplay, which could be unpredictable and costly). Instead, adaptation comes from pattern recognition. The boss can keep track of what the player is doing in the current fight (or across fights) and adjust. For example, if it notices the player uses a particular combo frequently (say the player always attacks twice then dodges), the boss can specifically adjust by anticipating the dodge (perhaps delaying its counterattack to catch the dodge recovery). We can hard-code detection of such patterns and have a few contingent behaviors: - Maintain counters for player actions (e.g., how many times the player has successfully parried the boss, how many times the player used dodge in the last X seconds). Feed these counters as part of the observation to the policy, so the neural network naturally alters its strategy if it sees high values (since we trained on varied opponents, it likely learned to handle both extremes). - Alternatively, have a simple meta-strategy switch: e.g., if player is very defensive (parry a lot), the boss might switch to a more grab or feint heavy approach if those existed. With our limited moves, the boss can respond to frequent parries by doing nothing occasionally (to bait the premature parry) then attacking. This could be encoded as a small BT override: “if player parried >3 times in last 20 seconds, 20% chance the boss will perform a feint (start an attack and cancel, which in our move set might be simulated by a very short forward shuffle without attacking) to draw out a parry.” We can implement feints by using an unused action in the space that means “threaten attack but don’t commit” – essentially a do-nothing with a brief lunge animation. - If the player spams dodge constantly, the boss might start using a slower, delayed attack to catch them as they come out of dodge (or simply wait out the dodge then attack immediately). Our RL training likely covered some of this (since one synthetic bot was an evader), but we can amplify it by a rule: e.g., “if player has dodged 3 times in quick succession, the boss intentionally holds its next attack for a moment.” - Learning Across Attempts: To mimic how a boss seems to “learn” after you die and retry, we can persist some memory between fights. For example, if in the last attempt the player heavily favored a certain move, the boss’s starting behavior in the next attempt can be adjusted. A simple implementation: maintain a profile of the player’s strategy (e.g., aggressive vs defensive ratio) over each fight. When a new fight begins, initialize some hidden state or parameters of the boss AI according to the last fight. Since our boss’s brain is an RL policy that doesn’t literally update weights at runtime, this could be as simple as starting the boss at a higher difficulty level if the player breezed through last time, or enabling a certain counter-tactic if the player used one move overwhelmingly before. To support real-time opponent modeling, we train a GRU-based player behavior model in parallel with the offline RL pipeline. As the boss policy is trained using PPO against a diverse pool of synthetic players—each with distinct styles such as aggressive rushdown, defensive turtling, or evasive baiting—the same simulation environment is instrumented to log full gameplay trajectories: sequences of player and boss actions, relative positions, game state variables, and timestamps. These sequences serve as the training data for the GRU. One GRU variant is trained as a next-action predictor, using supervised learning to model P(player_action_{t+1} | history_{t-N:t}), enabling tactical counter-prediction. Another variant may be used to generate a latent playstyle embedding (z_player) by exposing the final hidden state h_T after N steps. This embedding is later used by the in-game style selector to shift boss strategy based on player tendencies. Both models are trained offline with PyTorch and frozen at inference time. Because the GRU is trained using the same game dynamics and behavioral diversity as the RL policy, its internal representation generalizes well to new players and supports robust adaptation in real-time without requiring online backpropagation or retraining. This way, the player gets the sense “the boss remembers what I did last time”. 

All these adaptation techniques ensure that the “boss is learning” feeling is maximized. The player will notice that cheap tricks stop working after a while and that repeating the same maneuver triggers smarter counters from the boss. Under the hood, this is achieved through a combination of the trained policy’s broad competency and explicit adaptive logic for pattern countering. 

Importantly, we also guarantee that the boss isn’t impossible. Borrowing a lesson from the industry: a boss that learns shouldn’t become a superhuman, unbeatably optimal agent[18]. We ensure the boss still has exploitable moments (perhaps built-in weaknesses or just the fact that it behaves with some human-like delay and imperfection). For instance, even if the boss learns your favorite move, you can mix up your tactics to throw it off – making the game a satisfying battle of wits. 

Model Integration and Deployment 

After training, the best-performing boss policy model will be exported for use in the game. We will likely export it as a TorchScript .pt file via torch.jit.trace/script, or to an ONNX format. The Rust game will load this model at startup. Using the tch crate (bindings to PyTorch C++ API) is a convenient approach: it allows us to load the TorchScript model and run inference on tensors efficiently in Rust[1]. The model inference consists of feeding the current state (observation vector) into the neural network and obtaining an action distribution. The game’s AI System will then sample or pick the action and apply it to the boss character. 

We pay attention to performance optimizations: - The model can be run on CPU in a separate thread if needed. Given its small size, CPU is sufficient (and avoids needing a GPU on the user’s machine). We ensure to batch operations if possible – though in a live game, it’s one observation at a time, which is fine. - We can compile the model with half-precision (float16) if needed to speed up inference, but likely unnecessary for such a small net. - If using ONNX Runtime or a similar library, we could enable neural network execution on GPU or use inference optimizers. However, simplicity and portability lean towards just using libtorch which is well-optimized for CPU as well. - We will test the inference step to ensure it’s well under 1ms per tick. If it’s borderline, we could reduce the network complexity (e.g., remove LSTM or reduce hidden units) or run the AI at say 30 Hz instead of 60 and interpolate behavior (though that could harm responsiveness, so 60 Hz is the aim). 

The game build will bundle the model file, and because we use local native inference, there’s no external dependency at runtime (no Python needed on end-user side, just the linked C++ libtorch which can be packaged). This approach is robust for a desktop portfolio piece. 

Additional Development Notes 

Animation and Feel: To truly reach “Sekiro levels of fluidity”, the implementation will spend time on tuning animations and timings. For example, the parry window (the few frames during which a parry input will successfully parry an attack) should be generous enough to be usable but not too easy. We might set it to ~100-200ms based on testing. The boss’s RL brain might learn to exploit exact timing if it were superhuman, but we implicitly limit that by giving the boss a reaction delay in observations (e.g., the boss might not “see” the exact moment of player attack start, or we ensure the policy doesn’t output a perfect parry unless conditions are reasonable). This keeps the boss feeling fair. 

Testing and Iteration: We'll iteratively test fights against the boss to observe its behavior. If we find issues (e.g., the boss learned something weird like spinning in circles), we will adjust the training (either add a penalty for that behavior or include an opponent that punishes it). This iterative training and testing cycle continues until the boss’s behavior is satisfying. 

Senior Dev Practices: The project will be developed with clean code architecture: clear module separation (e.g., ai module for all ML-related code, gameplay for mechanics, assets for loading models/animations, etc.), thorough documentation, and possibly unit tests for critical logic (like ensuring the reward function works as intended, or the environment simulation matches the real game). We use version control (Git) from the start and track experiments for training (maybe using TensorBoard to ensure the reward curve is improving). 

Resume Highlights: The finished project demonstrates a modern approach to game AI: 

Deep RL-driven boss behavior (using PyTorch and PPO with carefully tuned hyperparameters). 

Integration of an RL model into a real-time Rust game engine, using tools like TorchScript (showing ability to bridge ML and systems programming). 

A hybrid AI system that combines ML with classical techniques (behavior trees, dynamic difficulty) – reflecting understanding of both domains. 

Emphasis on performance and smooth gameplay, showing engineering prowess in optimization and real-time systems. 

The boss adapts to players, which is a cutting-edge feature (even NVIDIA has showcased prototypes of AI bosses that learn[19][20]). Our project stands as a portfolio piece that not only implements this but does so in a clean, end-to-end manner. 

By following this design, we ensure the boss AI is robust, efficient, and convincingly adaptive, providing a challenging and memorable experience for the player. This document serves as a comprehensive blueprint, covering all necessary components to build the project from scratch with no naive shortcuts, resulting in a polished implementation of a learning samurai boss duel. 

Sources: 

Eidos-Sherbrooke demo combining Unreal’s EQS sensing with an ML model instead of traditional behavior trees[2]. 

Discussion of fun in AI behavior: avoiding optimal-but-boring strategies (e.g., camping) for better gameplay[10]. 

Naruto Mobile’s Shūkai project: successfully using PPO in a commercial fighting game, with tailored training methods to align agent behavior with human players[11][8]. 

Stable-Baselines3 and research defaults for PPO hyperparameters, which informed our choices (e.g., 
𝛾=0.99,𝜆=0.95,
𝛾
=
0
.
99
,
𝜆
=
0
.
95
,
 
 LR 
3𝑒−4
3
e
−
4
 
, clip 0.2)[13]. Also, the benefit of lower entropy for stability in policy learning[15]. 

Example of deploying a trained PyTorch model in Rust via TorchScript and tch for fast inference[1]. 

 

[1]  Rust PyTorch - Using LibTorch, tch-rs :: Adhita Selvaraj — Be Kind  

https://www.swiftdiaries.com/rust/pytorch/ 

[2] [7] [10] [18] Machine Learning Could Create the Perfect Game Bosses | WIRED 

https://www.wired.com/story/machine-learning-ai-game-development-bosses-enemies/ 

[3] [4] PPO — Stable Baselines3 2.7.1a3 documentation 

https://stable-baselines3.readthedocs.io/en/master/modules/ppo.html 

[5] [6] [15] [P] Building an Reinforcement Learning Agent to play The Legend of Zelda : r/reinforcementlearning 

https://www.reddit.com/r/reinforcementlearning/comments/1i3t4nt/p_building_an_reinforcement_learning_agent_to/ 

[8] [9] [11] [12] [14] [16] [17] Advancing DRL Agents in Commercial Fighting Games: Training, Integration, and Agent-Human Alignment 

https://arxiv.org/html/2406.01103v1 

[13] deep learning - Stable Baselines 3: Default parameters - Stack Overflow 

https://stackoverflow.com/questions/75509729/stable-baselines-3-default-parameters 

[19] Asterion: NVIDIA's AI-Powered Boss is Redefining Gaming 

https://medialist.info/en/2025/02/23/asterion-nvidias-ai-powered-boss-is-redefining-gaming/ 

[20] Asterion is the world's first AI boss and it's set to change gaming ... 

https://www.instagram.com/p/DGDh29dNj-r/?hl=en 