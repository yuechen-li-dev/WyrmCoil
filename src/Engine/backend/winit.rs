#![allow(non_snake_case)]

use crate::Engine::Engine;
use crate::Engine::backend::{PlatformInput, PlatformKey, QueueTranslatedInput};
use winit::event::ElementState;
use winit::keyboard::{KeyCode, PhysicalKey};

pub fn TranslateWinitKeyCode(code: KeyCode, state: ElementState) -> Option<PlatformInput> {
    let key = match code {
        KeyCode::ArrowRight | KeyCode::KeyD => PlatformKey::Right,
        KeyCode::ArrowLeft | KeyCode::KeyA => PlatformKey::Left,
        KeyCode::Space => PlatformKey::Stop,
        KeyCode::KeyQ => PlatformKey::AlertGuard,
        KeyCode::KeyE => PlatformKey::NudgeGuard,
        _ => PlatformKey::Unknown,
    };

    match state {
        ElementState::Pressed => Some(PlatformInput::KeyPressed(key)),
        ElementState::Released => Some(PlatformInput::KeyReleased(key)),
    }
}

pub fn TranslateWinitPhysicalKey(key: PhysicalKey, state: ElementState) -> Option<PlatformInput> {
    match key {
        PhysicalKey::Code(code) => TranslateWinitKeyCode(code, state),
        PhysicalKey::Unidentified(_) => None,
    }
}

pub fn QueueWinitPhysicalKey(engine: &mut Engine, key: PhysicalKey, state: ElementState) -> bool {
    if let Some(platform_input) = TranslateWinitPhysicalKey(key, state) {
        return QueueTranslatedInput(engine, platform_input);
    }
    false
}
