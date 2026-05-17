//! Input buffering — read every fixed tick into a tiny ring; drain when the
//! player's state allows it.
//!
//! Without this, attacks pressed during recovery vanish. Sifu and Sekiro both
//! preserve presses for a short window (we use 200 ms).

use crate::game::feel::INPUT_BUFFER_TICKS;
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferedAction {
    LightAttack,
    HeavyAttack,
    Parry,
    Dodge,
    /// Posture-break execution. Bound to `E`; only meaningful when the boss
    /// is in the broken window and the player is in range.
    Execute,
}

#[derive(Clone, Copy, Debug)]
pub struct BufferedPress {
    pub action: BufferedAction,
    pub tick_pressed: u64,
}

#[derive(Resource, Default, Debug)]
pub struct InputBuffer {
    pub queue: VecDeque<BufferedPress>,
    pub tick: u64,
}

impl InputBuffer {
    pub fn push(&mut self, action: BufferedAction) {
        if self.queue.len() >= 4 {
            self.queue.pop_front();
        }
        self.queue.push_back(BufferedPress {
            action,
            tick_pressed: self.tick,
        });
    }

    /// Drop entries older than `INPUT_BUFFER_TICKS`.
    fn evict_stale(&mut self) {
        let cutoff = self.tick.saturating_sub(INPUT_BUFFER_TICKS);
        while let Some(front) = self.queue.front() {
            if front.tick_pressed < cutoff {
                self.queue.pop_front();
            } else {
                break;
            }
        }
    }

    /// Pop the oldest still-valid press matching `pred`. Returns it.
    pub fn consume<F: Fn(BufferedAction) -> bool>(&mut self, pred: F) -> Option<BufferedPress> {
        self.evict_stale();
        let mut idx = None;
        for (i, p) in self.queue.iter().enumerate() {
            if pred(p.action) {
                idx = Some(i);
                break;
            }
        }
        if let Some(i) = idx {
            self.queue.remove(i)
        } else {
            None
        }
    }

    #[allow(dead_code)]
    pub fn peek_any(&mut self) -> Option<BufferedPress> {
        self.evict_stale();
        self.queue.front().copied()
    }
}

/// Read the keyboard, mouse, AND any connected gamepad each fixed tick and
/// push presses into the buffer.
///
/// PlayStation / DualShock / DualSense mapping (Bevy uses cardinal names):
/// - **R1**           → Light Attack       (`GamepadButton::RightTrigger`)
/// - **R2**           → Heavy Attack       (`GamepadButton::RightTrigger2`)
/// - **L1**           → Parry (tap) / Block (hold) — `GamepadButton::LeftTrigger`
/// - **Cross (X)**    → Dodge              (`GamepadButton::South`)
/// - **Triangle**     → Execute            (`GamepadButton::North`)
/// - **R3** click     → Lock-on toggle (handled in `lockon.rs`)
/// - Left stick       → Movement (handled in `player_movement`)
///
/// Both input sources push into the same `InputBuffer`; the player can
/// switch between keyboard and pad mid-fight without losing presses.
pub fn read_input_to_buffer_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    gamepads: Query<&Gamepad>,
    mut buffer: ResMut<InputBuffer>,
) {
    buffer.tick = buffer.tick.saturating_add(1);

    // ─── Keyboard + mouse ──
    if keyboard.just_pressed(KeyCode::Space) || mouse.just_pressed(MouseButton::Left) {
        buffer.push(BufferedAction::LightAttack);
    }
    if mouse.just_pressed(MouseButton::Right) {
        buffer.push(BufferedAction::HeavyAttack);
    }
    if keyboard.just_pressed(KeyCode::KeyQ) {
        buffer.push(BufferedAction::Parry);
    }
    if keyboard.just_pressed(KeyCode::ShiftLeft) {
        buffer.push(BufferedAction::Dodge);
    }
    if keyboard.just_pressed(KeyCode::KeyE) {
        buffer.push(BufferedAction::Execute);
    }

    // ─── Gamepad (PlayStation layout) ──
    for gamepad in &gamepads {
        if gamepad.just_pressed(GamepadButton::RightTrigger) {
            buffer.push(BufferedAction::LightAttack);
        }
        if gamepad.just_pressed(GamepadButton::RightTrigger2) {
            buffer.push(BufferedAction::HeavyAttack);
        }
        if gamepad.just_pressed(GamepadButton::LeftTrigger) {
            buffer.push(BufferedAction::Parry);
        }
        if gamepad.just_pressed(GamepadButton::South) {
            buffer.push(BufferedAction::Dodge);
        }
        if gamepad.just_pressed(GamepadButton::North) {
            buffer.push(BufferedAction::Execute);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evicts_stale_entries() {
        let mut buf = InputBuffer::default();
        buf.tick = 0;
        buf.push(BufferedAction::LightAttack);
        buf.tick = INPUT_BUFFER_TICKS + 1;
        assert!(buf.peek_any().is_none());
    }

    #[test]
    fn consume_returns_first_match() {
        let mut buf = InputBuffer::default();
        buf.push(BufferedAction::Parry);
        buf.push(BufferedAction::LightAttack);
        let p = buf.consume(|a| a == BufferedAction::LightAttack).unwrap();
        assert_eq!(p.action, BufferedAction::LightAttack);
        // The Parry that was queued first remains.
        assert_eq!(buf.queue.len(), 1);
        assert_eq!(buf.queue[0].action, BufferedAction::Parry);
    }
}
