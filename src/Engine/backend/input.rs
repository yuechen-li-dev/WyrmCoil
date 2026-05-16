#![allow(non_snake_case)]

use crate::Demo::{InputEvent, World};
use crate::Engine::Engine;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlatformKey {
    Right,
    Left,
    Stop,
    AlertGuard,
    NudgeGuard,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlatformInput {
    KeyPressed(PlatformKey),
    KeyReleased(PlatformKey),
}

pub fn TranslatePlatformInput(event: PlatformInput) -> Option<InputEvent> {
    match event {
        PlatformInput::KeyPressed(PlatformKey::Right) => Some(InputEvent::MoveRightPressed),
        PlatformInput::KeyPressed(PlatformKey::Left) => Some(InputEvent::MoveLeftPressed),
        PlatformInput::KeyPressed(PlatformKey::Stop) => Some(InputEvent::StopPressed),
        PlatformInput::KeyPressed(PlatformKey::AlertGuard) => Some(InputEvent::AlertGuardPressed),
        PlatformInput::KeyPressed(PlatformKey::NudgeGuard) => Some(InputEvent::NudgeGuardPressed),
        PlatformInput::KeyReleased(PlatformKey::Right)
        | PlatformInput::KeyReleased(PlatformKey::Left) => Some(InputEvent::StopPressed),
        PlatformInput::KeyReleased(_) | PlatformInput::KeyPressed(PlatformKey::Unknown) => None,
    }
}

pub fn QueueTranslatedInput(
    engine: &mut Engine<World, InputEvent>,
    platform_event: PlatformInput,
) -> bool {
    if let Some(input) = TranslatePlatformInput(platform_event) {
        engine.EnqueueInput(input);
        return true;
    }
    false
}
