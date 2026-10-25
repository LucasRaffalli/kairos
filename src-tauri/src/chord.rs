//! Les raccourcis globaux, utilisables même quand Kairos n'est pas au premier plan : ⌘K, puis dans
//! les deux secondes une touche. S arrête le lot, ↵ relance le dernier, P met en pause ou reprend,
//! Échap annule. ⌘K seul est enregistré en permanence ; S, ↵, P et Échap ne le sont que pendant ces
//! deux secondes, pour ne pas voler ces touches au reste du système.
//!
//! La machine d'états ([`step`]) et l'enregistrement des touches ([`FollowUps`]) sont purs et
//! testés ; le fil de travail ([`spawn`]) les branche sur le plugin et sur le pont. Les touches sont
//! les codes PHYSIQUES du plugin : S, P, ↵, Échap et K sont aux mêmes places en AZERTY et QWERTY.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_global_shortcut::{
    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutEvent, ShortcutState,
};

use crate::bridge;
use crate::inflight::InFlight;
use crate::tray;

/// Le temps laissé, après ⌘K, pour la touche suivante.
pub const CHORD_WINDOW: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// ⌘K, enregistré en permanence.
    Prefix,
    Stop,
    Relaunch,
    Pause,
    Cancel,
}

/// Les touches enregistrées seulement pendant que l'accord est armé.
const FOLLOW_UPS: [Key; 4] = [Key::Stop, Key::Relaunch, Key::Pause, Key::Cancel];

impl Key {
    fn shortcut(self) -> Shortcut {
        match self {
            Key::Prefix => Shortcut::new(Some(Modifiers::SUPER), Code::KeyK),
            Key::Stop => Shortcut::new(None, Code::KeyS),
            Key::Relaunch => Shortcut::new(None, Code::Enter),
            Key::Pause => Shortcut::new(None, Code::KeyP),
            Key::Cancel => Shortcut::new(None, Code::Escape),
        }
    }

    fn of(shortcut: &Shortcut) -> Option<Key> {
        [Key::Prefix]
            .into_iter()
            .chain(FOLLOW_UPS)
            .find(|key| key.shortcut().id() == shortcut.id())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Stop,
    Relaunch,
    TogglePause,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    Armed { until: Instant },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Key(Key),
    /// Le temps passe : l'accord expire s'il est armé et dépassé.
    Tick,
    /// L'enregistrement des touches suivantes a échoué : on n'est pas armé.
    ArmFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Arm,
    Disarm,
    Run(Action),
}

/// La machine d'états : l'état suivant et ce qu'il faut faire.
pub fn step(state: State, input: Input, now: Instant) -> (State, Vec<Effect>) {
    match (state, input) {
        (State::Idle, Input::Key(Key::Prefix)) => (
            State::Armed {
                until: now + CHORD_WINDOW,
            },
            vec![Effect::Arm],
        ),
        // Les touches suivantes ne sont enregistrées qu'armé : une arrivée tardive ne fait rien.
        (State::Idle, _) => (State::Idle, vec![]),
        (State::Armed { .. }, Input::Key(Key::Prefix)) => (
            State::Armed {
                until: now + CHORD_WINDOW,
            },
            vec![],
        ),
        (State::Armed { .. }, Input::Key(Key::Cancel)) => (State::Idle, vec![Effect::Disarm]),
        (State::Armed { .. }, Input::Key(key)) => {
            let action = match key {
                Key::Stop => Action::Stop,
                Key::Relaunch => Action::Relaunch,
                _ => Action::TogglePause,
            };
            (State::Idle, vec![Effect::Disarm, Effect::Run(action)])
        }
        (State::Armed { until }, Input::Tick) if now >= until => {
            (State::Idle, vec![Effect::Disarm])
        }
        (armed @ State::Armed { .. }, Input::Tick) => (armed, vec![]),
        (State::Armed { .. }, Input::ArmFailed) => (State::Idle, vec![]),
    }
}

/// Le temps à attendre avant la prochaine échéance ; `None` : rien n'expire.
pub fn wait(state: State, now: Instant) -> Option<Duration> {
    match state {
        State::Idle => None,
        State::Armed { until } => Some(until.saturating_duration_since(now)),
    }
}

/// Enregistrer et libérer une touche globale (le plugin, ou un faux dans les tests).
pub trait Registrar {
    fn register(&mut self, key: Key) -> Result<(), String>;
    fn unregister(&mut self, key: Key) -> Result<(), String>;
}

/// Les touches suivantes actuellement prises au système. Garantie : un armement qui échoue rend
/// celles qu'il a prises, et une touche qui refuse de se libérer est réessayée au désarmement suivant.
#[derive(Default)]
pub struct FollowUps {
    held: Vec<Key>,
}

impl FollowUps {
    pub fn arm(&mut self, registrar: &mut impl Registrar) -> Result<(), String> {
        let mut taken = Vec::new();
        for key in FOLLOW_UPS {
            if self.held.contains(&key) {
                continue;
            }
            if let Err(error) = registrar.register(key) {
                for key in taken {
                    if registrar.unregister(key).is_ok() {
                        self.held.retain(|held| *held != key);
                    }
                }
                return Err(format!("{key:?} : {error}"));
            }
            self.held.push(key);
            taken.push(key);
        }
        Ok(())
    }

    pub fn disarm(&mut self, registrar: &mut impl Registrar) {
        self.held.retain(|key| registrar.unregister(*key).is_err());
    }
}

/// Le titre de l'icône, avec « ⌘K… » devant tant que l'accord attend sa touche.
pub fn chord_title(title: Option<String>, armed: bool) -> Option<String> {
    match (title, armed) {
        (title, false) => title,
        (Some(title), true) => Some(format!("⌘K… {title}")),
        (None, true) => Some("⌘K…".to_string()),
    }
}

/// Ce que reçoit le fil des raccourcis.
enum Message {
    Key(Key),
    /// Le réglage `global_shortcuts` a changé.
    Enable(bool),
}

/// Le côté « app » du fil : envoyer des messages, et savoir si l'accord est armé (pour le titre).
pub struct Chords {
    sender: Sender<Message>,
    armed: Arc<AtomicBool>,
}

impl Chords {
    /// Active ou désactive ⌘K (le réglage `global_shortcuts`), sans relancer l'app.
    pub fn enable(&self, on: bool) {
        let _ = self.sender.send(Message::Enable(on));
    }

    pub fn is_armed(&self) -> bool {
        self.armed.load(Ordering::Relaxed)
    }
}

/// Le plugin (qui transmet les touches pressées au fil), le fil à lancer, et la poignée à gérer.
pub fn plugin() -> (tauri::plugin::TauriPlugin<Wry>, Worker, Chords) {
    let (sender, receiver) = mpsc::channel();
    let armed = Arc::new(AtomicBool::new(false));
    let handler_sender = sender.clone();
    let plugin = tauri_plugin_global_shortcut::Builder::new()
        .with_handler(move |_app, shortcut: &Shortcut, event: ShortcutEvent| {
            if event.state == ShortcutState::Pressed {
                if let Some(key) = Key::of(shortcut) {
                    let _ = handler_sender.send(Message::Key(key));
                }
            }
        })
        .build();
    let worker = Worker {
        receiver,
        armed: armed.clone(),
    };
    (plugin, worker, Chords { sender, armed })
}

pub struct Worker {
    receiver: Receiver<Message>,
    armed: Arc<AtomicBool>,
}

/// Le plugin comme registre de touches.
struct Plugin<'a>(&'a AppHandle);

impl Registrar for Plugin<'_> {
    fn register(&mut self, key: Key) -> Result<(), String> {
        self.0
            .global_shortcut()
            .register(key.shortcut())
            .map_err(|e| e.to_string())
    }

    fn unregister(&mut self, key: Key) -> Result<(), String> {
        self.0
            .global_shortcut()
            .unregister(key.shortcut())
            .map_err(|e| e.to_string())
    }
}

/// Lance le fil des raccourcis. Les enregistrements bloquent sur le fil principal : jamais depuis lui.
pub fn spawn(app: AppHandle, worker: Worker) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("kairos-chords".into())
        .spawn(move || run(&app, worker))?;
    Ok(())
}

fn run(app: &AppHandle, worker: Worker) {
    let Worker { receiver, armed } = worker;
    let mut registrar = Plugin(app);
    let mut follow_ups = FollowUps::default();
    let mut state = State::Idle;
    let mut prefix_taken = false;
    loop {
        let received = match wait(state, Instant::now()) {
            Some(timeout) => receiver.recv_timeout(timeout),
            None => receiver.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        let input = match received {
            Ok(Message::Key(key)) => Input::Key(key),
            Ok(Message::Enable(on)) => {
                if on != prefix_taken {
                    let result = if on {
                        registrar.register(Key::Prefix)
                    } else {
                        // Éteint : on rend aussi les touches suivantes d'un accord en cours.
                        follow_ups.disarm(&mut registrar);
                        state = State::Idle;
                        armed.store(false, Ordering::Relaxed);
                        registrar.unregister(Key::Prefix)
                    };
                    match result {
                        Ok(()) => prefix_taken = on,
                        // ⌘K déjà pris par le système ou une autre app : on le dit, sans plantage.
                        Err(error) => eprintln!("kairos : raccourci ⌘K indisponible ({error})"),
                    }
                }
                continue;
            }
            Err(RecvTimeoutError::Timeout) => Input::Tick,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        let (next, effects) = step(state, input, Instant::now());
        state = next;
        for effect in effects {
            match effect {
                Effect::Arm => match follow_ups.arm(&mut registrar) {
                    Ok(()) => armed.store(true, Ordering::Relaxed),
                    Err(error) => {
                        eprintln!("kairos : accord ⌘K impossible ({error})");
                        state = step(state, Input::ArmFailed, Instant::now()).0;
                    }
                },
                Effect::Disarm => {
                    follow_ups.disarm(&mut registrar);
                    armed.store(false, Ordering::Relaxed);
                }
                Effect::Run(action) => perform(app, action),
            }
        }
    }
}

/// Fait l'action par les mêmes fonctions que les commandes du pont, puis laisse le surveillant de la
/// base mettre le titre à jour. Sans lot à relancer, la fenêtre s'ouvre pour montrer l'état.
fn perform(app: &AppHandle, action: Action) {
    let actions = app.state::<InFlight>();
    let result = match action {
        Action::Stop => bridge::perform(&actions, bridge::stop).map(|_| true),
        Action::TogglePause => bridge::perform(&actions, bridge::toggle_pause).map(|()| true),
        Action::Relaunch => bridge::perform(&actions, bridge::relaunch_last),
    };
    match result {
        Ok(true) => {}
        Ok(false) => tray::show_popover(app),
        Err(error) => eprintln!("kairos : raccourci sans effet ({error:?})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn t0() -> Instant {
        Instant::now()
    }

    #[test]
    fn the_prefix_arms_the_chord_for_two_seconds() {
        let now = t0();
        let (state, effects) = step(State::Idle, Input::Key(Key::Prefix), now);
        assert_eq!(
            state,
            State::Armed {
                until: now + CHORD_WINDOW
            }
        );
        assert_eq!(effects, [Effect::Arm]);
        assert_eq!(CHORD_WINDOW, Duration::from_secs(2));
    }

    #[test]
    fn each_follow_up_key_runs_its_action_and_releases_the_keys() {
        for (key, action) in [
            (Key::Stop, Action::Stop),
            (Key::Relaunch, Action::Relaunch),
            (Key::Pause, Action::TogglePause),
        ] {
            let now = t0();
            let (armed, _) = step(State::Idle, Input::Key(Key::Prefix), now);
            let (state, effects) = step(armed, Input::Key(key), now + Duration::from_millis(500));
            assert_eq!(state, State::Idle, "{key:?}");
            assert_eq!(effects, [Effect::Disarm, Effect::Run(action)], "{key:?}");
        }
    }

    #[test]
    fn escape_cancels_without_acting() {
        let now = t0();
        let (armed, _) = step(State::Idle, Input::Key(Key::Prefix), now);
        let (state, effects) = step(armed, Input::Key(Key::Cancel), now);
        assert_eq!(state, State::Idle);
        assert_eq!(effects, [Effect::Disarm]);
    }

    #[test]
    fn the_chord_expires_after_two_seconds_and_not_before() {
        let now = t0();
        let (armed, _) = step(State::Idle, Input::Key(Key::Prefix), now);
        let early = step(armed, Input::Tick, now + Duration::from_millis(1999));
        assert_eq!(early, (armed, vec![]));
        let (state, effects) = step(armed, Input::Tick, now + CHORD_WINDOW);
        assert_eq!(state, State::Idle);
        assert_eq!(effects, [Effect::Disarm]);
    }

    #[test]
    fn a_repeated_prefix_restarts_the_window_without_registering_twice() {
        let now = t0();
        let (armed, _) = step(State::Idle, Input::Key(Key::Prefix), now);
        let later = now + Duration::from_millis(1500);
        let (state, effects) = step(armed, Input::Key(Key::Prefix), later);
        assert_eq!(
            state,
            State::Armed {
                until: later + CHORD_WINDOW
            }
        );
        assert!(effects.is_empty());
    }

    #[test]
    fn follow_up_keys_do_nothing_when_idle() {
        // Elles ne sont enregistrées qu'armé ; un événement tardif ne doit rien déclencher.
        for key in [Key::Stop, Key::Relaunch, Key::Pause, Key::Cancel] {
            assert_eq!(
                step(State::Idle, Input::Key(key), t0()),
                (State::Idle, vec![])
            );
        }
        assert_eq!(step(State::Idle, Input::Tick, t0()), (State::Idle, vec![]));
    }

    #[test]
    fn a_failed_arming_goes_back_to_idle() {
        let now = t0();
        let (armed, _) = step(State::Idle, Input::Key(Key::Prefix), now);
        assert_eq!(step(armed, Input::ArmFailed, now), (State::Idle, vec![]));
    }

    #[test]
    fn the_wait_is_the_time_left_or_none_when_idle() {
        let now = t0();
        assert_eq!(wait(State::Idle, now), None);
        let armed = State::Armed {
            until: now + CHORD_WINDOW,
        };
        assert_eq!(
            wait(armed, now + Duration::from_millis(500)),
            Some(Duration::from_millis(1500))
        );
        assert_eq!(
            wait(armed, now + Duration::from_secs(3)),
            Some(Duration::ZERO)
        );
    }

    /// Un registre factice : note ce qui est enregistré et peut refuser des touches.
    #[derive(Default)]
    struct Fake {
        held: Vec<Key>,
        refuse_register: Vec<Key>,
        refuse_unregister: Vec<Key>,
    }

    impl Registrar for Fake {
        fn register(&mut self, key: Key) -> Result<(), String> {
            if self.refuse_register.contains(&key) {
                return Err("prise ailleurs".into());
            }
            self.held.push(key);
            Ok(())
        }

        fn unregister(&mut self, key: Key) -> Result<(), String> {
            if self.refuse_unregister.contains(&key) {
                return Err("refus".into());
            }
            self.held.retain(|k| *k != key);
            Ok(())
        }
    }

    #[test]
    fn arming_registers_the_four_follow_up_keys_and_disarming_frees_them() {
        let mut reg = Fake::default();
        let mut keys = FollowUps::default();
        keys.arm(&mut reg).unwrap();
        assert_eq!(reg.held, FOLLOW_UPS);
        keys.disarm(&mut reg);
        assert!(reg.held.is_empty());
    }

    #[test]
    fn a_refused_key_rolls_back_the_ones_already_taken() {
        let mut reg = Fake {
            refuse_register: vec![Key::Pause],
            ..Fake::default()
        };
        let mut keys = FollowUps::default();
        assert!(keys.arm(&mut reg).is_err());
        assert!(reg.held.is_empty(), "restées : {:?}", reg.held);
    }

    #[test]
    fn a_key_that_will_not_unregister_is_retried_at_the_next_disarm() {
        let mut reg = Fake {
            refuse_unregister: vec![Key::Stop],
            ..Fake::default()
        };
        let mut keys = FollowUps::default();
        keys.arm(&mut reg).unwrap();
        keys.disarm(&mut reg);
        assert_eq!(reg.held, [Key::Stop], "les autres sont libérées");
        // Le système la rend ; le désarmement suivant la libère, et un armement ne la double pas.
        reg.refuse_unregister.clear();
        keys.arm(&mut reg).unwrap();
        assert_eq!(reg.held.iter().filter(|k| **k == Key::Stop).count(), 1);
        keys.disarm(&mut reg);
        assert!(reg.held.is_empty());
    }

    #[test]
    fn the_title_shows_the_pending_chord() {
        assert_eq!(
            chord_title(Some("0:42:13".into()), false).as_deref(),
            Some("0:42:13")
        );
        assert_eq!(chord_title(None, false), None);
        assert_eq!(
            chord_title(Some("0:42:13".into()), true).as_deref(),
            Some("⌘K… 0:42:13")
        );
        assert_eq!(chord_title(None, true).as_deref(), Some("⌘K…"));
    }
}
