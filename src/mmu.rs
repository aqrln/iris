use alloc::{boxed::Box, collections::btree_map::BTreeMap, vec::Vec};
use bitflags::bitflags;

use crate::{
    mmu::{
        addr::{AddressRange, PageType, PhysicalAddr, VirtualAddr},
        pte::{InvalidPermissions, PteSlot, PteValue},
    },
    println,
};

pub mod addr;
pub mod pte;

#[repr(C, align(4096))]
struct PageTable {
    entries: [PteSlot; 512],
}

impl PageTable {
    fn new() -> Self {
        Self {
            entries: [const { PteSlot::new(PteValue::unmapped()) }; _],
        }
    }

    fn entry(&self, idx: usize) -> &PteSlot {
        &self.entries[idx]
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
    #[error("range {0} is already mapped in this address space")]
    Conflict(AddressRange<VirtualAddr>),
    #[error("invalid page permissions: {0}")]
    InvalidPermissions(#[from] InvalidPermissions),
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
        println!("mapping range {virtual_range}");

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

        assert!(virtual_range.size() == 0);
        assert!(physical_range.size() == 0);

        Ok(())
    }

    /// Maps a single page without alignment checks.
    ///
    /// Although the page table has interior mutability and an exclusive reference
    /// is not required for mutation, it is important for correctness: it statically
    /// proves that this method cannot be used concurrently and nothing can mutate
    /// the page table entries at the same time (other than the CPU setting the
    /// dirty/accessed flags).
    fn map_page(
        &mut self,
        page_type: PageType,
        virtual_addr: VirtualAddr,
        physical_addr: PhysicalAddr,
        permissions: PagePermissions,
    ) -> Result<(), MapError> {
        let map_leaf_pte = |table: &PageTable, pte_index| {
            let pte = table.entry(pte_index);
            if pte.load().is_valid() {
                Err(MapError::Conflict(AddressRange::page(
                    virtual_addr,
                    page_type,
                )))
            } else {
                Ok(pte.store(PteValue::leaf(physical_addr, permissions)?))
            }
        };

        let get_or_create_page_table = |parent_table: &PageTable, parent_pte_index| {
            let pte = parent_table.entry(parent_pte_index);
            let pte_val = pte.load();

            if !pte_val.is_valid() {
                let next_table = Box::leak(Box::new(PageTable::new())) as &_;
                let addr = VirtualAddr::expose_provenance(next_table as *const _);
                let phys_addr = addr.identity_mapped_physical();
                pte.store(PteValue::non_leaf(phys_addr));
                Ok(next_table)
            } else if pte_val.is_leaf() {
                Err(MapError::Conflict(AddressRange::page(
                    virtual_addr,
                    page_type,
                )))
            } else {
                let phys_addr = PhysicalAddr::from_ppn(pte_val.ppn());
                let addr = phys_addr.identity_mapped_virtual();
                let ptr = core::ptr::with_exposed_provenance::<PageTable>(addr.get());
                Ok(unsafe { &*ptr })
            }
        };

        let map_indirect_pte = |parent_table, parent_pte_index, leaf_pte_index| {
            map_leaf_pte(
                get_or_create_page_table(parent_table, parent_pte_index)?,
                leaf_pte_index,
            )
        };

        match page_type {
            PageType::Huge => map_leaf_pte(&self.root, virtual_addr.vpn2()),
            PageType::Large => {
                map_indirect_pte(&self.root, virtual_addr.vpn2(), virtual_addr.vpn1())
            }
            PageType::Small => {
                let next_table = get_or_create_page_table(&self.root, virtual_addr.vpn2())?;
                map_indirect_pte(next_table, virtual_addr.vpn1(), virtual_addr.vpn0())
            }
        }
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
