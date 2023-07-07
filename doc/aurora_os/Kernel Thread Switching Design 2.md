# States and Transitions

- Running
	- Running -> Ready: timeslice expired, thread_yield
	- Running -> Suspended: thread_suspend, sleep, wait event
	- Running -> Dead: process exit, thread exit
		- All of these transitions are the running thread moving itself to a new state
- Ready
	- Ready -> Running: thread picked by scheduler
	- Ready -> Dead: process exit
		- All of these transitions are the scheduler picking a thread from the thread list and moving it to a new state
- Suspended
	- Suspended -> Ready: event, thread_resume, timeout
		- Event and timeout use a generation reference to the thread, so it becomes invalid as soon as the thread switches to another state
		- Thread resume atomically sets the status bits to ready and incraments generation
		- TODO: figure out a way to get rid of old thread refs so they don't keep underlying thread memory in use (the thread will be dropped but a weak reference will remain)
	- Suspended -> Dead: process exit
		- Atomically set status bits adn incrament generation
- Dead

## Process Exit Procedure

Set is_alive to false, and check it used to be true so only 1 thread terminates the process
release strong reference
send IPI_PROCESS_EXIT to all other cpus
exit current thread if the process is the current process

only process will hold a strong reference to its threads, so process being dropped will drop all its threads, including suspended ones

## Thread Status

bits 0 and 1: 2 bit status of either Running, Ready, Suspended, or Dead
other bits: current thread generation, used to invalidate thread refs that should not be used