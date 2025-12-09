use std::{any::Any, collections::HashSet, time::Duration};

use libp2p::gossipsub::{Config, Message, MessageId, Sha256Topic};

pub fn assert_common(config: &Config) {
    assert!(config.mesh_n_low() < config.mesh_n() && config.mesh_n() < config.mesh_n_high());
    assert!(config.gossip_lazy() <= config.mesh_n());
    assert!(config.history_gossip() <= config.history_length() && config.history_gossip() > 0);
    let positive_parameters = [
        config.heartbeat_interval(),
        config.fanout_ttl(),
        config.duplicate_cache_time(),
    ];
    for parameter in positive_parameters {
        assert!(parameter > Duration::ZERO);
    }
}

pub fn valid_message_id_computation(decoder: &Config) {
    let topic = "test_topic".to_string();
    let raw_data = b"raw_test_data";

    let message = Message {
        source: None,
        data: raw_data.to_vec(),
        sequence_number: None,
        topic: Sha256Topic::new(&topic).hash(),
    };
    let message_id = decoder.message_id(&message);

    assert!(message_id.0.len() == 20);
    assert!((&message_id.0 as &dyn Any).is::<Vec<u8>>());

    let message_2 = Message {
        source: None,
        data: raw_data.to_vec(),
        sequence_number: None,
        topic: Sha256Topic::new(&topic).hash(),
    };
    let message_2_id = decoder.message_id(&message_2);

    assert!(message_id == message_2_id);
}

pub fn consistant_message_id_caching(decoder: &Config) {
    let topic = "test_topic".to_string();
    let data = b"test_data";

    let message = Message {
        source: None,
        data: data.to_vec(),
        sequence_number: None,
        topic: Sha256Topic::new(&topic).hash(),
    };
    let first_id = decoder.message_id(&message);
    let second_id = decoder.message_id(&message);

    assert!(first_id == second_id);

    let message_2 = Message {
        source: None,
        data: data.to_vec(),
        sequence_number: None,
        topic: Sha256Topic::new(&topic).hash(),
    };
    let message_3 = Message {
        source: None,
        data: data.to_vec(),
        sequence_number: None,
        topic: Sha256Topic::new(&topic).hash(),
    };

    assert!(decoder.message_id(&message_2) == decoder.message_id(&message_3));
}

pub fn message_instantiation_edge_cases(decoder: &Config) {
    let big_data = vec![b'y'; 5000];
    let edge_cases: Vec<(String, &[u8])> = vec![
        (String::new(), b""), // empty input
        ("topic".to_string(), b"data1"),
        ("topic".to_string(), b"data2"),
        ("topic1".to_string(), b"data"),
        ("topic2".to_string(), b"data"),
        ("x".repeat(1000), &big_data),
        (
            String::from_utf8_lossy(b"\x00\xff\x01\xfe").into(),
            b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f",
        ),
    ];

    for edge_case in edge_cases {
        let message = Message {
            source: None,
            data: edge_case.1.to_vec(),
            sequence_number: None,
            topic: Sha256Topic::new(&edge_case.0).hash(),
        };
        let message_id = decoder.message_id(&message);

        assert!(message_id.0.len() == 20);
        assert!((&message_id.0 as &dyn Any).is::<Vec<u8>>());

        let message_2 = Message {
            source: None,
            data: edge_case.1.to_vec(),
            sequence_number: None,
            topic: Sha256Topic::new(&edge_case.0).hash(),
        };
        let message_2_id = decoder.message_id(&message_2);
        assert!(message_id == message_2_id);
    }
}

pub fn message_collisions(decoder: &Config) {
    let test_cases: Vec<(String, &[u8])> = vec![
        ("topic1".to_string(), b"data"),
        ("topic2".to_string(), b"data"),
        ("topic".to_string(), b"data1"),
        ("topic".to_string(), b"data2"),
        ("abc".to_string(), b"def"),
        ("def".to_string(), b"abc"),
        ("topic".to_string(), b"data"),
        (String::from_utf8(b"top\x00ic".to_vec()).unwrap(), b"data"),
    ];

    let messages: Vec<Message> = test_cases
        .into_iter()
        .map(|(topic, data)| Message {
            source: None,
            data: data.to_vec(),
            sequence_number: None,
            topic: Sha256Topic::new(&topic).hash(),
        })
        .collect();

    let message_ids: Vec<MessageId> = messages
        .iter()
        .map(|message| decoder.message_id(message))
        .collect();

    assert!(message_ids.len() == message_ids.iter().collect::<HashSet<_>>().len());

    for id in message_ids {
        assert!(id.0.len() == 20);
    }
}
