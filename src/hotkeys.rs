//! Global hotkeys: Ctrl+Alt+Up (brighter), Ctrl+Alt+Down (dimmer), Ctrl+Alt+P (pause).

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

pub enum Action {
    Brighter,
    Dimmer,
    TogglePause,
}

pub struct Hotkeys {
    /// Must stay alive: dropping the manager unregisters the hotkeys.
    _manager: Option<GlobalHotKeyManager>,
    brighter: HotKey,
    dimmer: HotKey,
    pause: HotKey,
}

impl Hotkeys {
    /// Must be called on the main (event loop) thread.
    pub fn new(enabled: bool) -> Self {
        let mods = Some(Modifiers::CONTROL | Modifiers::ALT);
        let brighter = HotKey::new(mods, Code::ArrowUp);
        let dimmer = HotKey::new(mods, Code::ArrowDown);
        let pause = HotKey::new(mods, Code::KeyP);

        let manager = if enabled {
            match GlobalHotKeyManager::new() {
                Ok(m) => Some(m),
                Err(e) => {
                    log!("Hotkeys unavailable: {e}");
                    None
                }
            }
        } else {
            None
        };
        if let Some(m) = &manager {
            for hotkey in [brighter, dimmer, pause] {
                // Fails if another app already owns the combination; the rest still work.
                if let Err(e) = m.register(hotkey) {
                    log!("Could not register hotkey {hotkey:?}: {e}");
                }
            }
        }
        Self { _manager: manager, brighter, dimmer, pause }
    }

    /// Next pending hotkey press, if any. Call repeatedly until it returns None.
    pub fn try_next(&self) -> Option<Action> {
        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.state != HotKeyState::Pressed {
                continue;
            }
            if event.id == self.brighter.id() {
                return Some(Action::Brighter);
            } else if event.id == self.dimmer.id() {
                return Some(Action::Dimmer);
            } else if event.id == self.pause.id() {
                return Some(Action::TogglePause);
            }
        }
        None
    }
}
