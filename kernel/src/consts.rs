//! Constants about kernel executable sections and other related memory constants from the linker

use crate::prelude::*;

pub use bit_utils::PAGE_SIZE;

extern "C" {
    // virtual address that physical memory is offset by (includes 1 extra megabyte) (does include lower half of kernel)
    static __KERNEL_VMA: usize;
    // physical address kernel resides at (does not include 1 extra megabyte) (does include lower half of kernel)
    static __KERNEL_LMA: usize;
    static __AP_PHYS_START: usize;
    static __AP_CODE_START: usize;
    static __AP_CODE_END: usize;
    static ap_data: usize;
    static __TEXT_START: usize;
    static __TEXT_END: usize;
    static __RODATA_START: usize;
    static __RODATA_END: usize;
    static __DATA_START: usize;
    static __DATA_END: usize;
    static __BSS_START: usize;
    static __BSS_END: usize;
    // virtual address that kernal starts at (does not include 1 extra megabyte) (does include lower half of kernel)
    static __KERNEL_START: usize;
    // virtual address that kernel ends at
    static __KERNEL_END: usize;
    static stack_bottom: usize;
    static stack_top: usize;
    static PDP_table: usize;
    static asm_user_copy: usize;
    static asm_user_copy_end: usize;
}

lazy_static! {
    /// This is the address that the kernel address range starts at
    /// Physical address 0 is mapped starting at this address in kernel memory
    pub static ref KERNEL_VMA: usize = unsafe { &__KERNEL_VMA } as *const _ as usize;
    /// This is the physical address the kernel resides at in memory
    pub static ref KERNEL_LMA: usize = unsafe { &__KERNEL_LMA } as *const _ as usize;

    /// This is the address the ap trampoline is compiled in the kernel at
    pub static ref AP_CODE_PHYS_START: usize = unsafe { &__AP_PHYS_START } as *const _ as usize;
    /// This is the address the ap trampoline is copied to to run
    pub static ref AP_CODE_RUN_START: usize = unsafe { &__AP_CODE_START } as *const _ as usize;
    pub static ref AP_CODE_RUN_END: usize = unsafe { &__AP_CODE_END } as *const _ as usize;
    /// This is the physical address ap data will be at when the tampoline is running
    pub static ref AP_DATA: usize = unsafe { &ap_data } as *const _ as usize;
    
    pub static ref AP_CODE_SIZE: usize = *AP_CODE_RUN_END - *AP_CODE_RUN_START;

    pub static ref AP_CODE_SRC_RANGE: APhysRange = APhysRange::new_aligned(
        PhysAddr::new(*AP_CODE_PHYS_START),
        *AP_CODE_SIZE,
    );
    // the physical memory range that the code zone will be copied to
    pub static ref AP_CODE_DEST_RANGE: APhysRange = APhysRange::new_aligned(
        PhysAddr::new(*AP_CODE_RUN_START),
        *AP_CODE_SIZE,
    );

    pub static ref TEXT_START: usize = unsafe { &__TEXT_START } as *const _ as usize;
    pub static ref TEXT_END: usize = unsafe { &__TEXT_END } as *const _ as usize;
    pub static ref TEXT_VIRT_RANGE: AVirtRange = AVirtRange::new_aligned(
        VirtAddr::new(*TEXT_START),
        *TEXT_END - *TEXT_START,
    );

    pub static ref RODATA_START: usize = unsafe { &__RODATA_START } as *const _ as usize;
    pub static ref RODATA_END: usize = unsafe { &__RODATA_END } as *const _ as usize;
    pub static ref RODATA_VIRT_RANGE: AVirtRange = AVirtRange::new_aligned(
        VirtAddr::new(*RODATA_START),
        *RODATA_END - *RODATA_START,
    );

    pub static ref DATA_START: usize = unsafe { &__DATA_START } as *const _ as usize;
    pub static ref DATA_END: usize = unsafe { &__DATA_END } as *const _ as usize;

    pub static ref BSS_START: usize = unsafe { &__BSS_START } as *const _ as usize;
    pub static ref BSS_END: usize = unsafe { &__BSS_END } as *const _ as usize;

    pub static ref KERNEL_START: usize = unsafe { &__KERNEL_START } as *const _ as usize;
    pub static ref KERNEL_END: usize = unsafe { &__KERNEL_END } as *const _ as usize;

    pub static ref KERNEL_PHYS_RANGE: APhysRange = APhysRange::new_aligned(
        PhysAddr::new(*KERNEL_LMA),
        *KERNEL_END - *KERNEL_START,
    );
    pub static ref KERNEL_VIRT_RANGE: AVirtRange = AVirtRange::new_aligned(
        VirtAddr::new(*KERNEL_START),
        *KERNEL_END - *KERNEL_START,
    );

    pub static ref INIT_STACK: AVirtRange = AVirtRange::new_aligned(
        PhysAddr::new(unsafe { &stack_bottom } as *const _ as usize).to_virt(),
        (unsafe { &stack_top } as *const _ as usize)
            - (unsafe { &stack_bottom } as *const _ as usize)
    );

    pub static ref KZONE_PAGE_TABLE_POINTER: PhysAddr =
        PhysAddr::new(unsafe { &PDP_table } as *const _ as usize);
    
    /// This is the region of code that could page fault if userspace gives us bad pointer when calling syscall
    /// Page fault handler will use this to determine if it is real kernel page fault of page fault from userspace
    pub static ref ASM_USER_COPY_CODE_REGION: UVirtRange = UVirtRange::new(
        VirtAddr::new(unsafe { &asm_user_copy } as *const _ as usize),
        (unsafe { &asm_user_copy_end } as *const _ as usize)
            - (unsafe { &asm_user_copy } as *const _ as usize),
    );
}
