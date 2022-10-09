initSidebarItems({"fn":[["allow_uninitialized","Allow logs files to be created from threads in which the logger is specifically uninitialized. It can be useful when you don’t have control on threads spawned by a dependency, for instance."],["initialize","Initializes the current process/thread with a logger, parsing the RUST_LOG environment variables to set the logging level filter and/or directives to set a filter by module name, following the usual env_logger conventions."],["initialize_with_formatter","Initializes the current process/thread with a logger, parsing the RUST_LOG environment variables to set the logging level filter and/or directives to set a filter by module name, following the usual env_logger conventions. The format function specifies the format in which the logs will be printed."]],"type":[["FormatFn","Format function to print logs in a custom format."]]});