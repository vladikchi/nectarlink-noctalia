// SPDX-License-Identifier: GPL-3.0-or-later
//! Local preparation state only. Already sent offers retain upstream validity.
use nectarlink_core::DeviceId;
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

pub(super) const ASK_FOR: Duration = Duration::from_secs(60);
#[derive(Clone, Copy)]
enum Phase {
    Preparing,
    Asking(Instant),
}
#[derive(Clone, Copy)]
struct Pending {
    device: DeviceId,
    phase: Phase,
}
#[derive(Default)]
pub(super) struct State {
    generation: u64,
    pending: Option<Pending>,
}

impl State {
    pub(super) fn begin(&mut self, device: DeviceId) -> u64 {
        self.generation += 1;
        self.pending = Some(Pending { device, phase: Phase::Preparing });
        self.generation
    }
    pub(super) fn current(&self, generation: u64) -> bool {
        self.generation == generation
    }
    pub(super) fn asking(&mut self, generation: u64) -> bool {
        if !self.current(generation) {
            return false;
        }
        let Some(pending) = &mut self.pending else {
            return false;
        };
        pending.phase = Phase::Asking(Instant::now());
        true
    }
    pub(super) fn finish_preparing(&mut self, generation: u64) -> bool {
        if self.current(generation) && self.pending.is_some_and(|p| matches!(p.phase, Phase::Preparing)) {
            self.pending = None;
            return true;
        }
        false
    }
    pub(super) fn snapshot(&self) -> Option<(DeviceId, &'static str)> {
        self.pending.and_then(|p| match p.phase {
            Phase::Preparing => Some((p.device, "preparing")),
            Phase::Asking(at) if at.elapsed() < ASK_FOR => Some((p.device, "asking")),
            _ => None,
        })
    }
    pub(super) fn observed_watching(&mut self, device: DeviceId) {
        if self.pending.is_some_and(|p| p.device == device && matches!(p.phase, Phase::Asking(_))) {
            self.pending = None;
        }
    }
    pub(super) fn cancel(&mut self, device: Option<DeviceId>) -> bool {
        if let Some(device) = device {
            if self.snapshot().is_some_and(|(pending, _)| pending != device) {
                return false;
            }
        }
        self.generation += 1;
        self.pending = None;
        true
    }
}
static STATE: Mutex<State> = Mutex::new(State { generation: 0, pending: None });
pub(super) fn state() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_completion_cannot_clear_or_mark_a_new_request() {
        let mut s = State::default();
        let old = s.begin(DeviceId([1; 32]));
        let new = s.begin(DeviceId([2; 32]));
        s.finish_preparing(old);
        assert!(!s.asking(old));
        assert_eq!(s.snapshot(), Some((DeviceId([2; 32]), "preparing")));
        assert!(s.asking(new));
        s.finish_preparing(new);
        assert_eq!(s.snapshot(), Some((DeviceId([2; 32]), "asking")));
    }
    #[test]
    fn cancel_is_device_scoped_and_invalidates_preparation() {
        let mut s = State::default();
        let a = DeviceId([1; 32]);
        let b = DeviceId([2; 32]);
        let old = s.begin(a);
        assert!(!s.cancel(Some(b)));
        assert!(s.current(old));
        assert!(s.cancel(Some(a)));
        assert!(!s.current(old));
        assert!(!s.asking(old));
        assert_eq!(s.snapshot(), None);
    }
    #[test]
    fn watching_does_not_clear_a_new_preparation_or_another_phone() {
        let mut s = State::default();
        let a = DeviceId([1; 32]);
        let b = DeviceId([2; 32]);
        let request = s.begin(a);
        s.observed_watching(a);
        assert!(s.snapshot().is_some());
        assert!(s.asking(request));
        s.observed_watching(b);
        assert!(s.snapshot().is_some());
        s.observed_watching(a);
        assert_eq!(s.snapshot(), None);
    }
}
