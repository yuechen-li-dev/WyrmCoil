#![allow(non_snake_case)]

use winit::event::ElementState;
use winit::keyboard::{KeyCode, NativeKeyCode, PhysicalKey};
use wyrmcoil::Engine as wyrmcoil_engine;

#[test]
fn PlatformInputTranslationMapsSelectedKeysToNormalizedInputEvents() {
    assert_eq!(
        wyrmcoil_engine::TranslatePlatformInput(wyrmcoil_engine::PlatformInput::KeyPressed(
            wyrmcoil_engine::PlatformKey::Right,
        )),
        Some(wyrmcoil_engine::InputEvent::MoveRightPressed)
    );
    assert_eq!(
        wyrmcoil_engine::TranslatePlatformInput(wyrmcoil_engine::PlatformInput::KeyPressed(
            wyrmcoil_engine::PlatformKey::Left,
        )),
        Some(wyrmcoil_engine::InputEvent::MoveLeftPressed)
    );
    assert_eq!(
        wyrmcoil_engine::TranslatePlatformInput(wyrmcoil_engine::PlatformInput::KeyPressed(
            wyrmcoil_engine::PlatformKey::Stop,
        )),
        Some(wyrmcoil_engine::InputEvent::StopPressed)
    );
    assert_eq!(
        wyrmcoil_engine::TranslatePlatformInput(wyrmcoil_engine::PlatformInput::KeyReleased(
            wyrmcoil_engine::PlatformKey::Right,
        )),
        Some(wyrmcoil_engine::InputEvent::StopPressed)
    );
    assert_eq!(
        wyrmcoil_engine::TranslatePlatformInput(wyrmcoil_engine::PlatformInput::KeyPressed(
            wyrmcoil_engine::PlatformKey::Unknown,
        )),
        None
    );
}

#[test]
fn WinitKeyTranslationMapsSelectedKeysToPlatformInputs() {
    assert_eq!(
        wyrmcoil_engine::TranslateWinitKeyCode(KeyCode::ArrowRight, ElementState::Pressed),
        Some(wyrmcoil_engine::PlatformInput::KeyPressed(
            wyrmcoil_engine::PlatformKey::Right
        ))
    );
    assert_eq!(
        wyrmcoil_engine::TranslateWinitKeyCode(KeyCode::KeyD, ElementState::Pressed),
        Some(wyrmcoil_engine::PlatformInput::KeyPressed(
            wyrmcoil_engine::PlatformKey::Right
        ))
    );
    assert_eq!(
        wyrmcoil_engine::TranslateWinitKeyCode(KeyCode::ArrowLeft, ElementState::Pressed),
        Some(wyrmcoil_engine::PlatformInput::KeyPressed(
            wyrmcoil_engine::PlatformKey::Left
        ))
    );
    assert_eq!(
        wyrmcoil_engine::TranslateWinitKeyCode(KeyCode::Space, ElementState::Pressed),
        Some(wyrmcoil_engine::PlatformInput::KeyPressed(
            wyrmcoil_engine::PlatformKey::Stop
        ))
    );
    assert_eq!(
        wyrmcoil_engine::TranslateWinitKeyCode(KeyCode::KeyQ, ElementState::Pressed),
        Some(wyrmcoil_engine::PlatformInput::KeyPressed(
            wyrmcoil_engine::PlatformKey::AlertGuard
        ))
    );
    assert_eq!(
        wyrmcoil_engine::TranslateWinitKeyCode(KeyCode::KeyE, ElementState::Pressed),
        Some(wyrmcoil_engine::PlatformInput::KeyPressed(
            wyrmcoil_engine::PlatformKey::NudgeGuard
        ))
    );
    assert_eq!(
        wyrmcoil_engine::TranslateWinitKeyCode(KeyCode::F1, ElementState::Pressed),
        Some(wyrmcoil_engine::PlatformInput::KeyPressed(
            wyrmcoil_engine::PlatformKey::Unknown
        ))
    );
    assert_eq!(
        wyrmcoil_engine::TranslateWinitPhysicalKey(
            PhysicalKey::Unidentified(NativeKeyCode::Unidentified),
            ElementState::Pressed,
        ),
        None
    );
}

#[test]
fn QueueTranslatedInputBridgesToEngineQueueWithoutTickingClocks() {
    let mut engine = wyrmcoil_engine::Engine::New();
    let before = engine.Clock();

    let queued = wyrmcoil_engine::QueueTranslatedInput(
        &mut engine,
        wyrmcoil_engine::PlatformInput::KeyPressed(wyrmcoil_engine::PlatformKey::Right),
    );
    let queued_stop = wyrmcoil_engine::QueueTranslatedInput(
        &mut engine,
        wyrmcoil_engine::PlatformInput::KeyReleased(wyrmcoil_engine::PlatformKey::Left),
    );
    let ignored = wyrmcoil_engine::QueueTranslatedInput(
        &mut engine,
        wyrmcoil_engine::PlatformInput::KeyPressed(wyrmcoil_engine::PlatformKey::Unknown),
    );

    assert_eq!(queued, true);
    assert_eq!(queued_stop, true);
    assert_eq!(ignored, false);
    assert_eq!(
        engine.InputQueueSnapshot(),
        vec![
            wyrmcoil_engine::InputEvent::MoveRightPressed,
            wyrmcoil_engine::InputEvent::StopPressed
        ]
    );
    assert_eq!(
        engine.Clock(),
        before,
        "backend enqueue helpers must not advance control, simulation, or render clocks"
    );
}

#[test]
fn QueueWinitPhysicalKeyBridgesToEngineWithoutTickingOrWorldMutation() {
    let mut engine = wyrmcoil_engine::Engine::New();
    let before_clock = engine.Clock();
    let before_world = engine.World.clone();

    let queued = wyrmcoil_engine::QueueWinitPhysicalKey(
        &mut engine,
        PhysicalKey::Code(KeyCode::ArrowRight),
        ElementState::Pressed,
    );
    let ignored = wyrmcoil_engine::QueueWinitPhysicalKey(
        &mut engine,
        PhysicalKey::Code(KeyCode::F1),
        ElementState::Pressed,
    );

    assert_eq!(queued, true);
    assert_eq!(ignored, false);
    assert_eq!(engine.InputQueueLen(), 1);
    assert_eq!(
        engine.InputQueueSnapshot(),
        vec![wyrmcoil_engine::InputEvent::MoveRightPressed]
    );
    assert_eq!(engine.Clock(), before_clock);
    assert_eq!(engine.World, before_world);
}

#[test]
fn PlatformInputPathRespectsControlAndSimulationBoundaries() {
    let mut engine = wyrmcoil_engine::Engine::New();
    let baseline = engine
        .World
        .Transforms
        .Position(engine.Player)
        .expect("player should exist before backend bridge boundary test");

    wyrmcoil_engine::QueueTranslatedInput(
        &mut engine,
        wyrmcoil_engine::PlatformInput::KeyPressed(wyrmcoil_engine::PlatformKey::Right),
    );

    let before_sim = engine.RenderSnapshot();
    assert_eq!(before_sim.Items[0].Position, baseline);

    let _control = engine.TickControl();
    let after_control = engine.RenderSnapshot();
    assert_eq!(
        after_control.Items[0].Position, baseline,
        "control tick should consume input and issue acts, but world movement must wait for simulation"
    );

    engine.TickSimulation();
    let after_first_sim = engine.RenderSnapshot();

    let _control_2 = engine.TickControl();
    engine.TickSimulation();
    let after_second_sim = engine.RenderSnapshot();

    assert!(
        after_second_sim.Items[0].Position.X >= after_first_sim.Items[0].Position.X,
        "simulation ticks should be the only phase where movement integration appears"
    );
}
