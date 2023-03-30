use axum::error_handling::HandleErrorLayer;
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{BoxError, Json, Router, Server};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{read_to_string, write};
use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;
use tokio::time::sleep;
use tower::ServiceBuilder;
use tower_http::request_id::{MakeRequestId, RequestId};
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tower_http::ServiceBuilderExt;
use tracing::info;
use tracing::instrument;
use tracing_subscriber;
use tracing_subscriber::EnvFilter;
use ulid::Ulid;

// This struct implements the trait needed to associate a request with a UUID

#[derive(Clone)]
pub struct MakeRequestUlid;

impl MakeRequestId for MakeRequestUlid {
    fn make_request_id<B>(&mut self, _request: &Request<B>) -> Option<RequestId> {
        let request_id = Ulid::new().to_string().parse().unwrap();
        Some(RequestId::new(request_id))
    }
}

// This struct describes the shape of the JSON that can be posted to our app.

#[derive(Debug, Deserialize)]
pub struct Input {
    content: String,
}

// This struct describes the shape of the AVRO file our app manage and the shape of the JSON output.

#[derive(Debug, Deserialize, Serialize)]
pub struct Record {
    content: String,
}

// This handler saves a JSON into an AVRO file.

#[instrument]
async fn json_to_avro(Json(input): Json<Input>) -> impl IntoResponse {
    let output = Record {
        content: input.content,
    };
    let file_content = read_to_string(Path::new("./schema.avro")).unwrap();
    let schema = apache_avro::Schema::parse_str(&file_content).unwrap();
    let mut writer = apache_avro::Writer::new(&schema, Vec::new());
    writer.append_ser(output).unwrap();
    write("./record.avro", &writer.into_inner().unwrap()).unwrap();
}

// This handler read the AVRO file and return its content as JSON.

#[instrument]
async fn avro_to_json() -> Json<Value> {
    let schema = read_to_string(Path::new("./schema.avro")).unwrap();
    let schema = apache_avro::Schema::parse_str(&schema).unwrap();
    let record = std::fs::read(Path::new("./record.avro")).unwrap();
    let reader = apache_avro::Reader::with_schema(&schema, &record[..]).unwrap();
    let data = reader
        .map(|record| apache_avro::from_value::<Record>(&record.unwrap()).unwrap())
        .map(|output| serde_json::to_value(&output).unwrap())
        .collect::<Value>();
    Json(data)
}

// This function will complete when Ctrl-C is pressed and the platform signal is sent to the app.
// We use it as an example of handling graceful shutdown.

async fn shutdown_signal() {
    tokio::signal::ctrl_c().await.unwrap();
    info!("Ctr-C received: gracefully shutting down...");
}

// This function will handle the application errors and converting them to HTTP status codes.
// I'm really don't know what's going on under the hood but it works.

async fn handle_error(err: BoxError) -> StatusCode {
    if err.is::<tower::timeout::error::Elapsed>() {
        StatusCode::REQUEST_TIMEOUT
    } else if err.is::<tower::load_shed::error::Overloaded>() {
        StatusCode::TOO_MANY_REQUESTS
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

#[tokio::main]
async fn main() {
    // Initialize the tracing subscriber that allow us to transform traces into logs.
    tracing_subscriber::fmt()
        // We specify the pretty printed format for better dev experience.
        .event_format(tracing_subscriber::fmt::format().pretty())
        // This will allow us to read the log level from the env.
        .with_env_filter(EnvFilter::from_default_env())
        // This alllow us to log the spans
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::ACTIVE)
        .init();

    // Initialize the router and the application.
    let router = Router::new()
        // Return an empty 200.
        .route("/healthcheck", get(|| async {}))
        // Return a 408 after 10 seconds.
        .route(
            "/timeout",
            get(|| async { sleep(Duration::from_secs(20)).await }),
        )
        // Return an empty 200 after 5 seconds.
        // This is usefult to test load shedding, concurrency limits and similar.
        .route(
            "/delay",
            get(|| async {
                sleep(Duration::from_secs(5)).await;
            }),
        )
        // Given a compliant JSON file store it in a binary avro file.
        .route("/upload", post(json_to_avro))
        // Send back the avro file deserializing it to JSON.
        .route("/download", get(avro_to_json))
        .layer(
            ServiceBuilder::new()
                // We inject the error handler.
                .layer(HandleErrorLayer::new(handle_error))
                // .layer(tower::load_shed::LoadShedLayer::new())
                .layer(tower::limit::ConcurrencyLimitLayer::new(2))
                .timeout(std::time::Duration::from_secs(10))
                // This layer generate and assign a UULID for each request.
                .set_x_request_id(MakeRequestUlid)
                // This layer generates a new tracing span when a request is received.
                .layer(
                    TraceLayer::new_for_http()
                        .make_span_with(DefaultMakeSpan::new().include_headers(true))
                        .on_response(DefaultOnResponse::new().include_headers(true)),
                )
                // This layer propagate the ULID to the response headers.
                .propagate_x_request_id(),
        );

    Server::bind(&SocketAddr::from(([127, 0, 0, 1], 3000)))
        .serve(router.into_make_service())
        // We inject the Ctrl-C handling function using it for graceful shutdown
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}
