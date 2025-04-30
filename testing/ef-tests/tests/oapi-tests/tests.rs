async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Version of the API specification to test against
    let version = "v2.3.0";
    
    // Configuration for the OpenAPI file
    let open_api_file = OpenApiFile {
        url: format!("https://github.com/ethereum/beacon-APIs/releases/download/{}/beacon-node-oapi.json", version),
        filepath: "oapi-schemas/beacon-node-oapi.json".into(),
        version: Regex::new(&version).unwrap(),
    };

    // Combine route data from all modules
    let routes_data = Routes {
        beacon: routes::beacon::routes_data(),
        config: routes::config::routes_data(),
        debug: routes::debug::routes_data(),
        events: routes::events::routes_data(),
        lightclient: routes::lightclient::routes_data(),
        node: routes::node::routes_data(),
        proof: routes::proof::routes_data(),
        validator: routes::validator::routes_data(),
    };

    // Create chain configuration
    let config = create_chain_fork_config(ChainForkConfig {
        altair_fork_epoch: 1,
        bellatrix_fork_epoch: 2,
        ..default_chain_config()
    });

    // Combine request serializers from all modules
    let req_serializers = ReqSerializers {
        beacon: routes::beacon::get_req_serializers(&config),
        config: routes::config::get_req_serializers(),
        debug: routes::debug::get_req_serializers(),
        events: get_events_req_serializers(),
        lightclient: routes::lightclient::get_req_serializers(),
        node: routes::node::get_req_serializers(),
        proof: routes::proof::get_req_serializers(),
        validator: routes::validator::get_req_serializers(),
    };

    // Combine return types from all modules
    let return_types = ReturnTypes {
        beacon: routes::beacon::get_return_types(),
        config: routes::config::get_return_types(),
        debug: routes::debug::get_return_types(),
        lightclient: routes::lightclient::get_return_types(),
        node: routes::node::get_return_types(),
        proof: routes::proof::get_return_types(),
        validator: routes::validator::get_return_types(),
    };

    // Combine test data from all modules
    let test_datas = TestData {
        beacon: beacon_test_data(),
        config: config_test_data(),
        debug: debug_test_data(),
        events: events_test_data(),
        lightclient: lightclient_test_data(),
        node: node_test_data(),
        proofs: proofs_test_data(),
        validator: validator_test_data(),
    };

    // Fetch and parse the OpenAPI specification
    let open_api_json = fetch_open_api_spec(&open_api_file).await?;
    
    // Options for test execution
    let options = TestOptions {
        routes_drop_one_of: vec![
            "produceBlockV2".to_string(),
            "produceBlindedBlock".to_string(),
            "publishBlindedBlock".to_string(),
        ],
    };

    // Run the main validation tests
    run_test_check_against_spec(
        &open_api_json,
        &routes_data,
        &req_serializers,
        &return_types,
        &test_datas,
        &options,
    )?;

    // Additional test for eventstream events
    test_eventstream_events(&open_api_json, &config)?;

    Ok(())
}

// Implementation of specialized events request serializers
fn get_events_req_serializers() -> EventsReqSerializers {
    EventsReqSerializers {
        eventstream: EventstreamSerializer {
            write_req: |topics| {
                QueryParams {
                    topics,
                }
            },
            parse_req: |query| {
                (query.topics, None, None)
            },
            schema: Schema {
                query: SchemaQuery {
                    topics: Schema::string_array(),
                },
            },
        },
    }
}

// Additional test function for eventstream events
fn test_eventstream_events(
    open_api_json: &serde_json::Value,
    config: &ChainForkConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    // Extract eventstream examples from the OpenAPI spec
    let eventstream_examples = open_api_json
        .get("paths")
        .and_then(|paths| paths.get("/eth/v1/events"))
        .and_then(|path| path.get("get"))
        .and_then(|get| get.get("responses"))
        .and_then(|responses| responses.get("200"))
        .and_then(|ok| ok.get("content"))
        .and_then(|content| content.get("text/event-stream"))
        .and_then(|stream| stream.get("examples"))
        .ok_or_else(|| "Failed to find eventstream examples in OpenAPI spec".into())?;

    // Get event serializers/deserializers
    let event_serdes = routes::events::get_event_serdes(config);
    
    // Create a set of known event topics
    let known_topics: HashSet<String> = routes::events::event_types()
        .into_iter()
        .map(String::from)
        .collect();

    // Iterate through each example in the spec
    for (topic, example) in eventstream_examples.as_object().unwrap() {
        if !known_topics.contains(topic) {
            return Err(format!("Topic {} not implemented", topic).into());
        }

        let value = example
            .get("value")
            .ok_or_else(|| format!("No value for example {}", topic))?
            .as_str()
            .unwrap();

        // Extract the data JSON from the example
        let example_data_str = value
            .lines()
            .find(|line| line.starts_with("data:"))
            .ok_or_else(|| format!("Event example value must include 'data:' {}", value))?;

        let example_data_json: serde_json::Value = serde_json::from_str(
            &example_data_str[5..].trim()
        )?;

        // Get our test event data for this topic
        let test_event = event_test_data()
            .get(topic)
            .ok_or_else(|| format!("No eventTestData for {}", topic))?;

        // Convert our test event to JSON format
        let test_event_json = event_serdes.to_json(&BeaconEvent {
            type_: topic.to_string(),
            message: test_event.clone(),
        })?;

        // Assert that our test data matches the example from the spec
        assert_eq!(
            test_event_json, 
            example_data_json,
            "eventTestData[{}] does not match spec's example", 
            topic
        );
    }

    Ok(())
}


