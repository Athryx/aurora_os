
# Requirements

- Get the process of the currently executing thread
- Given a thread id, set the to a specific state if it is not currently running
- If it is running, only the current thread can set its own state (except when process exits, it will be force destroyed if it is currently running)
- Cleanup any resources on the kernel stack when a thread is destroyed to avoid resource leaks (mainly dropping capabilities and arcs)

# Design

## States

- Running: the thread is currently running on a cpu
- Ready: the thread is ready to run
- Destroy: the thread is destroyed, but its resources have not been cleaned up yet
- Suspended: not running until further changes
- Wait on event: thread is waiting for an event to occur
- Wait on timeout: thread is waiting for a time to pass
- A thread can also be waiting on an event and timout

## Structures

```
struct Process {
	alive: AtomicBool,
	threads: Vec<Arc<Thread>>,
	num_threads_running: AtomicUsize,
}

struct Thread {
	process: Weak<Process>,
	wait_capid: AtomicUsize,
	handle: *const ThreadHandle,	
}

enum State {
	Running,
	Ready,
	Destroy {
		// if true, the scheduler will check that this is the 
		// last thread switching away from a dead process,
		// and will destoy the process as well
		try_destroy_process: bool,
	},
	// if for event for either suspend is false,
	// the scheduler will not ehck the capid field on the thread,
	// and will assume it is not waiting for an event to improve performance
	Suspend {
		for_event: bool,
	}
	SuspendTimeout {
		until_nanosecond: u64,
		for_event: bool,
	}
}

struct ThreadHandle {
	state: State
	Arc<Thread>,
}

type EventHandler = Weak<Thread>;

static PROCESS_MAP: HashMap<Cid, StrongCapability<Process>> = HashMap::new();
```

ThreadHandle is used internally by the scheduler and will be in an intrusive linked list or tree.
Evant handler is the type an object that emits events should hold if a thread is waiting on the object's events.

`wait_capid` stores the capability id of the event the thread is currently waiting on, or 0 if it is not wiating on an event

Whenever a thread transitions out of a suspend state with `for_event` set, the scheduler will do a compare exchange with `wait_capid` and set it to 0 and switch the state. If the scheduler finds that it has already been set to 0 (the event has been handled) it knows some other code will move the thread to ready

The cpu local global variables will also store an `Arc<Thread>` and an `Arc<Process>` for the currently running thread and process. The scheduler will switch these out on thread change. 

## Cleanup Resources

Whenever a thread acquires resources that need to be dropped (like decramenting a refcount) it must us an `IntDisable` to disable interrupts before acquiring resources to prevent a process exit from leaking resources. It should then drop the int disable when its done, or pass the int disable to a change running thread function which will enable interrupts just before switching threads

## Process Exit Procedure

First, `is_alive` flag on process is atomically compared and set to false. At this point, no new threads from this process will be scheduled to run. An ipi_exit interrupt will be sent, and any threads currently running from the process will set themselves to the destroy state. Once no more threads are running, except possibly the thread terminat