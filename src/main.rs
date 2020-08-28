extern crate grpcio;
extern crate isatty;
extern crate itertools;
extern crate proto;
extern crate protobuf;

extern crate clap;
use clap::{App, Arg, ArgMatches, SubCommand};

use proto::p4info;
use proto::p4runtime::CapabilitiesRequest;
use proto::p4runtime::ForwardingPipelineConfig;
use proto::p4runtime::ForwardingPipelineConfig_Cookie;
use proto::p4runtime::GetForwardingPipelineConfigRequest;
use proto::p4runtime::SetForwardingPipelineConfigRequest;
use proto::p4runtime::SetForwardingPipelineConfigRequest_Action;
use proto::p4runtime::Uint128;
use proto::p4runtime_grpc::P4RuntimeClient;
use proto::p4types;

use protobuf::parse_from_reader;
use protobuf::Message;

use std::collections::HashMap;
use std::env;
use std::ffi::OsStr;
use std::fmt::{self, Display};
use std::fs;
use std::io::{self, Write};
use std::str;
use std::str::FromStr;
use std::sync::Arc;

use grpcio::{ChannelBuilder, EnvBuilder};

use itertools::Itertools;

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

fn bytes_to_text(s: Vec<u8>) -> Option<String> {
    match String::from_utf8(s) {
        Ok(utf8) if !utf8.contains("\0") => Some(utf8),
        _ => None,
    }
}

fn set_pipeline_subcommand<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name("set-pipeline")
        .about("Installs a new forwarding pipeline.")
        .arg(Arg::from_usage(
            "<p4info> 'Name of file with P4Info to install, in protobuf binary format'",
        ))
        .arg(Arg::from_usage(
            "<opaque> 'Name of file with P4 device config in device-specific format'",
        ))
        .arg(Arg::from_usage("--cookie [COOKIE] 'Cookie to install'").validator(validate_u64))
        .arg(
            Arg::from_usage("--action [ACTION] 'Action to take'")
                .possible_values(&[
                    "verify",
                    "verify-and-save",
                    "verify-and-commit",
                    "reconcile-and-commit",
                ])
                .default_value("verify-and-commit")
                .case_insensitive(true),
        )
}

fn do_set_pipeline(
    set_pipeline: &ArgMatches,
    device_id: u64,
    role_id: u64,
    election_id: Option<Uint128>,
    target: &str,
    client: &P4RuntimeClient,
) {
    let p4info_os = set_pipeline.value_of_os("p4info").unwrap();
    let p4info_str = set_pipeline.value_of_lossy("p4info").unwrap();
    let mut p4info_file = fs::File::open(p4info_os)
        .unwrap_or_else(|err| panic!("{}: could not open P4Info ({})", p4info_str, err));
    let p4info = parse_from_reader(&mut p4info_file)
        .unwrap_or_else(|err| panic!("{}: could not read P4Info ({})", p4info_str, err));

    let opaque_filename = set_pipeline.value_of_os("opaque").unwrap();
    let opaque = fs::read(opaque_filename).unwrap_or_else(|err| {
        panic!(
            "{}: could not read opaque data ({})",
            opaque_filename.to_string_lossy(),
            err
        )
    });

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
        _ => RECONCILE_AND_COMMIT,
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
        .unwrap_or_else(|err| panic!("{}: failed to set forwarding pipeline ({})", target, err));
}

fn commit_pipeline_subcommand<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name("commit-pipeline").about(
        "Realizes the config last saved, but not committed (e.g. with \
                \"set-pipeline --action=verify-and-save\")",
    )
}

fn do_commit_pipeline(
    device_id: u64,
    role_id: u64,
    election_id: Option<Uint128>,
    target: &str,
    client: &P4RuntimeClient,
) {
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
        .unwrap_or_else(|err| panic!("{}: failed to commit forwarding pipeline ({})", target, err));
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

fn do_get_pipeline(
    get_pipeline: &ArgMatches,
    device_id: u64,
    target: &str,
    client: &P4RuntimeClient,
) {
    let opaque = get_pipeline.value_of_os("opaque");
    let cookie = get_pipeline.is_present("cookie");
    let mut p4info = get_pipeline.value_of_os("p4info");
    if !cookie && p4info == None && opaque == None {
        p4info = Some(OsStr::new("-"))
    }

    let mut get_pipeline_request = GetForwardingPipelineConfigRequest::new();
    get_pipeline_request.set_device_id(device_id);
    use proto::p4runtime::GetForwardingPipelineConfigRequest_ResponseType::*;
    get_pipeline_request.set_response_type(if opaque != None && p4info != None {
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
            panic!(
                "{}: failed to retrieve forwarding pipeline ({})",
                target, err
            )
        });
    let pipeline = pipeline_response.get_config();

    if let Some(p4info) = p4info {
        if !pipeline.has_p4info() {
            panic!("{}: device did not return P4Info", target);
        } else if p4info == "-" {
            println!("{:#?}", pipeline.get_p4info());
        } else {
            fs::write(p4info, pipeline.get_p4info().write_to_bytes().unwrap()).unwrap_or_else(
                |err| {
                    panic!(
                        "{}: could not write P4Info ({})",
                        p4info.to_string_lossy(),
                        err
                    )
                },
            );
        }
    }

    if let Some(opaque) = opaque {
        let config = pipeline.get_p4_device_config();
        if config.len() == 0 {
            eprintln!(
                "{}: warning: device returned empty opaque configuration",
                target
            );
        }

        if opaque != "-" {
            fs::write(opaque, config).unwrap_or_else(|err| {
                panic!("{}: could not write opaque configuration ({})", target, err)
            });
        } else if !stdout_isatty() {
            io::stdout()
                .write_all(pipeline.get_p4_device_config())
                .unwrap_or_else(|err| panic!("-: could not write opaque configuration ({})", err));
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

fn get_capabilities_subcommand<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name("get-capabilities")
        .about("Prints the server's capabilities string.")
        .long_about(
            "Prints the server's capabilities string (a version number, \
                     e.g. \"1.1.0-rc.1\").",
        )
}

fn do_get_capabilities(target: &str, client: &P4RuntimeClient) {
    let capabilities_response = client
        .capabilities(&CapabilitiesRequest::new())
        .unwrap_or_else(|err| panic!("{}: failed to get capabilities ({})", target, err));
    println!("{}", capabilities_response.p4runtime_api_version)
}

#[derive(Clone)]
struct SourceLocation {
    file: String,
    line: i32,
    column: i32,
}

impl From<&p4types::SourceLocation> for SourceLocation {
    fn from(s: &p4types::SourceLocation) -> Self {
        SourceLocation {
            file: s.file.clone(),
            line: s.line,
            column: s.column,
        }
    }
}

impl Display for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.file)?;
        if self.line != 0 {
            write!(f, ":{}", self.line)?;
            if self.column != 0 {
                write!(f, ":{}", self.column)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
enum Expression {
    String(String),
    Integer(i64),
    Bool(bool),
}

impl From<&p4types::Expression> for Expression {
    fn from(e: &p4types::Expression) -> Self {
        use p4types::Expression_oneof_value::*;
        match e.value {
            Some(string_value(ref s)) => Expression::String(s.clone()),
            Some(int64_value(i)) => Expression::Integer(i),
            Some(bool_value(b)) => Expression::Bool(b),
            None => todo!(),
        }
    }
}

impl Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expression::String(s) => write!(f, "\"{}\"", s.escape_debug()),
            Expression::Integer(i) => write!(f, "{}", i),
            Expression::Bool(b) => write!(f, "{}", b),
        }
    }
}

#[derive(Clone)]
struct KeyValuePair(String, Expression);

impl From<&p4types::KeyValuePair> for KeyValuePair {
    fn from(kvp: &p4types::KeyValuePair) -> Self {
        KeyValuePair(kvp.get_key().into(), kvp.get_value().into())
    }
}

impl Display for KeyValuePair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}={}", self.0.escape_debug(), self.1)
    }
}

#[derive(Clone)]
enum AnnotationValue {
    Empty,
    Unstructured(String),
    Expressions(Vec<Expression>),
    KeyValuePairs(Vec<KeyValuePair>),
}

impl From<&p4types::ExpressionList> for AnnotationValue {
    fn from(el: &p4types::ExpressionList) -> Self {
        AnnotationValue::Expressions(el.get_expressions().iter().map(|e| e.into()).collect())
    }
}

impl From<&p4types::KeyValuePairList> for AnnotationValue {
    fn from(kvpl: &p4types::KeyValuePairList) -> Self {
        AnnotationValue::KeyValuePairs(kvpl.get_kv_pairs().iter().map(|kvp| kvp.into()).collect())
    }
}

impl From<&p4types::StructuredAnnotation> for AnnotationValue {
    fn from(sa: &p4types::StructuredAnnotation) -> AnnotationValue {
        if sa.has_expression_list() {
            sa.get_expression_list().into()
        } else {
            sa.get_kv_pair_list().into()
        }
    }
}

#[derive(Clone)]
struct Annotations(HashMap<String, (Option<SourceLocation>, AnnotationValue)>);

fn parse_annotations<'a, T, U, V>(
    annotations: T,
    annotation_locs: U,
    structured_annotations: V,
) -> Annotations
where
    T: IntoIterator<Item = &'a String>,
    U: IntoIterator<Item = &'a p4types::SourceLocation>,
    V: IntoIterator<Item = &'a p4types::StructuredAnnotation>,
{
    use AnnotationValue::*;

    // The annotation locations are optional.  Extend them so that we
    // always have one to match up with the annotations.
    let extended_annotation_locs = annotation_locs
        .into_iter()
        .map(|a| Some(a.into()))
        .chain(std::iter::repeat(None));
    let unstructured_annotations =
        annotations
            .into_iter()
            .zip(extended_annotation_locs)
            .map(|(s, source_location)| {
                let s = s.trim_start_matches("@");
                if s.contains("(") && s.ends_with(")") {
                    let index = s.find("(").unwrap();
                    let name = String::from(&s[0..index]);
                    let value = s[index + 1..].strip_suffix(')').unwrap().into();
                    (name, (source_location, Unstructured(value)))
                } else {
                    (s.into(), (source_location, Empty))
                }
            });
    let structured_annotations = structured_annotations.into_iter().map(|x| {
        (
            x.name.clone(),
            (
                if x.has_source_location() {
                    Some(x.get_source_location().into())
                } else {
                    None
                },
                x.into(),
            ),
        )
    });
    Annotations(
        unstructured_annotations
            .chain(structured_annotations)
            .collect(),
    )
}

fn format_structured_annotation<T, U>(f: &mut fmt::Formatter<'_>, values: T) -> fmt::Result
where
    T: Iterator<Item = U>,
    U: Display,
{
    write!(f, "[")?;
    for (i, e) in values.enumerate() {
        if i > 0 {
            write!(f, ", ")?;
        }
        write!(f, "{}", e)?;
    }
    write!(f, "]")
}

impl Display for Annotations {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Sort annotations by name to ensure predictable output.
        let sorted_annotations = self.0.iter().sorted_by(|a, b| a.0.cmp(b.0));
        for (i, (k, (_, v))) in sorted_annotations.into_iter().enumerate() {
            if i > 0 {
                write!(f, " ")?;
            }
            write!(f, "@{}", k)?;

            use AnnotationValue::*;
            match v {
                Empty => (),
                Unstructured(s) => write!(f, "({})", s.escape_debug())?,
                Expressions(expressions) => format_structured_annotation(f, expressions.iter())?,
                KeyValuePairs(kvp) => format_structured_annotation(f, kvp.iter())?,
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
struct Documentation {
    brief: String,
    description: String,
}

impl From<&p4info::Documentation> for Documentation {
    fn from(t: &p4info::Documentation) -> Self {
        Self {
            brief: t.brief.clone(),
            description: t.description.clone(),
        }
    }
}

#[derive(Clone)]
struct Preamble {
    id: u32,
    name: String,
    alias: String,
    annotations: Annotations,
    doc: Documentation,
}

impl From<&p4info::Preamble> for Preamble {
    fn from(p: &p4info::Preamble) -> Self {
        Preamble {
            id: p.id,
            name: p.name.clone(),
            alias: p.alias.clone(),
            annotations: parse_annotations(
                p.get_annotations(),
                p.get_annotation_locations(),
                p.get_structured_annotations(),
            ),
            doc: p.get_doc().into(),
        }
    }
}

enum MatchType {
    Unspecified,
    Exact,
    Lpm,
    Ternary,
    Range,
    Optional,
    Other(String),
}

impl Display for MatchType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use MatchType::*;
        let s = match self {
            Unspecified => "unspecified",
            Exact => "exact",
            Lpm => "LPM",
            Ternary => "ternary",
            Range => "range",
            Optional => "optional",
            Other(s) => &s,
        };
        write!(f, "{}", s)
    }
}

struct MatchField {
    // The protobuf representation of MatchField doesn't include a
    // Preamble but it includes everything in the preamble except
    // 'alias'.  It seems more uniform to just use Preamble here.
    preamble: Preamble,
    bit_width: i32,
    match_type: MatchType,
    type_name: Option<String>,
}

impl From<&p4info::MatchField> for MatchField {
    fn from(mf: &p4info::MatchField) -> Self {
        use p4info::MatchField_MatchType::*;
        MatchField {
            preamble: Preamble {
                id: mf.id,
                name: mf.name.clone(),
                alias: mf.name.clone(),
                annotations: parse_annotations(
                    mf.get_annotations(),
                    mf.get_annotation_locations(),
                    mf.get_structured_annotations(),
                ),
                doc: mf.get_doc().into(),
            },
            bit_width: mf.bitwidth,
            match_type: match mf.get_match_type() {
                EXACT => MatchType::Exact,
                LPM => MatchType::Lpm,
                TERNARY => MatchType::Ternary,
                RANGE => MatchType::Range,
                OPTIONAL => MatchType::Optional,
                UNSPECIFIED => {
                    if mf.has_other_match_type() {
                        MatchType::Other(mf.get_other_match_type().into())
                    } else {
                        MatchType::Unspecified
                    }
                }
            },
            type_name: None, // XXX
        }
    }
}

impl Display for MatchField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "field {}: bit<{}>", self.preamble.name, self.bit_width)?;
        if let Some(ref type_name) = self.type_name {
            write!(f, " ({})", type_name.escape_debug())?;
        }
        write!(f, " {}-match", self.match_type)?;
        if !self.preamble.annotations.0.is_empty() {
            write!(f, " {}", self.preamble.annotations)?;
        };
        Ok(())
    }
}

fn parse_type_name(pnto: Option<&p4types::P4NamedType>) -> Option<String> {
    pnto.map(|pnt| pnt.name.clone())
}

#[derive(Clone)]
struct Param {
    // The protobuf representation of Param doesn't include a
    // Preamble but it includes everything in the preamble except
    // 'alias'.  It seems more uniform to just use Preamble here.
    preamble: Preamble,
    bit_width: i32,
    type_name: Option<String>,
}

impl From<&p4info::Action_Param> for Param {
    fn from(ap: &p4info::Action_Param) -> Self {
        Param {
            preamble: Preamble {
                id: ap.id,
                name: ap.name.clone(),
                alias: ap.name.clone(),
                annotations: parse_annotations(
                    ap.get_annotations(),
                    ap.get_annotation_locations(),
                    ap.get_structured_annotations(),
                ),
                doc: ap.get_doc().into(),
            },
            bit_width: ap.bitwidth,
            type_name: parse_type_name(ap.type_name.as_ref()),
        }
    }
}

impl Display for Param {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: bit<{}>", self.preamble.name, self.bit_width)
    }
}

#[derive(Clone)]
struct Action {
    preamble: Preamble,
    params: Vec<Param>,
}

impl From<&p4info::Action> for Action {
    fn from(a: &p4info::Action) -> Self {
        Action {
            preamble: a.get_preamble().into(),
            params: a.get_params().iter().map(|x| x.into()).collect(),
        }
    }
}

impl Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "action {}(", self.preamble.name)?;
        for (p_index, p) in self.params.iter().enumerate() {
            if p_index > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", p)?;
        }
        write!(f, ")")
    }
}

struct ActionRef {
    action: Action,
    may_be_default: bool, // Allowed as the default action?
    may_be_entry: bool,   // Allowed as an entry's action?
    annotations: Annotations,
}

impl ActionRef {
    fn new_from_proto(ar: &p4info::ActionRef, actions: &HashMap<u32, Action>) -> Self {
        ActionRef {
            action: actions.get(&ar.id).unwrap().clone(),
            may_be_default: ar.scope != p4info::ActionRef_Scope::TABLE_ONLY,
            may_be_entry: ar.scope != p4info::ActionRef_Scope::DEFAULT_ONLY,
            annotations: parse_annotations(
                ar.get_annotations(),
                ar.get_annotation_locations(),
                ar.get_structured_annotations(),
            ),
        }
    }
}

impl Display for ActionRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.may_be_entry {
            write!(f, "default-only ")?;
        } else if !self.may_be_default {
            write!(f, "not-default ")?;
        }
        write!(f, "{}", self.action)?;
        if !self.annotations.0.is_empty() {
            write!(f, " {}", self.annotations)?;
        };
        Ok(())
    }
}

struct Table {
    preamble: Preamble,
    match_fields: Vec<MatchField>,
    actions: Vec<ActionRef>,
    const_default_action: Option<Action>,
    //action_profile: Option<ActionProfile>,
    //direct_counter: Option<DirectCounter>,
    //direct_meter: Option<DirectMeter>,
    max_entries: Option<u64>,
    idle_notify: bool,
    is_const_table: bool,
}

impl Table {
    fn new_from_proto(t: &p4info::Table, actions: &HashMap<u32, Action>) -> Self {
        Table {
            preamble: t.get_preamble().into(),
            match_fields: t.get_match_fields().iter().map(|x| x.into()).collect(),
            actions: t
                .get_action_refs()
                .iter()
                .map(|x| ActionRef::new_from_proto(x, actions))
                .collect(),
            const_default_action: None, // XXX
            max_entries: if t.size > 0 {
                Some(t.size as u64)
            } else {
                None
            },
            idle_notify: t.idle_timeout_behavior
                == p4info::Table_IdleTimeoutBehavior::NOTIFY_CONTROL,
            is_const_table: t.is_const_table,
        }
    }
}

struct Switch {
    tables: Vec<Table>,
}

impl From<&p4info::P4Info> for Switch {
    fn from(p4i: &p4info::P4Info) -> Self {
        let actions: HashMap<u32, Action> = p4i
            .get_actions()
            .iter()
            .map(|x| (x.get_preamble().id, x.into()))
            .collect();
        let tables: Vec<Table> = p4i
            .get_tables()
            .iter()
            .map(|x| Table::new_from_proto(x, &actions))
            .collect();
        Switch { tables }
    }
}

fn list_tables_subcommand<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name("list-tables").about("Lists the tables in the forwarding pipeline.")
}

fn do_list_tables(device_id: u64, target: &str, client: &P4RuntimeClient) {
    let mut get_pipeline_request = GetForwardingPipelineConfigRequest::new();
    get_pipeline_request.set_device_id(device_id);
    get_pipeline_request.set_response_type(
        proto::p4runtime::GetForwardingPipelineConfigRequest_ResponseType::P4INFO_AND_COOKIE,
    );

    let pipeline_response = client
        .get_forwarding_pipeline_config(&get_pipeline_request)
        .unwrap_or_else(|err| {
            panic!(
                "{}: failed to retrieve forwarding pipeline ({})",
                target, err
            )
        });
    let pipeline = pipeline_response.get_config();
    if !pipeline.has_p4info() {
        panic!("{}: device did not return P4Info", target);
    }

    let switch: Switch = pipeline.get_p4info().into();
    for table in switch.tables {
        println!("table {}:", table.preamble.name);
        for mf in table.match_fields {
            println!("\t{}", mf);
        }
        for ar in table.actions {
            println!("\t{}", ar);
        }
        if let Some(max_entries) = table.max_entries {
            println!("\tsize: {}", max_entries);
        }
        if let Some(a) = table.const_default_action {
            println!("\tconst default action {}", a);
        }
        if table.is_const_table {
            println!("\tconst table");
        }
        if table.idle_notify {
            println!("\tidle notify");
        }
    }
}

fn main() {
    let matches = App::new(env!("CARGO_PKG_NAME"))
        .max_term_width(80)
        .version(env!("CARGO_PKG_VERSION"))
        .about("Queries and controls programmable switches using the P4Runtime API")
        .arg(
            Arg::from_usage("-t, --target <TARGET> 'Remote switch target'")
                .default_value("localhost:50051"),
        )
        .arg(
            Arg::from_usage("-d, --device-id <ID> 'Device ID'")
                .validator(validate_u64)
                .default_value("0"),
        )
        .arg(
            Arg::from_usage("-r, --role-id <ID> 'Role ID'")
                .validator(validate_u64)
                .default_value("0"),
        )
        .arg(Arg::from_usage("-e, --election-id [ID] 'Election ID'").validator(validate_uint128))
        .subcommand(get_pipeline_subcommand())
        .subcommand(set_pipeline_subcommand())
        .subcommand(commit_pipeline_subcommand())
        .subcommand(get_capabilities_subcommand())
        .subcommand(list_tables_subcommand())
        .get_matches();

    let device_id = parse_u64(matches.value_of("device-id").unwrap()).unwrap();
    let role_id = parse_u64(matches.value_of("role-id").unwrap()).unwrap();
    let election_id = matches
        .value_of("election-id")
        .map(|x| parse_uint128(x).unwrap());

    let env = Arc::new(EnvBuilder::new().build());
    let target = matches.value_of("target").unwrap();
    let ch = ChannelBuilder::new(env).connect(target);
    let client = P4RuntimeClient::new(ch);

    if let Some(set_pipeline) = matches.subcommand_matches("set-pipeline") {
        do_set_pipeline(
            set_pipeline,
            device_id,
            role_id,
            election_id,
            target,
            &client,
        );
    } else if let Some(_) = matches.subcommand_matches("commit-pipeline") {
        do_commit_pipeline(device_id, role_id, election_id, target, &client);
    } else if let Some(get_pipeline) = matches.subcommand_matches("get-pipeline") {
        do_get_pipeline(get_pipeline, device_id, target, &client);
    } else if let Some(_) = matches.subcommand_matches("get-capabilities") {
        do_get_capabilities(target, &client);
    } else if let Some(_) = matches.subcommand_matches("list-tables") {
        do_list_tables(device_id, target, &client);
    } else {
        unreachable!()
    }
}
