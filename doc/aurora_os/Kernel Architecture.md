New kernel design

# Memory

### Page Allocator
LLFree lockless page allocator

Start off with metadata just fixed size array in binary
change it to be mapped from pages and be dynamically sized

Maybe array of pages which store metadata about each page similar to linux?

### Heap Allocator
Probably lockless bitmap implemntation
Chunk which stores objects of same size allocated from page allocator

Bitmap and object count stored at start of chunk

Possible security mitigations:
- 1 object type per cache
	- Microkernel won't have to many distinct object types
- System similar to CONFIG_KMALLOC_VIRTUAL
	- ensures cross cache attack not possible, so overlapping objects with another different object type is never possible

### Block Management
For each cache with each cache size:
- `local_blocks`: 1 linked list of blocks per cpu
- `partial_blocks`: 1 global doubly linked list
- `empty_blocks`: 1 global doubly linked list

Parameters:
- `partial_repopulate_count`
- `empty_repopulate_count`
- `empty_free_threshold`
- `empty_min_amount`
- `local_evict_threshhold`
- `local_min_amount`

### Allocation
If `local_blocks` empty:
- First try to take `partial_repopulate_count` blocks from `partial_blocks`
- If `partial_blocks` empty, try to take `empty_repopulate_count` blocks from `empty_blocks`
	- If less then `empty_repopulate_count` blocks are stolen, allocate remaining blocks from page allocator
- If both empty, allocate `empty_repopulate_count` new blocks from page allocator

 Then allocate object from head of `local_blocks` list
 If block is now full, remove it from `local_blocks` list and unpin it
### Deallocation
Mark bit as free and increase block count.
If block is pinned no further actions will be taken.

If block goes from fully allocated to partially allocated:
- Atomic decrament also set pin bit
- Add it on front of `local_blocks` list
- If length of `local_list` > `local_evict_threshold`
	- Remove tail of list excluding first `local_min_amount` blocks to `partial_blocks` and `empty_blocks`, also unpin all blocks
		- Use 2 intermediate lists, so locks are held for very short amount of time
		- Issue: different CPU could free object on a partial block and now it becoms free after checking

If block goes from partially allocated to empty:
- Move from `partial_blocks` to front of `empty_blocks`
- If length of `empty_blocks` > `empty_free_threshold`
	- Free all blocks in `empty_list` until there is only `empty_min_amount` blocks left
	- Probably free from back of empty list

### Synchronizing Global Double Linked Lists
Easy solution is spinlock
Probably ok for inserting one at a time when freed

Prefer LinkedList and Trees over HashMap and Vec, it will work well with allocator
There is really no place in kernel where Vec will give better performance except some places in old design use a vec with each index for per cpu data

Instead should just directly embed these structures in the per cpu data

Vec may be more conveniant for some things with starting initial userspce process
Easy solution is just put a simple allocator on top of a chunk of memoryu from page allocator and use vec just for that

### Virtual Layout