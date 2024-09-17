use std::env;
use std::time::Duration;

use actix_web::{get, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use serde::{Deserialize, Serialize};

#[get("/")]
async fn index(_req: HttpRequest) -> impl Responder {
    log::info!("Endpoint: index");
    "Welcome!"
}

#[get("/healthcheck")]
async fn healthcheck(_req: HttpRequest) -> impl Responder {
    log::info!("Endpoint: healthcheck");
    HttpResponse::Ok()
}

#[get("/shutdown")]
async fn shutdown(_req: HttpRequest) -> impl Responder {
    log::info!("Endpoint: shutdown");
    HttpResponse::Ok()
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GenerateTrafficQuery {
    #[serde(with = "humantime_serde")]
    pub sleep_time: Duration,
    pub response_size: u64,
}

#[get("/generate-traffic")]
async fn generate_traffic(info: web::Query<GenerateTrafficQuery>) -> impl Responder {
    log::info!(
        "Endpoint: generate-traffic. Sleep time: {}. Response size: {}",
        humantime::format_duration(info.sleep_time),
        info.response_size
    );

    tokio::time::sleep(info.sleep_time).await;
    HttpResponse::Ok().body(vec![1; info.response_size as usize])
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let args: Vec<String> = env::args().collect();

    log::info!("Dummy runtime. Args: {args:?}");

    Ok(HttpServer::new(|| {
        App::new()
            .service(index)
            .service(healthcheck)
            .service(generate_traffic)
            .service(shutdown)
    })
    .bind(("127.0.0.1", 7861))?
    .run()
    .await?)
}
