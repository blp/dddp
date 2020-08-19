extern crate grpcio;
extern crate proto;
use proto::p4runtime_grpc::P4RuntimeClient;
//use proto::p4runtime::WriteRequest;
use proto::p4runtime::CapabilitiesRequest;

use std::env;
use std::sync::Arc;

use grpcio::{ChannelBuilder, EnvBuilder};

fn main() {
    let args = env::args().collect::<Vec<_>>();
    if args.len() != 2 {
        panic!("Expected exactly one argument, the port to connect to.")
    }
    let port = args[1]
        .parse::<u16>()
        .unwrap_or_else(|_| panic!("{} is not a valid port number", args[1]));

    let env = Arc::new(EnvBuilder::new().build());
    let ch = ChannelBuilder::new(env).connect(format!("localhost:{}", port).as_str());
    let client = P4RuntimeClient::new(ch);

/*
    let mut write_request = WriteRequest::new();
    write_request.set_device_id(123);
    write_request.set_role_id(567);
    let write_response = client.write(&write_request).expect("RPC Failed!");
    println!("Ate {:?} and got back {:?}", write_request, write_response);
     */

    let cap_request = CapabilitiesRequest::new();
    let cap_response = client.capabilities(&cap_request).expect("RPC failed!");
    println!("send {:?} and received {:?}", cap_request, cap_response);
}
