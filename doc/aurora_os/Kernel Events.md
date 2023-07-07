# Planning

### Possible Ways to Handle Event
- Synchronous
	- 1 thread can block at a time
	- many threads can block at a time
	- queue of threads waiting for event
- Asynchronous
	- received into fixed size event pool
	- received into variable size event pool

### Event Sources and Prefered Handling Methods
- Process events (invalid instruction, segfault, etc)
	- normal errors (invalid instruction, page fault, etc)
		- single listener
		- should be handled by fixed size event pool
	- breakpoint exception
		- multiple listener, maybe single listener
		- same as regular events, but need some way to stop debugged process overriding this
		- probably some sort of syscall filtering is ideal way, or many listeners
		- also need to make a way to make these events invisible in a process so it can't know its being debugged (ie malware)
- Channel
	- Send complete
		- queue of threads or variable sized event pools
	- Receive complete
		- queue of threads or variable sized event pools
- Lock status change
	- listener wait queue is needed
- Interrupts
	- single listener
	- fixed size event pool
- Event pool event received
	- many listeners
	- many threads blocking at a time
- Allocator out of memory
	- many listeners
	- 1 listener at a time is probably ok, but many listeners is desired
		- so 1 process can't override oom listener and stop other processess on same allocator getting oom event
- Root oom
	- This does not use the normal event api

# Determined Event Design
2 types of events:
- broadcast
	- many listeners which are all notified when event occurs
	- thread listeners must make syscall to listen block, they are unblocked when event occurs
	- thread pools register to listen to event in either 1 shot or persistent mode
- queue
	- queue of listeners, with each event being dispatched to the listener at the head of the queue
	- listener is removed from the head of the queue when event recieved
	- syscall is used to register thread or event pool to enter queue
		- if thread is listening it is blocked until event is received
		- threads and event pools are in the same queue
		- event pools can be configured to automatically reenter the queue at the end without requiring another syscall

### Event Source Types
- Process events (invalid instruction, segfault, breakpoint, etc)
	- broadcast event
- Channel
	- queue event (send and receive)
		- synchronous receive must specify a buffer and max size to receive into
		- asynchronous receive can specify a max receive size
- Lock status change
	- queue event
- Interrupts
	- broadcast event
- Event pool event received
	- broadcast event
- Allocator out of memory
	- broadcast event
- Root oom
	- This does not use the normal event api

### Event Pool
