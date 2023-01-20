use modular_bitfield::BitfieldSpecifier;

#[derive(Debug, Clone, Copy, BitfieldSpecifier)]
#[bits = 3]
pub enum DelivMode {
	Fixed = 0,
	// only available for io apic and ipi
	// avoid for ipi
	LowPrio = 1,
	// avoid for ipi
	Smi = 2,
	Nmi = 4,
	Init = 5,
	// only available for ipi
	Sipi = 6,
	ExtInt = 7,
}

#[derive(Debug, Clone, Copy, BitfieldSpecifier)]
pub enum DestMode {
	Physical = 0,
	Logical = 1,
}

#[derive(Debug, Clone, Copy, BitfieldSpecifier)]
pub enum DelivStatus {
	Idle = 0,
	Pending = 1,
}

#[derive(Debug, Clone, Copy, BitfieldSpecifier)]
pub enum TriggerMode {
	Edge = 0,
	// avoid for ipi
	Level = 1,
}

// Default for when acpi tables say use default
impl Default for TriggerMode {
	fn default() -> Self {
		Self::Edge
	}
}

#[derive(Debug, Clone, Copy, BitfieldSpecifier)]
pub enum PinPolarity {
	ActiveHigh = 0,
	ActiveLow = 1,
}

// Default for when acpi tables say use default
impl Default for PinPolarity {
	fn default() -> Self {
		Self::ActiveHigh
	}
}

#[derive(Debug, Clone, Copy, BitfieldSpecifier)]
pub enum RemoteIrr {
	None = 0,
	Servicing = 1,
}