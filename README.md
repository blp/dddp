# Build

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
