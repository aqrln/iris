use alloc::{boxed::Box, collections::btree_map::BTreeMap, vec::Vec};
use bitflags::bitflags;

use crate::mmu::{
    addr::{AddressRange, PageType, PhysicalAddr, VirtualAddr},
    pte::PteSlot,
};

pub mod addr;
mod pte;

#[repr(C, align(4096))]
struct PageTable {
    entries: [PteSlot; 512],
}

impl PageTable {
    fn new() -> Self {
        Self {
            entries: [const { PteSlot::unmapped() }; _],
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AddressSpaceId(u16);

impl AddressSpaceId {
    pub fn kernel() -> Self {
        Self(0)
    }

    fn get(self) -> u16 {
        self.0
    }
}

struct AddressSpace {
    root: Box<PageTable>,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum MapError {
    #[error("virtual address {0} is not aligned to page size")]
    VirtualUnaligned(VirtualAddr),
    #[error("physical address {0} is not aligned to page size")]
    PhysicalUnaligned(PhysicalAddr),
    #[error(
        "virtual range {0} contains {vsize} bytes but physical range {1} contains {psize} bytes",
        vsize = .0.size(),
        psize = .1.size())
    ]
    MismatchedLength(AddressRange<VirtualAddr>, AddressRange<PhysicalAddr>),
    #[error("range {requested} overlaps with already mapped range {mapped} in this address space")]
    Conflict {
        requested: VirtualAddr,
        mapped: AddressRange<VirtualAddr>,
    },
}

bitflags! {
    #[derive(Debug, Copy, Clone, PartialEq, Eq)]
    pub struct PagePermissions: u8 {
        const READ = 1 << 0;
        const WRITE = 1 << 1;
        const EXECUTE = 1 << 2;
    }
}

impl AddressSpace {
    fn new() -> Self {
        Self {
            root: Box::new(PageTable::new()),
        }
    }

    fn new_with_global_mappings(
        mappings: &[(AddressRange<VirtualAddr>, PagePermissions)],
    ) -> Result<Self, MapError> {
        let mut space = Self::new();
        for &(range, permissions) in mappings {
            space.identity_map_range(range, permissions)?;
        }
        Ok(space)
    }

    fn identity_map_range(
        &mut self,
        range: AddressRange<VirtualAddr>,
        permissions: PagePermissions,
    ) -> Result<(), MapError> {
        self.map_range(range, range.identity_mapped_physical(), permissions)
    }

    fn map_range(
        &mut self,
        mut virtual_range: AddressRange<VirtualAddr>,
        mut physical_range: AddressRange<PhysicalAddr>,
        permissions: PagePermissions,
    ) -> Result<(), MapError> {
        for addr in [virtual_range.start, virtual_range.end] {
            if !addr.is_aligned(PageType::Small) {
                return Err(MapError::VirtualUnaligned(addr));
            }
        }

        for addr in [physical_range.start, physical_range.end] {
            if !addr.is_aligned(PageType::Small) {
                return Err(MapError::PhysicalUnaligned(addr));
            }
        }

        if virtual_range.size() != physical_range.size() {
            return Err(MapError::MismatchedLength(virtual_range, physical_range));
        }

        for page_type in [PageType::Huge, PageType::Large, PageType::Small] {
            while virtual_range.start.is_aligned(page_type)
                && physical_range.start.is_aligned(page_type)
                && virtual_range.size() >= page_type.size()
                && physical_range.size() >= page_type.size()
            {
                self.map_page(
                    page_type,
                    virtual_range.start,
                    physical_range.start,
                    permissions,
                )?;
                let offset = page_type.size() as isize;
                virtual_range.start = virtual_range.start.offset(offset);
                physical_range.start = physical_range.start.offset(offset);
            }
        }

        Ok(())
    }

    fn map_page(
        &mut self,
        page_type: PageType,
        virtual_addr: VirtualAddr,
        physical_addr: PhysicalAddr,
        permissions: PagePermissions,
    ) -> Result<(), MapError> {
        todo!()
    }
}

pub struct MemoryManager {
    address_spaces: BTreeMap<AddressSpaceId, AddressSpace>,
    _global_mappings: Vec<(AddressRange<VirtualAddr>, PagePermissions)>,
}

impl MemoryManager {
    pub fn new_with_global_mappings(
        global_mappings: Vec<(AddressRange<VirtualAddr>, PagePermissions)>,
    ) -> Result<Self, MapError> {
        let mut address_spaces = BTreeMap::new();
        address_spaces.insert(
            AddressSpaceId::kernel(),
            AddressSpace::new_with_global_mappings(&global_mappings)?,
        );
        Ok(Self {
            address_spaces,
            _global_mappings: global_mappings,
        })
    }

    pub fn map_kernel_mmio(&mut self, range: AddressRange<PhysicalAddr>) -> Result<(), MapError> {
        self.map_kernel_identity(
            range.identity_mapped_virtual(),
            PagePermissions::READ | PagePermissions::WRITE,
        )
    }

    pub fn map_kernel_identity(
        &mut self,
        range: AddressRange<VirtualAddr>,
        permissions: PagePermissions,
    ) -> Result<(), MapError> {
        self.kernel_address_space_mut()
            .identity_map_range(range, permissions)
    }

    fn kernel_address_space_mut(&mut self) -> &mut AddressSpace {
        self.address_spaces
            .get_mut(&AddressSpaceId::kernel())
            .expect("kernel address space should exist")
    }

    pub fn sync(asid: AddressSpaceId) {
        riscv::asm::sfence_vma(asid.get() as _, 0);
    }

    pub fn sync_all() {
        riscv::asm::sfence_vma_all();
    }
}
