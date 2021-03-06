use std::{collections::HashMap, time::Duration};

use bson::oid::ObjectId;
use serde::Deserialize;

use crate::{
    error::ErrorKind,
    is_master::{IsMasterCommandResponse, IsMasterReply},
    options::{ClientOptions, StreamAddress},
    sdam::description::{
        server::{ServerDescription, ServerType},
        topology::{TopologyDescription, TopologyType},
    },
    test::run_spec_test,
};

#[derive(Debug, Deserialize)]
pub struct TestFile {
    description: String,
    uri: String,
    phases: Vec<Phase>,
}

#[derive(Debug, Deserialize)]
pub struct Phase {
    responses: Vec<Response>,
    outcome: Outcome,
}

#[derive(Debug, Deserialize)]
pub struct Response(String, IsMasterCommandResponse);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    topology_type: TopologyType,
    set_name: Option<String>,
    servers: HashMap<String, Server>,
    logical_session_timeout_minutes: Option<i32>,
    compatible: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Server {
    #[serde(rename = "type")]
    server_type: String,
    set_name: Option<String>,
    set_version: Option<i32>,
    election_id: Option<ObjectId>,
    logical_session_timeout_minutes: Option<i32>,
    min_wire_version: Option<i32>,
    max_wire_version: Option<i32>,
}

fn server_type_from_str(s: &str) -> Option<ServerType> {
    let t = match s {
        "Standalone" => ServerType::Standalone,
        "Mongos" => ServerType::Mongos,
        "RSPrimary" => ServerType::RSPrimary,
        "RSSecondary" => ServerType::RSSecondary,
        "RSArbiter" => ServerType::RSArbiter,
        "RSOther" => ServerType::RSOther,
        "RSGhost" => ServerType::RSGhost,
        "Unknown" | "PossiblePrimary" => ServerType::Unknown,
        _ => return None,
    };

    Some(t)
}

fn run_test(test_file: TestFile) {
    let mut options = ClientOptions::parse(&test_file.uri).expect(&test_file.description);

    if options.hosts.len() == 1 {
        options.direct_connection = Some(true);
    }

    let mut topology_description = TopologyDescription::new(options).unwrap();

    for (i, phase) in test_file.phases.into_iter().enumerate() {
        for Response(address, command_response) in phase.responses {
            let is_master_reply = if command_response == Default::default() {
                Err(ErrorKind::OperationError {
                    message: "dummy error".to_string(),
                }
                .into())
            } else {
                Ok(IsMasterReply {
                    command_response,
                    round_trip_time: Some(Duration::from_millis(1234)), // Doesn't matter for tests.
                })
            };

            topology_description
                .update(ServerDescription::new(
                    StreamAddress::parse(&address).unwrap(),
                    Some(is_master_reply),
                ))
                .unwrap();
        }

        assert_eq!(
            topology_description.topology_type, phase.outcome.topology_type,
            "{}: {}",
            &test_file.description, i,
        );

        assert_eq!(
            topology_description.set_name, phase.outcome.set_name,
            "{}: {}",
            &test_file.description, i,
        );

        // TODO RUST-52: Test for proper logicalSessionTimeoutMinutes value once sessions spec
        // is implemented.

        if let Some(compatible) = phase.outcome.compatible {
            assert_eq!(
                topology_description.compatibility_error.is_none(),
                compatible,
                "{}: {}",
                &test_file.description,
                i,
            );
        }

        assert_eq!(
            topology_description.servers.len(),
            phase.outcome.servers.len(),
            "{}: {}",
            &test_file.description,
            i
        );

        let description = &test_file.description;

        for (address, server) in phase.outcome.servers {
            let actual_server = &topology_description
                .servers
                .get(&StreamAddress::parse(&address).unwrap())
                .unwrap_or_else(|| panic!("{} (phase {})", description, i));

            let server_type = server_type_from_str(&server.server_type)
                .unwrap_or_else(|| panic!("{} (phase {})", description, i));

            assert_eq!(
                actual_server.server_type, server_type,
                "{} (phase {})",
                &test_file.description, i
            );

            assert_eq!(
                actual_server.set_name().unwrap_or(None),
                server.set_name,
                "{} (phase {})",
                &test_file.description,
                i
            );

            assert_eq!(
                actual_server.set_version().unwrap_or(None),
                server.set_version,
                "{} (phase {})",
                &test_file.description,
                i
            );

            assert_eq!(
                actual_server.election_id().unwrap_or(None),
                server.election_id,
                "{} (phase {})",
                &test_file.description,
                i
            );

            // TODO: Test for proper logicalSessionTimeoutMinutes value once sessions spec
            // is implemented.

            if let Some(min_wire_version) = server.min_wire_version {
                assert_eq!(
                    actual_server.min_wire_version().unwrap(),
                    Some(min_wire_version),
                    "{} (phase {})",
                    &test_file.description,
                    i
                );
            }

            if let Some(max_wire_version) = server.max_wire_version {
                assert_eq!(
                    actual_server.max_wire_version().unwrap(),
                    Some(max_wire_version),
                    "{} (phase {})",
                    &test_file.description,
                    i
                );
            }
        }
    }
}

#[cfg_attr(feature = "tokio-runtime", tokio::test(core_threads = 2))]
#[cfg_attr(feature = "async-std-runtime", async_std::test)]
async fn single() {
    run_spec_test(&["server-discovery-and-monitoring", "single"], run_test);
}

#[cfg_attr(feature = "tokio-runtime", tokio::test(core_threads = 2))]
#[cfg_attr(feature = "async-std-runtime", async_std::test)]
async fn rs() {
    run_spec_test(&["server-discovery-and-monitoring", "rs"], run_test);
}

#[cfg_attr(feature = "tokio-runtime", tokio::test(core_threads = 2))]
#[cfg_attr(feature = "async-std-runtime", async_std::test)]
async fn sharded() {
    run_spec_test(&["server-discovery-and-monitoring", "sharded"], run_test);
}
