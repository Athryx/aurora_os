# Implementation Details

## Specific

- improve the kernel heap allocator, it is very bad
- add guard pages at the end of kernel stacks
- update orderings on all atomics to use minimum needed ordering
- fix oom panics that could happen in scheduler
    - possible solution is reserve space in ready map whenever a thread is created
- change to use relative call instructions instead of absolute call with register
    - this option is called code model
    - should set vma to 0xffffffff80000000, and use mcmodel kernel
- use Size type from bit_utils in more places in kernel
- use strum for enum convert from int
- maybe look into enumflags2 for syscall options
- maybe: figure out why disk image is so big?

## General

- put safety comments wherever unsafe is used
- document old code
- run auto formatter
- add tests
- logging


# Api / Overall Design

- improve memory mapping api
    - don't require strong reference
    - don't require cap to be in that same process
    - maybe don't have 1 mapping restriction on resize in place
    - add support to memory mapping syscalls that allow mapping at some offset into the memory capability
    - extend memory_update_mapping to support moving the base address and changing mapping flags
    - maybe merge memory_resize and memory_update_mapping syscalls
- seperate process into its different components
    - address space
    - capability space
    - group of threads (or maybe just make thread a capability?)
- figure out if weak capabilities are even needed
    - their original intent was so you can send your capability to another process and still have control over when its dropped
    - probably just remove weak auto destroy though, and make destroying invalid weaks the default behavior
- clean up ownership of capabilities in userspace
    - it can be hard to reason about
    - this is just changing sys library, not the kernel