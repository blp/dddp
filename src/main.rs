extern crate grpcio;
extern crate proto;
extern crate protobuf;

use proto::p4runtime_grpc::P4RuntimeClient;
use proto::p4runtime::CapabilitiesRequest;
use proto::p4runtime::GetForwardingPipelineConfigRequest;
use proto::p4runtime::ForwardingPipelineConfig;
use proto::p4runtime::SetForwardingPipelineConfigRequest;
use proto::p4runtime::SetForwardingPipelineConfigRequest_Action;
use proto::p4info::P4Info;

use protobuf::parse_from_reader;

use std::env;
use std::fs;
use std::sync::Arc;

use grpcio::{ChannelBuilder, EnvBuilder};

fn main() {
    let args = env::args().collect::<Vec<_>>();
    if args.len() != 4 {
        panic!("usage: {} PORT P4INFO.BIN BMV2.JSON", args[0])
    }
    let port = args[1]
        .parse::<u16>()
        .unwrap_or_else(|_| panic!("{} is not a valid port number", args[1]));
    let mut p4info_file = fs::File::open(&args[2])
        .unwrap_or_else(|err| panic!("{}: open failed ({})", args[2], err));
    let p4info: P4Info  = parse_from_reader(&mut p4info_file)
        .unwrap_or_else(|err| panic!("{}: read failed ({})", args[2], err));
    let json = fs::read(&args[3])
        .unwrap_or_else(|err| panic!("{}: could not read file ({})", args[3], err));

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

    let mut config = ForwardingPipelineConfig::new();
    config.set_p4_device_config(json);
    config.set_p4info(p4info);

    let mut pipeline_request = SetForwardingPipelineConfigRequest::new();
    pipeline_request.set_action(SetForwardingPipelineConfigRequest_Action::VERIFY_AND_COMMIT);
    pipeline_request.set_device_id(0); // bmv2 default (set with --device-id)
    pipeline_request.set_role_id(123);
    pipeline_request.set_config(config);
    let pipeline_response = client
        .set_forwarding_pipeline_config(&pipeline_request)
        .expect("RPC failed!");
    println!(
        "sent SetForwardingPipelineConfigRequest and received {:?}",
        pipeline_response
    );

    let pipeline_request = GetForwardingPipelineConfigRequest::new();
    let pipeline_response = client
        .get_forwarding_pipeline_config(&pipeline_request)
        .expect("RPC failed!");
    println!(
        "send {:?} and received {:?}",
        pipeline_request, pipeline_response
    );
}
