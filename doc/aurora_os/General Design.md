# Objects

universal serialization fromat for objects

objects composed of data and kernel capabilities

## Capabilities

Objects are referenced by a capability

### Kernel Capabilties

all kerne objects are accessed by an capability id, and the kernel object has read, write and prod permssions flags

some methods are only allowed if a certain flag is set

### User Capabilties

many userspace servers will use a similar design pattern
userspace objects are composite objects in a serialized form,
but the userspace server will still keep track of cpabilties for that object

# Namespace

each process has its own namespace

namespace: mapping from string to object

namespace lives in process, it is set up when spawned by the parent process

# Capability Groups

For some resources (such as files) it is not possible to say who owns it, because it can not be saved on disk every single process that has a given capability for the file

The solution is a capability group subsystem

A process can have a capability to a group, and send this group capability to the filesystem server, and the filesystem server can get the group id and proof of ownership, and use access control list in the filesystem which stores the group id

The filesystem should be sent a version of the capability that allows it to verify ownership, but not too claim ownership anywhere else

## Launching Apps from a Different Capability Group

The init system has all the capabilties and all the capability groups
(NOTE: since this has all capability groups, this may end up being the same as the user system)

Call into the process and tell it to launch a service (assuming you have capabilties to do this)
each service binary will be in its own directory with a file detailing what capability group it is a part of and other capabilties it should have

the init process will spawn the given service accoring to these rules
the service permission file will also specify which fields in it's namespace can be set by the unprivilidged process