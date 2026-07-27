use core::{fmt, ops::Range};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PageType {
    Small,
    Large,
    Huge,
}

impl PageType {
    pub const fn size(self) -> usize {
        match self {
            PageType::Small => 4096,
            PageType::Large => 2 * 1024 * 1024,
            PageType::Huge => 1024 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct PhysicalAddr(u64);

impl PhysicalAddr {
    pub const fn new(addr: u64) -> Self {
        Self(addr)
    }

    pub fn get(self) -> u64 {
        self.0
    }

    pub fn ppn(self) -> u64 {
        self.0 >> 12
    }

    pub fn page(self, page_type: PageType) -> AddressRange<Self> {
        AddressRange::new(self, Self(self.0 + page_type.size() as u64))
    }

    pub fn identity_mapped_virtual(self) -> VirtualAddr {
        VirtualAddr::new(self.0 as _)
    }

    pub fn is_aligned(self, page_type: PageType) -> bool {
        self.0.is_multiple_of(page_type.size() as _)
    }

    pub fn offset(self, by: isize) -> Self {
        Self::new(self.0.strict_add_signed(by as _))
    }
}

impl fmt::Debug for PhysicalAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("PhysicalAddr")
            .field_with(|f| write!(f, "{:#010x}", self.0))
            .finish()
    }
}

impl fmt::Display for PhysicalAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#010x}", self.0)
    }
}

impl From<PhysicalAddr> for usize {
    fn from(value: PhysicalAddr) -> Self {
        value.get() as _
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct VirtualAddr(usize);

impl VirtualAddr {
    // TODO: add virtual address validation (canonical lower or upper half)
    pub const fn new(addr: usize) -> Self {
        Self(addr)
    }

    pub fn get(self) -> usize {
        self.0
    }

    pub fn vpn0(self) -> usize {
        (self.0 >> 12) & 0x1ff
    }

    pub fn vpn1(self) -> usize {
        (self.0 >> 21) & 0x1ff
    }

    pub fn vpn2(self) -> usize {
        (self.0 >> 30) & 0x1ff
    }

    pub fn identity_mapped_physical(self) -> PhysicalAddr {
        PhysicalAddr::new(self.0 as _)
    }

    pub fn is_aligned(self, page_type: PageType) -> bool {
        self.0.is_multiple_of(page_type.size())
    }

    pub fn offset(self, by: isize) -> Self {
        Self::new(self.0.strict_add_signed(by))
    }
}

impl fmt::Debug for VirtualAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("VirtualAddr")
            .field_with(|f| write!(f, "{:#016x}", self.0))
            .finish()
    }
}

impl<T> From<*const T> for VirtualAddr {
    fn from(value: *const T) -> Self {
        Self::new(value.addr())
    }
}

impl From<usize> for VirtualAddr {
    fn from(value: usize) -> Self {
        Self::new(value)
    }
}

impl fmt::Display for VirtualAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#016x}", self.0)
    }
}

impl From<VirtualAddr> for usize {
    fn from(value: VirtualAddr) -> Self {
        value.get()
    }
}

#[derive(Debug, Copy, Clone)]
pub struct AddressRange<T> {
    pub start: T,
    pub end: T,
}

impl<T> AddressRange<T> {
    /// Creates a new address range with addresses `[from, until)`.
    ///
    /// `start` is the first address in the range.
    /// `end` is the first address outside the range.
    pub fn new(start: T, end: T) -> Self {
        Self { start, end }
    }
}

impl AddressRange<PhysicalAddr> {
    pub fn identity_mapped_virtual(&self) -> AddressRange<VirtualAddr> {
        (self.start.identity_mapped_virtual()..self.end.identity_mapped_virtual()).into()
    }
}

impl AddressRange<VirtualAddr> {
    pub fn identity_mapped_physical(&self) -> AddressRange<PhysicalAddr> {
        (self.start.identity_mapped_physical()..self.end.identity_mapped_physical()).into()
    }
}

impl<T: Copy + Into<usize>> AddressRange<T> {
    pub fn size(&self) -> usize {
        self.end.into().saturating_sub(self.start.into())
    }
}

impl<T> AddressRange<T>
where
    T: Copy + From<usize> + Into<usize>,
{
    pub fn with_aligned_end(self, page_type: PageType) -> Self {
        Self::new(
            self.start,
            self.end.into().next_multiple_of(page_type.size()).into(),
        )
    }
}

impl<T: fmt::Display> fmt::Display for AddressRange<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

impl<T: Into<VirtualAddr>> From<Range<T>> for AddressRange<VirtualAddr> {
    fn from(range: Range<T>) -> Self {
        Self::new(range.start.into(), range.end.into())
    }
}

impl<T: Into<PhysicalAddr>> From<Range<T>> for AddressRange<PhysicalAddr> {
    fn from(range: Range<T>) -> Self {
        Self::new(range.start.into(), range.end.into())
    }
}
