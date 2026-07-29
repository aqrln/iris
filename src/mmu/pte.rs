use core::sync::atomic::{AtomicU64, Ordering};

use bitflags::bitflags;

use crate::mmu::{
    PagePermissions,
    addr::{PageType, PhysicalAddr},
};

bitflags! {
    struct PteFlags: u64 {
        const VALID = 1 << 0;
        const READ = 1 << 1;
        const WRITE = 1 << 2;
        const EXECUTE = 1 << 3;
        const USER = 1 << 4;
        const GLOBAL = 1 << 5;
        const ACCESSED = 1 << 6;
        const DIRTY = 1 << 7;
    }
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum InvalidPermissions {
    #[error("no permissions set, this is a non-leaf pte marker")]
    Empty,
    #[error("write-only pages are not supported")]
    WriteOnly,
}

impl TryFrom<PagePermissions> for PteFlags {
    type Error = InvalidPermissions;

    fn try_from(permissions: PagePermissions) -> Result<Self, Self::Error> {
        let mut flags = Self::empty();

        for perm in permissions {
            flags |= match perm {
                PagePermissions::READ => Self::READ,
                PagePermissions::WRITE => Self::WRITE,
                PagePermissions::EXECUTE => Self::EXECUTE,
                _ => Self::empty(),
            }
        }

        if flags.is_empty() {
            return Err(InvalidPermissions::Empty);
        }

        if flags.contains(Self::WRITE) && !flags.contains(Self::READ) {
            return Err(InvalidPermissions::WriteOnly);
        }

        Ok(flags | Self::VALID)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub(super) struct PteValue(u64);

impl PteValue {
    pub const fn unmapped() -> Self {
        Self(0)
    }

    fn new(addr: PhysicalAddr, flags: PteFlags) -> Self {
        // TODO: introduce a static PageAlignedPhysicalAddr proof to avoid repeating checks.
        // AddressSpace::map_page should also assert both virtual and physical address
        // alignment on the type level, since it currently assumes the caller to do those checks.
        assert!(addr.is_aligned(PageType::Small));
        Self((addr.ppn() << 10) | flags.bits())
    }

    // TODO: see the TODO above
    pub fn leaf(
        addr: PhysicalAddr,
        permissions: PagePermissions,
    ) -> Result<Self, InvalidPermissions> {
        Ok(Self::new(addr, PteFlags::try_from(permissions)?))
    }

    // TODO: see the TODO above
    pub fn non_leaf(addr: PhysicalAddr) -> Self {
        Self::new(addr, PteFlags::VALID)
    }

    pub fn is_valid(self) -> bool {
        PteFlags::from_bits_truncate(self.0).contains(PteFlags::VALID)
    }

    pub fn is_leaf(self) -> bool {
        PteFlags::from_bits_truncate(self.0)
            .intersects(PteFlags::READ | PteFlags::WRITE | PteFlags::EXECUTE)
    }

    pub fn ppn(self) -> u64 {
        self.0 >> 10
    }
}

#[derive(Debug)]
#[repr(transparent)]
pub(super) struct PteSlot(AtomicU64);

impl PteSlot {
    pub const fn new(value: PteValue) -> Self {
        Self(AtomicU64::new(value.0))
    }

    pub fn load(&self) -> PteValue {
        PteValue(self.0.load(Ordering::Relaxed))
    }

    pub fn store(&self, value: PteValue) {
        self.0.store(value.0, Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;

    tests! {
        fn test_pte_leaf() {
            assert_eq!(
                PteValue::leaf(0x1000.into(), PagePermissions::empty()),
                Err(InvalidPermissions::Empty)
            );

            assert_eq!(
                PteValue::leaf(0x1000.into(), PagePermissions::READ),
                Ok(PteValue(0x403))
            );

            assert_eq!(
                PteValue::leaf(0x1000.into(), PagePermissions::WRITE),
                Err(InvalidPermissions::WriteOnly)
            );

            assert_eq!(
                PteValue::leaf(0x1000.into(), PagePermissions::EXECUTE),
                Ok(PteValue(0x409))
            );

            assert_eq!(
                PteValue::leaf(0x1000.into(), PagePermissions::READ | PagePermissions::WRITE),
                Ok(PteValue(0x407))
            );

            assert_eq!(
                PteValue::leaf(0x1000.into(), PagePermissions::READ | PagePermissions::EXECUTE),
                Ok(PteValue(0x40b))
            );

            assert_eq!(
                PteValue::leaf(0x1000.into(), PagePermissions::WRITE | PagePermissions::EXECUTE),
                Err(InvalidPermissions::WriteOnly)
            );

            assert_eq!(
                PteValue::leaf(0x1000.into(), PagePermissions::all()),
                Ok(PteValue(0x40f))
            );
        }

        fn test_pte_nonleaf() {
            assert_eq!(
                PteValue::non_leaf(0x1000.into()),
                PteValue(0x401)
            );
        }
    }
}
