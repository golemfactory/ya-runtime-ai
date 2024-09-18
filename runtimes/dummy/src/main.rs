use base64::prelude::*;
use std::env;
use std::io::Cursor;
use std::time::Duration;

use actix_web::{get, post, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use serde::{Deserialize, Serialize};
use serde_json::json;

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

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Text2ImageBody {
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[post("/sdapi/v1/txt2img")]
async fn text2img(params: web::Json<Text2ImageBody>) -> impl Responder {
    log::info!("Endpoint: sdapi/v1/txt2img");

    let mut bytes: Vec<u8> = Vec::new();

    let params = params.into_inner();
    let width = params.width.unwrap_or(800);
    let height = params.height.unwrap_or(800);

    match fractal(width, height).write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png) {
        Ok(_) => HttpResponse::Ok().json(json!({ "images": [BASE64_STANDARD.encode(bytes)] })),
        Err(e) => {
            log::error!("Error generating image: {e}");
            HttpResponse::InternalServerError().body(format!("Error generating image: {e}"))
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GenerateTrafficBody {
    #[serde(with = "humantime_serde")]
    pub sleep_time: Duration,
    pub response_size: u64,
}

#[post("/generate-traffic")]
async fn generate_traffic(info: web::Json<GenerateTrafficBody>) -> impl Responder {
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
            .service(text2img)
    })
    .bind(("127.0.0.1", 7861))?
    .run()
    .await?)
}

pub fn fractal(width: u32, height: u32) -> image::ImageBuffer<image::Rgb<u8>, Vec<u8>> {
    let imgx = width;
    let imgy = height;

    let scalex = 3.0 / imgx as f32;
    let scaley = 3.0 / imgy as f32;

    // Create a new ImgBuf with width: imgx and height: imgy
    let mut imgbuf = image::ImageBuffer::new(imgx, imgy);

    // Iterate over the coordinates and pixels of the image
    for (x, y, pixel) in imgbuf.enumerate_pixels_mut() {
        let r = (0.3 * x as f32) as u8;
        let b = (0.3 * y as f32) as u8;
        *pixel = image::Rgb([r, 0, b]);
    }

    // A redundant loop to demonstrate reading image data
    for x in 0..imgx {
        for y in 0..imgy {
            let cx = y as f32 * scalex - 1.5;
            let cy = x as f32 * scaley - 1.5;

            let c = num_complex::Complex::new(-0.4, 0.6);
            let mut z = num_complex::Complex::new(cx, cy);

            let mut i = 0;
            while i < 255 && z.norm() <= 2.0 {
                z = z * z + c;
                i += 1;
            }

            let pixel = imgbuf.get_pixel_mut(x, y);
            let image::Rgb(data) = *pixel;
            *pixel = image::Rgb([data[0], i as u8, data[2]]);
        }
    }

    imgbuf
}
