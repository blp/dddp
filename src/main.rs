extern crate grpcio;
extern crate isatty;
extern crate proto;
extern crate protobuf;

extern crate clap;
use clap::{App, Arg, ArgMatches, SubCommand};

use proto::p4runtime::ForwardingPipelineConfig;
use proto::p4runtime::ForwardingPipelineConfig_Cookie;
use proto::p4runtime::GetForwardingPipelineConfigRequest;
use proto::p4runtime::SetForwardingPipelineConfigRequest;
use proto::p4runtime::SetForwardingPipelineConfigRequest_Action;
use proto::p4runtime::Uint128;
use proto::p4runtime_grpc::P4RuntimeClient;

use protobuf::Message;
use protobuf::parse_from_reader;

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Write};
use std::str;
use std::str::FromStr;
use std::sync::Arc;

use grpcio::{ChannelBuilder, EnvBuilder};

use isatty::stdout_isatty;

fn parse_u64(s: &str) -> Result<u64, <u64 as FromStr>::Err> {
    str::parse(&s)
}
fn validate_u64(s: String) -> Result<(), String> {
    parse_u64(&s)
        .map_err(|_| String::from("Value must be unsigned 64-bit integer."))
        .map(|_| ())
}

fn parse_uint128(s: &str) -> Result<Uint128, <u128 as FromStr>::Err> {
    let x = str::parse::<u128>(&s)?;
    let mut uint128 = Uint128::new();
    uint128.set_high((x >> 64) as u64);
    uint128.set_low(x as u64);
    Ok(uint128)
}
fn validate_uint128(s: String) -> Result<(), String> {
    parse_uint128(&s)
        .map_err(|_| String::from("Value must be unsigned 128-bit integer."))
        .map(|_| ())
}

fn bytes_to_text(s: Vec<u8>) -> Option<String>{
    match String::from_utf8(s) {
        Ok(utf8) if !utf8.contains("\0") => Some(utf8),
        _ => None
    }
}

fn set_pipeline_subcommand<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name("set-pipeline")
        .about("Installs a new forwarding pipeline.")
        .arg(Arg::from_usage("<p4info> 'Name of file with P4Info to install, in protobuf binary format'"))
        .arg(Arg::from_usage("<opaque> 'Name of file with P4 device config in device-specific format'"))
        .arg(Arg::from_usage("--cookie [COOKIE] 'Cookie to install'")
             .validator(validate_u64))
        .arg(Arg::from_usage("--action [ACTION] 'Action to take'")
             .possible_values(&["verify", "verify-and-save",
                                "verify-and-commit", "reconcile-and-commit"])
             .default_value("verify-and-commit")
             .case_insensitive(true))
}

fn do_set_pipeline(set_pipeline: &ArgMatches,
                   device_id: u64,
                   role_id: u64,
                   election_id: Option<Uint128>,
                   target: &str,
                   client: &P4RuntimeClient) {
    let p4info_os = set_pipeline.value_of_os("p4info").unwrap();
    let p4info_str = set_pipeline.value_of_lossy("p4info").unwrap();
    let mut p4info_file = fs::File::open(p4info_os)
        .unwrap_or_else(|err| panic!("{}: could not open P4Info ({})", p4info_str, err));
    let p4info = parse_from_reader(&mut p4info_file)
        .unwrap_or_else(|err| panic!("{}: could not read P4Info ({})", p4info_str, err));

    let opaque_filename = set_pipeline.value_of_os("opaque").unwrap();
    let opaque = fs::read(opaque_filename)
        .unwrap_or_else(|err| panic!("{}: could not read opaque data ({})", opaque_filename.to_string_lossy(), err));

    let mut config = ForwardingPipelineConfig::new();
    config.set_p4_device_config(opaque);
    config.set_p4info(p4info);
    if let Some(cookie) = set_pipeline.value_of("cookie") {
        let mut cookie_jar = ForwardingPipelineConfig_Cookie::new();
        cookie_jar.set_cookie(str::parse::<u64>(&cookie).unwrap());
        config.set_cookie(cookie_jar);
    }

    use SetForwardingPipelineConfigRequest_Action::*;
    let action = match set_pipeline.value_of("action").unwrap() {
        "verify" => VERIFY,
        "verify-and-save" => VERIFY_AND_SAVE,
        "verify-and-commit" => VERIFY_AND_COMMIT,
        _ => RECONCILE_AND_COMMIT
    };

    let mut set_pipeline_request = SetForwardingPipelineConfigRequest::new();
    set_pipeline_request.set_action(action);
    set_pipeline_request.set_device_id(device_id);
    set_pipeline_request.set_role_id(role_id);
    if let Some(id) = election_id {
        set_pipeline_request.set_election_id(id);
    }
    set_pipeline_request.set_config(config);
    client
        .set_forwarding_pipeline_config(&set_pipeline_request)
        .unwrap_or_else(|err| {
            panic!("{}: failed to set forwarding pipeline ({})", target, err)
        });
}

fn commit_pipeline_subcommand<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name("commit-pipeline")
        .about("Realizes the config last saved, but not committed (e.g. with \
                \"set-pipeline --action=verify-and-save\")")
}

fn do_commit_pipeline(device_id: u64,
                      role_id: u64,
                      election_id: Option<Uint128>,
                      target: &str,
                      client: &P4RuntimeClient) {
    use SetForwardingPipelineConfigRequest_Action::COMMIT;
    let mut set_pipeline_request = SetForwardingPipelineConfigRequest::new();
    set_pipeline_request.set_action(COMMIT);
    set_pipeline_request.set_device_id(device_id);
    set_pipeline_request.set_role_id(role_id);
    if let Some(id) = election_id {
        set_pipeline_request.set_election_id(id);
    }
    client
        .set_forwarding_pipeline_config(&set_pipeline_request)
        .unwrap_or_else(|err| {
            panic!("{}: failed to commit forwarding pipeline ({})", target, err)
        });
}

fn get_pipeline_subcommand<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name("get-pipeline")
        .about("Retrieves the current forwarding pipeline.")
        .long_about("Retrieves the current forwarding pipeline.  \
                     With no options, retrieves and prints the P4Info in \
                     protobuf text format on stdout.")
        .arg(Arg::from_usage("--p4info [P4INFO] 'Write P4Info, in protobuf binary format, to <P4INFO>; use - to write to stdout in text format'"))
        .arg(Arg::from_usage("--opaque [OPAQUE] 'Write P4 device config, in device-specific format, to <OPAQUE>; use - to write to stdout'"))
        .arg(Arg::from_usage("--cookie 'Print cookie on stdout'"))
}

fn do_get_pipeline(get_pipeline: &ArgMatches,
                   device_id: u64,
                   target: &str,
                   client: &P4RuntimeClient) {
    let opaque = get_pipeline.value_of_os("opaque");
    let cookie = get_pipeline.is_present("cookie");
    let mut p4info = get_pipeline.value_of_os("p4info");
    if !cookie && p4info == None && opaque == None {
        p4info = Some(OsStr::new("-"))
    }

    let mut get_pipeline_request = GetForwardingPipelineConfigRequest::new();
    get_pipeline_request.set_device_id(device_id);
    use proto::p4runtime::GetForwardingPipelineConfigRequest_ResponseType::*;
    get_pipeline_request.set_response_type(
        if opaque != None && p4info != None {
            ALL
        } else if opaque != None {
            DEVICE_CONFIG_AND_COOKIE
        } else if p4info != None {
            P4INFO_AND_COOKIE
        } else if cookie {
            COOKIE_ONLY
        } else {
            unreachable!()
        });

    let pipeline_response = client
        .get_forwarding_pipeline_config(&get_pipeline_request)
        .unwrap_or_else(|err| {
            panic!("{}: failed to retrieve forwarding pipeline ({})", target, err)
        });
    let pipeline = pipeline_response.get_config();

    if let Some(p4info) = p4info {
        if !pipeline.has_p4info() {
            panic!("{}: device did not return P4Info", target);
        } else if p4info == "-" {
            println!("{:#?}", pipeline.get_p4info());
        } else {
            fs::write(p4info, pipeline.get_p4info().write_to_bytes().unwrap())
                .unwrap_or_else(|err| {
                    panic!("{}: could not write P4Info ({})",
                           p4info.to_string_lossy(), err)
                });
        }
    }

    if let Some(opaque) = opaque {
        let config = pipeline.get_p4_device_config();
        if config.len() == 0 {
            eprintln!("{}: warning: device returned empty opaque configuration", target);
        }

        if opaque != "-" {
            fs::write(opaque, config)
                .unwrap_or_else(|err| {
                    panic!("{}: could not write opaque configuration ({})",
                           target, err)
                });
        } else if !stdout_isatty() {
            io::stdout().write_all(pipeline.get_p4_device_config())
                .unwrap_or_else(|err| {
                    panic!("-: could not write opaque configuration ({})", err)
                });
        } else if let Some(s) = bytes_to_text(config.to_vec()) {
            println!("{}", s)
        } else {
            panic!("{}: not writing binary device config to terminal", target)
        }
    }

    if cookie {
        println!("{}", pipeline.get_cookie().cookie);
    }
}

fn main() {
    let matches = App::new(env!("CARGO_PKG_NAME")).max_term_width(80)
        .version(env!("CARGO_PKG_VERSION"))
        .about("Queries and controls programmable switches using the P4Runtime API")
        .arg(Arg::from_usage("-t, --target <TARGET> 'Remote switch target'")
             .default_value("localhost:50051"))
        .arg(Arg::from_usage("-d, --device-id <ID> 'Device ID'")
             .validator(validate_u64)
             .default_value("0"))
        .arg(Arg::from_usage("-r, --role-id <ID> 'Role ID'")
             .validator(validate_u64)
             .default_value("0"))
        .arg(Arg::from_usage("-e, --election-id [ID] 'Election ID'")
             .validator(validate_uint128))
        .subcommand(get_pipeline_subcommand())
        .subcommand(set_pipeline_subcommand())
        .subcommand(commit_pipeline_subcommand())
        .get_matches();

    let device_id = parse_u64(matches.value_of("device-id").unwrap()).unwrap();
    let role_id = parse_u64(matches.value_of("role-id").unwrap()).unwrap();
    let election_id = matches.value_of("election-id").map(|x| parse_uint128(x).unwrap());

    let env = Arc::new(EnvBuilder::new().build());
    let target = matches.value_of("target").unwrap();
    let ch = ChannelBuilder::new(env).connect(target);
    let client = P4RuntimeClient::new(ch);

    if let Some(set_pipeline) = matches.subcommand_matches("set-pipeline") {
        do_set_pipeline(set_pipeline, device_id, role_id, election_id,
                        target, &client);
    } else if let Some(_) = matches.subcommand_matches("commit-pipeline") {
        do_commit_pipeline(device_id, role_id, election_id, target, &client);
    } else if let Some(get_pipeline) = matches.subcommand_matches("get-pipeline") {
        do_get_pipeline(get_pipeline, device_id, target, &client);
    } else {
        unreachable!()
    }
}
