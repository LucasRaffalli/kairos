//! Les actions en vol (lancer, changer, arrêter, pause, reprise) : quitter attend qu'elles se
//! terminent, pour qu'une écriture ne soit jamais coupée par la fermeture de l'app.

use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Attente maximale des actions en vol avant de quitter quand même.
pub const QUIT_WAIT: Duration = Duration::from_secs(3);

/// Le nombre d'actions en cours, partagé entre les commandes et « Quitter ».
#[derive(Default, Clone)]
pub struct InFlight(Arc<Counter>);

#[derive(Default)]
struct Counter {
    count: Mutex<usize>,
    idle: Condvar,
}

/// Une action en cours : elle se termine quand le garde est relâché, même sur une erreur.
pub struct Guard(Arc<Counter>);

impl InFlight {
    pub fn begin(&self) -> Guard {
        *self.lock() += 1;
        Guard(Arc::clone(&self.0))
    }

    /// Attend qu'aucune action ne soit en cours, au plus `timeout`. Vrai si c'est le cas.
    pub fn wait_idle(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut count = self.lock();
        while *count > 0 {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            count = self
                .0
                .idle
                .wait_timeout(count, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        true
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, usize> {
        // Un compteur reste juste même si un fil a paniqué en le tenant.
        self.0.count.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        let mut count = self.0.count.lock().unwrap_or_else(PoisonError::into_inner);
        *count = count.saturating_sub(1);
        if *count == 0 {
            self.0.idle.notify_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn nothing_in_flight_is_idle_at_once() {
        let actions = InFlight::default();
        assert!(actions.wait_idle(Duration::ZERO));
    }

    #[test]
    fn an_action_in_flight_holds_the_wait_until_the_deadline() {
        let actions = InFlight::default();
        let _guard = actions.begin();
        let started = Instant::now();
        assert!(!actions.wait_idle(Duration::from_millis(50)));
        assert!(started.elapsed() >= Duration::from_millis(50));
    }

    #[test]
    fn the_wait_ends_when_the_last_action_finishes() {
        let actions = InFlight::default();
        let first = actions.begin();
        let second = actions.begin();
        drop(first);
        assert!(!actions.wait_idle(Duration::from_millis(10)));
        let started = Instant::now();
        let worker = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            drop(second);
        });
        assert!(actions.wait_idle(Duration::from_secs(3)));
        let waited = started.elapsed();
        assert!(waited >= Duration::from_millis(40), "{waited:?}");
        assert!(waited < Duration::from_secs(3), "{waited:?}");
        worker.join().unwrap();
    }
}
