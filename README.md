# P4++ skeleton

There's not anything useful here yet.  It's skeletal.  Here are
instructions, anyway.

## Build dddp

1. Get the submodules:

```
git submodule update --init
```

2. Install the protobuf compiler:

```
$ cargo install protobuf-codegen
```

3. Install the gRPC compiler:

```
$ cargo install grpcio-compiler
```

4. Build:

```
cargo build
```

## Build bmv2 simple_switch_grpc

I had to apply the following patch before it worked for me properly.
The ``Makefile.am`` change was necessary to make the binary link.  The
``main.cpp`` change was necessary to keep grpc startup from hanging
the binary in an infinite loop (this is a grpc bug that is fixed in
new-enough grpc, so possibly you won't have it).

```
diff --git a/targets/simple_switch_grpc/Makefile.am b/targets/simple_switch_grpc/Makefile.am
index 1a8510cc4c93..01e53cdcf959 100644
--- a/targets/simple_switch_grpc/Makefile.am
+++ b/targets/simple_switch_grpc/Makefile.am
@@ -22,7 +22,7 @@ bin_PROGRAMS = simple_switch_grpc
 simple_switch_grpc_SOURCES = main.cpp
 
 simple_switch_grpc_LDADD = \
-libsimple_switch_grpc.la
+libsimple_switch_grpc.la -lpip4info
 
 # We follow this tutorial to link with grpc++_reflection:
 # https://github.com/grpc/grpc/blob/master/doc/server_reflection_tutorial.md
diff --git a/targets/simple_switch_grpc/main.cpp b/targets/simple_switch_grpc/main.cpp
index ae7f32c7b8bb..05a232a5547f 100644
--- a/targets/simple_switch_grpc/main.cpp
+++ b/targets/simple_switch_grpc/main.cpp
@@ -111,3 +111,9 @@ main(int argc, char* argv[]) {
   runner.wait();
   return 0;
 }
+
+void grpc_tracer_init(const char *)
+{}
+
+void grpc_tracer_init()
+{}
```

## Run

1. Start ``simple_switch_grpc``.  From its build directory:

```
$ ./simple_switch_grpc --no-p4 -- --grpc-server-addr 0.0.0.0:50051 --cpu-port 1010
Calling target program-options parser
Server listening on 0.0.0.0:50051
```

   (If you don't see the second line above then probably grpc startup is
   hanging as mentioned above.)

2. Run the dddp binary.  It sends a capabilities request and receives
   the reply, then it sends a forwarding pipeline reconfiguration
   message and prints the reply.  It doesn't do anything else yet:
   
```
$ target/debug/client 50051 examples/simple_router/simple_router.p4info.bin examples/simple_router/simple_router.json
send  and received p4runtime_api_version: "1.2.0"
```
