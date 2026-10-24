//! L'horloge, injectable : le cœur ne lit jamais l'heure du système directement.

use std::sync::atomic::{AtomicI64, Ordering};

/// Une source de temps. `now` donne les secondes depuis l'epoch (UTC) ;
/// `utc_offset` le décalage local, en secondes, **à l'instant donné** : il change
/// au passage à l'heure d'été, et les limites d'un jour en dépendent.
pub trait Clock {
    fn now(&self) -> i64;
    fn utc_offset(&self, at: i64) -> i32;
}

/// L'horloge du système, dans le fuseau local de la machine.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> i64 {
        chrono::Utc::now().timestamp()
    }

    fn utc_offset(&self, at: i64) -> i32 {
        use chrono::{Local, Offset, TimeZone};
        Local
            .timestamp_opt(at, 0)
            .single()
            .map_or(0, |d| d.offset().fix().local_minus_utc())
    }
}

/// Une horloge pour les tests : l'heure se règle et avance à la main.
pub struct FakeClock {
    now: AtomicI64,
    offset: Box<dyn Fn(i64) -> i32 + Send + Sync>,
}

impl FakeClock {
    /// Heure de départ et décalage constant.
    pub fn new(now: i64, offset: i32) -> Self {
        FakeClock {
            now: AtomicI64::new(now),
            offset: Box::new(move |_| offset),
        }
    }

    /// Remplace le décalage constant par une fonction de l'instant (changement d'heure).
    pub fn with_offset_fn(mut self, f: Box<dyn Fn(i64) -> i32 + Send + Sync>) -> Self {
        self.offset = f;
        self
    }

    pub fn set_now(&self, now: i64) {
        self.now.store(now, Ordering::SeqCst);
    }

    pub fn advance(&self, secs: i64) {
        self.now.fetch_add(secs, Ordering::SeqCst);
    }
}

impl Clock for FakeClock {
    fn now(&self) -> i64 {
        self.now.load(Ordering::SeqCst)
    }

    fn utc_offset(&self, at: i64) -> i32 {
        (self.offset)(at)
    }
}
