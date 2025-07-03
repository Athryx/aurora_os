# Thread
Thread of execution

# ThreadGroup
A group of threads or child thread groups
Thread groups form a tree starting from the root thread group

# CapabilitySpace
A table mapping capability IDs to the corresponding capability
Only a thread can hold a CapabilitySpace with a strong reference to prevent reference cycle

# AddressRegion
Region of address space which can be mapped and handle page faults.

Subregion can be created from existing region
Subregions cannot overlap
Pagefault is handled in the smallest containing subregion

### Mapping
Individual pages or page groups can be mapped, which results in address space taking strong reference to underlying page.

Anonomous regions can be mapped, which don't 

# Page

# PageGroup

# Lock

# EventPool

# IpcAllocator

# IpcAllocCapacity

# DataLayout

# Channel

# Reply

# DropCheck

# DropCheckReceiver

# Key

# Allocator

# MmioAllocator

# RootOom

# IntAllocator

# Interrupt

