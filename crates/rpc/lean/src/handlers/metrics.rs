use actix_web::{HttpResponse, Responder, get};
use prometheus_exporter::prometheus::{Encoder, TextEncoder, gather};

const PROMETHEUS_TEXT_CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

/// Serves the Prometheus text exposition format expected by Lean API clients.
#[get("/metrics")]
pub async fn get_metrics() -> impl Responder {
    let encoder = TextEncoder::new();
    let mut body = Vec::new();

    match encoder.encode(&gather(), &mut body) {
        Ok(()) => HttpResponse::Ok()
            .content_type(PROMETHEUS_TEXT_CONTENT_TYPE)
            .body(body),
        Err(error) => HttpResponse::InternalServerError()
            .content_type("text/plain; charset=utf-8")
            .body(format!("failed to encode Prometheus metrics: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use actix_web::{
        App,
        http::{StatusCode, header::CONTENT_TYPE},
        test,
    };
    use ream_metrics::{NODE_INFO, set_int_gauge_vec};

    use super::get_metrics;

    #[actix_web::test]
    async fn serves_prometheus_text_exposition() {
        set_int_gauge_vec(&NODE_INFO, 1, &["ream", "test"]);

        let app = test::init_service(App::new().service(get_metrics)).await;
        let request = test::TestRequest::get().uri("/metrics").to_request();
        let response = test::call_service(&app, request).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            "text/plain; version=0.0.4; charset=utf-8"
        );

        let body = String::from_utf8(test::read_body(response).await.to_vec()).unwrap();
        assert!(body.contains("# HELP lean_node_info"));
        assert!(body.contains("# TYPE lean_node_info gauge"));
    }
}
