# Filesytem Server
Control api that allows creating new connections with a given capability group
the control api is only used by the app launching server

Unlike unix, file paths are not directly opened to get a stream (altough there is a shorthand function for this)
Instead, first a path is acquired to give a file object, and then the file object can be opened as a stream. The acquire also performs the same role as stat. The reason for this seperation is to prevent race condition from checking if file exists and its access permissions and opening the file

Each connection to the filesystem server supports unveil

# System Server
in charge of initializing system

manages capability groups

used to start other services

