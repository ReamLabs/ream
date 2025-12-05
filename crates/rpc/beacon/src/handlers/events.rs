use actix_web::{Responder, get, web};
use actix_web_lab::sse;
use futures::stream;
use ream_events_beacon::BeaconEvent;
use serde::{Deserialize, Deserializer};
use tokio::sync::broadcast;

#[derive(Deserialize)]
pub struct EventQuery {
    #[serde(deserialize_with = "deserialize_topics_array")]
    topics: Vec<String>,
}

// The custom deserializer handles:
// Single string value: ?topics=head → converts to vec!["head"]
// Array format: ?topics=head&topics=block → converts to vec!["head", "block"]

fn deserialize_topics_array<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    use std::fmt;

    use serde::de::{self, Visitor};

    struct TopicsArrayVisitor;

    impl<'de> Visitor<'de> for TopicsArrayVisitor {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string or an array of strings")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(vec![value.to_string()])
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            let mut vec = Vec::new();
            while let Some(item) = seq.next_element::<String>()? {
                vec.push(item);
            }
            Ok(vec)
        }
    }

    deserializer.deserialize_any(TopicsArrayVisitor)
}

#[get("/events")]
pub async fn get_events(
    query: web::Query<EventQuery>,
    event_sender: web::Data<broadcast::Sender<BeaconEvent>>,
) -> impl Responder {
    let topics = query.topics.clone();

    let rx = event_sender.subscribe();

    let stream = stream::unfold(rx, move |mut rx| {
        let topics = topics.clone();
        async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        let event_name = event.event_name();
                        if topics.contains(&event_name.to_string()) {
                            match event.serialize_data() {
                                Ok(json_data) => {
                                    let sse_event = sse::Event::Data(
                                        sse::Data::new(json_data).event(event_name),
                                    );
                                    return Some((Ok::<_, actix_web::Error>(sse_event), rx));
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to serialize event {event_name}: {e}");
                                    continue;
                                }
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        return None;
                    }
                }
            }
        }
    });

    sse::Sse::from_stream(stream).with_keep_alive(std::time::Duration::from_secs(10))
}
