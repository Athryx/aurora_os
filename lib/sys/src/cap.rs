use bitflags::bitflags;
use bit_utils::get_bits;

bitflags! {
    pub struct CapFlags: usize {
        const READ = 1;
        const PROD = 1 << 1;
        const WRITE = 1 << 2;
        const UPGRADE = 1 << 3;
    }
}

#[repr(usize)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapType {
    Process = 1,
    Memory = 2,
    Lock = 3,
    EventPool = 4,
    Channel = 5,
    Key = 6,
    Interrupt = 7,
    Port = 8,
    Spawner = 9,
    Allocator = 10,
    RootOom = 11,
    MmioAllocator = 12,
    IntAllocator = 13,
    PortAllocator = 14,
}

impl CapType {
    pub fn from(n: usize) -> Option<Self> {
        Some(match n {
            1 => Self::Process,
            2 => Self::Memory,
            3 => Self::Lock,
            4 => Self::EventPool,
            5 => Self::Channel,
            6 => Self::Key,
            7 => Self::Interrupt,
            8 => Self::Port,
            9 => Self::Spawner,
            10 => Self::Allocator,
            11 => Self::RootOom,
            12 => Self::MmioAllocator,
            13 => Self::IntAllocator,
            14 => Self::PortAllocator,
            _ => return None,
        })
    }

    pub fn as_usize(&self) -> usize {
        *self as usize
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CapId(usize);

impl CapId {
    pub fn try_from(n: usize) -> Option<Self> {
        // fail if invalid type of cap object
        let bits = get_bits(n, 5..9);
        if bits == 0 || bits > 14 {
            None
        } else {
            Some(CapId(n))
        }
    }

    /// Creates a valid CapId from the given `cap_type`, `flags`, `is_weak`, and `base_id`
    /// 
    /// `base_id` should be a unique integer in order for this id to be unique
    pub fn new(cap_type: CapType, flags: CapFlags, is_weak: bool, base_id: usize) -> Self {
        CapId(flags.bits | ((is_weak as usize) << 4) | (cap_type.as_usize() << 5) | (base_id << 9))
    }

    pub fn flags(&self) -> CapFlags {
        CapFlags::from_bits_truncate(self.0)
    }

    pub fn is_weak(&self) -> bool {
        get_bits(self.0, 4..5) == 1
    }

    pub fn cap_type(&self) -> CapType {
        // panic safety: CapId will always have valid metadata, this is checked in the constructor
        CapType::from(get_bits(self.0, 5..9)).unwrap()
    }
}

impl From<CapId> for usize {
    fn from(cap_id: CapId) -> Self {
        cap_id.0
    }
}