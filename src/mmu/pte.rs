use core::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy)]
#[repr(transparent)]
pub(super) struct PteValue(u64);

#[repr(transparent)]
pub(super) struct PteSlot(AtomicU64);

impl PteSlot {
    pub const fn new(value: PteValue) -> Self {
        Self(AtomicU64::new(value.0))
    }

    pub const fn unmapped() -> Self {
        Self::new(PteValue(0))
    }

    fn load(&self) -> PteValue {
        PteValue(self.0.load(Ordering::Relaxed))
    }
}
