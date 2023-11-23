Workspace containing all the crates used for userspace.

A few are also used in kernel, but kernel cannot be part of this workspace since it is built with a different target.

There is a symlink to the target file in every crate to fix an issue with rust analyzer not being able to find the target.
