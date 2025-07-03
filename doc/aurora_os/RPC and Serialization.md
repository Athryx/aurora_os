RPC will go over Channels (possibly support for RPC over shared memory as well?)

# Serialization
Ideally zero copy deserialization

Existing rust ones:
- rkyv (rust specific)
- cap n' proto (full RPC framework and IDL, like protobuf)

Using custom one better for flexibility
Zero copy means non self describing, but if self describing needed serialized interface could be sent out of band to data

Integers, floats, tuples, structs, fixed size arrays can just be sent as is with repr(C)
But it is perhaps better to explicitly position elements, for backwards comparability

### Kernel RPC Serialization
When sending a message, pass metadata describing shape of data to be sent

This metadata is an array of fixup steps, which specify an offset into the buffer to perform some action on
Fixup steps must be ordered by offset
Offset is 8 byte aligned, since words are copied 1 at a time

Size of object is also stored along with fixup array

By default, word is just copied
- Capability fixup interprets high bits as permission mask and rest of bits as capability id to send, so capability can be sent with equal or reduced permissions
- Pointer fixup performs scatter gather IO
	- It can also point to child fixup array, if it is an array of elments which have a type which also needs fixup
	- Offset should be to 2 8 byte values, one is data pointer one is byte length
		- Pointer must be 8 byte aligned
	- Have a different fixup for non array pointer, where it points to just one object, so no length field is needed

For start, mayb maximum number of fixups so fixups can be processed entirely in fixed size stack buffers

Fixup data also copied to end of message in event pool, so receiver knows the type of data
- TODO: maybe faster method for verifying?

#### Received Object Allocator

Probably don't copy object directly to event pool, but instead to memory mapped allocator for objects
- Enables freeing of objects at a later time, since event pool memory is invalidated the next time you wait for an event

Design options:
- Atomic bitmap which is mapped into userspace, can be edited to free objects
- Read only mapping with memory allocator metadata in the mapped region
- Memory allocator metadata stored out of line
	- This is probably preferable option?

Todo: capability for limiting amount of messages you can spam and take up space in another process
Probably IpcAllocator free syscall will decrament this value

### Serialization Format
Integers, floats, structs, fixed size arrays can just be sent as is with repr(C)
But it is perhaps better to explicitly position elements, for backwards compatability

Tuples are not allowed, as they are not representable in deterministic way with repr C (https://doc.rust-lang.org/nomicon/other-reprs.html)

Fieldless enums are made to be repr(N), and are checked to be a valid value during verify deserialize check

Enums with fields are also repr C with checked discriminant

Array and trees can be sent with scatter gather

Maps are just a VecMap
- Might want to add support to send hashmap
	- Can already be done, but empty slots are sent
	- Perhaps way to filter out empty slots?
	- fixup metadata could specify byte offset which if it is a particular offset it is not copied
		- Copy length is also adjusted accordingly

#### Userspace Serialization
When derive serialize on a struct, another variant of type is generated for zero copy view of serialized bytes.
Pointers in this view are offsets from start of buffer.
This view will be borrowed from slice.
In addition, slice can be deserialized to regular value.
Depth first deserialize, so root object at back.

Note that Channel RPC calls will not use this since they have absolute pointers, due to kernel translating pointers.

So for example this example:
```rust
#[derive(Aser)]
struct Foo {
	a: usize,
	b: OsString,
}
```

Will generate the following:
- Code to serialize Foo as bytes (with copy)
- Code to deserialize bytes to Foo (with copy)
- `FooView` to use bytes as foo in zero copy way
- Code to convert slice to `FooView` in zero copy way
- Description of needed fixups for aurore kernel RPC with channel
- Out of line metadata describing layout of struct

### RPC Interfaces
Endpoint represented by channel


### Design Decisions TODO
- IDL vs rust struct with derive macro