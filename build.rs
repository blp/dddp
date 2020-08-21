extern crate protoc_grpcio;

fn main() {
    let protos = [
        "p4/v1/p4runtime.proto",
        "p4/v1/p4data.proto",
        "p4/config/v1/p4info.proto",
        "p4/config/v1/p4types.proto",
        "google/rpc/status.proto",
        "google/rpc/code.proto",
    ];
    for proto in &protos {
        println!("cargo:rerun-if-changed={}", proto);
    }
    protoc_grpcio::compile_grpc_protos(
        &protos,
        &["p4runtime/proto", "googleapis"],
        "src/proto",
        None,
    )
    .expect("Failed to compile gRPC definitions!");
}
