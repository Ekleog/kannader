# Configuration Format

Kannader's distinctive feature is its configuration format. The
objective is to have the configuration be as flexible and fast as
could be, yet be easily usable for users with simple needs.

Fear ye not! These two objectives are not contradictory. The idea for
a solution handling these two use cases is to first build a flexible
and fast configuration format. Once this is done, it then becomes
possible to write wrappers around this configuration format that
expose its flexibility in an easy-to-use format.

## Flexibility

The first and foremost property we need for kannader's configuration
format is flexibility. It is flexibility that will allow kannader to
be usable in most contexts, and it is the one that will be built on to
provide easy-to-use configuration formats.

As a consequence, it is the one on which no compromise is possible.

For maximal flexibility, kannader is designed as a set of
libraries. However, libraries are not what a system administrator
wants to have to manage. As a consequence, a more configuration-y
configuration format is required.

This configuration format consists, basically, in hooks to the
behavior of kannader. Every place a hook could be used for doing
something reasonably useful, a hook should be available.

## Performance

Once we have hooks everywhere, a problem quickly arises:
performance. If hooks are used everywhere, then hooks *must* be
performant. Would that not be the case, the whole server would be
slowed down.

Performant hooks are usually written in Lua. This is what was
considered in the first design of kannader's configuration format.

However, Lua has some big issues: the only good implementations that
exist are all written in C, and running such code unsandboxed would go
against kannader's safety motto. It would be surprising if not a
single security flaw was present in Lua[^security]. Another major
issue of Lua is its being exotic. While other programs, like rspamd or
nginx, already use or allow Lua scripting, system administrators
usually are not fluent in Lua.

This led to searching for another option. Thus arose the idea of
sandboxing a Lua VM: it is possible to run any kind of interpreter
from within a sandbox. And a sandbox mechanism that recently gained
quite a lot of traction is WebAssembly (thereafter abbreviated wasm).

Wasm is *fast*. Wasm provides a sandbox. Wasm interpreters are
designed for safety, as they run untrusted code found anywhere on the
Internet.

Wasm thus is the choice of predilection for writing the hooks: it's
extremely fast, yet allows for near-perfect configurability.

## Ease of use

The one thing that wasm gets really, really wrong, among our
requirements, is ease of use. Yet, it is maybe one of our most
important objectives with kannader's configuration format: to provide
a configurable, *yet easy* configuration format.

This can thankfully be worked around, by using “wrappers.” These take
the configurability of the wasm hooks, and turn it into an easy-to-use
format.

These “wrappers” are, in fact, only pre-compiled wasm blobs that are
provided alongside kannader. They provide configuration formats that
vary in ease-of-use and configurability, allowing to adjust the knob
between the two.

Kannader, being passed a wasm configuration blob at startup (as well
as a list of which local and remote resources should be made available
to the sandbox), can thus simply provide pre-built wasm configuration
blobs that then parse and enforce a specific configuration format.

## Configuration blob examples

An example of what such configuration blobs could do would be to read
a format that mimicks OpenSMTPD's configuration format, and then
translates it into the appropriate kannader hooks, for easy migration.
This takes a “regular” previously-existing SMTP server's configuration
file, and turns it into a kannader configuration.

But more interesting things can be done with this scheme. For
instance, it is possible to have the wasm configuration blob read a
Lua file, and then run it through a Lua interpreter, thus making the
total flexibility available to the end-user like direct Lua hooks
would have done, to the expense of some performance by running the Lua
VM[^overhead].

Or, another wasm configuration blob could read a Python file, and run
it through MicroPython[^python]. This provides a language that will
probably be more familiar to the sysadmin, yet usable for most
non-maximal-performance use cases.

But it is possible to do more than just have configuration blobs that
read a generic configuration file and converts it into kannader hooks.
It is also possible to have the choice of the configuration blob
itself *be* part of the configuration. For instance, a configuration
blob could handle the “local server with local users only” use case,
that would setup authentication for all sending that's not directed
towards local users (using callbacks provided by eg. MicroPython code
for things like knowing whether a user is local), antispam and
antivirus if configured to do so, and maybe even automatically
validate the `From:` header if configured to.

This is a case of encoding domain-specific knowledge in the
configuration blob, in order to make it much easier to write. It
includes almost no flexibility, but should make the configuration as
simple to write as possible.

[^security]: Or in the configuration Lua code, vulnerability that the
attacker could then exploit over the network, with this issue being
more likely the less the configuration code is sandboxed.

[^overhead]: The wasm sandbox's overhead is probably lower than
LuaJIT's sandbox's overhead, while providing much better security,
which makes it a tool of choice for flexibility and performance.
However, the fact that LuaJIT is most likely very far from being
available on wasm does mean that only a non-JIT version of a Lua VM
can be run on top of wasm… which implies that Lua interpretation will
be much slower. However, this should hopefully not be a problem, as
for all non-intensive use cases the time spent in hooks will probably
not be a concern, and intensive use cases whose use case is not
covered by an existing configuration blob will probably write and
compile their own.

[^python]: While it may appear surprising to suggest MicroPython
insted of a regular CPython instance, this is based on the fact that
kannader needs to have one interpreter instance per message in flight,
to make sure there is no interference between two messages. As such,
the CPython resident memory size would probably be prohibitive for use
cases that see a high number of emails flow through. Use cases that
only handle few emails would probably work well with CPython, but, the
differences between CPython and MicroPython not being that important
for something that after all is nothing but a configuration format,
MicroPython will probably be a better choice.
