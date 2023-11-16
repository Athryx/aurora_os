# Bugs

# Implementation Details

## Specific

- drop implementation of thread group should remove itself from parent
    - also try to make it terminate current thread immediately if needed
- add options to call create syscalls to specify created object capflags
- improve the kernel heap allocator, it is very bad
- add guard pages at the end of kernel stacks
- update orderings on all atomics to use minimum needed ordering
- fix oom panics that could happen in scheduler
    - possible solution is reserve space in ready map whenever a thread is created
- change to use relative call instructions instead of absolute call with register
    - this option is called code model
    - should set vma to 0xffffffff80000000, and use mcmodel kernel
- use Size type from bit_utils in more places in kernel
    - also add a PageSize type which is guarenteed to be page aligned size
        - also add a PageSize type which is guarenteed to be page aligned size
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
    - maybe don't have 1 mapping restriction on resize in place
    - add support to memory mapping syscalls that allow mapping at some offset into the memory capability
    - extend memory_update_mapping to support moving the base address and changing mapping flags
    - maybe merge memory_resize and memory_update_mapping syscalls
- redisign memory system
    - copy on write
    - figure out how dirty page tracking is communcated to userspace
    - figure how paging to disc will communicate to userspace
- figure out if weak capabilities are even needed
    - their original intent was so you can send your capability to another process and still have control over when its dropped
    - probably just remove weak auto destroy though, and make destroying invalid weaks the default behavior
- clean up handling of weak capabilities in userspace
- Add syscalls to remove event pools from listening to an event
- Add zero copy channel sends
    - raw pages will just be remapped if reciever specifies page aligned address and size, and sender specifies page aligned address and special flag
        - the reason for using flag to enable it is so sender can still specify actual copy size not page aligned, so if reciever buffer is not page aligned extra bytes don't need to be copied
    - add another type of event pool or something for page aligned data, so pages can just be pushed into this event pool
        - seperate from normal event pool so small writes don't get interspersed and cause issues with wasted space due to alignmant needs
- Get rid of int disable everywhere
    - 2 options
        - get rid of IPI_PROCESS_EXIT and have syscalls check if they should switch processess only when returning
        - make kernel entirely non preemptable, switch to 1 stack per cpu
