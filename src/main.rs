use axum::{
    extract::{Path, Query},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use image::{self, imageops::FilterType, ImageReader};
use serde::Deserialize;
use std::env;
use std::path::Path as FilePath;
use tower_http::services::ServeDir;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Deserialize)]
struct ResizeParams {
    width: u32,
    height: u32,
    preserve_aspect_ratio: Option<bool>,
}

async fn resize_image(
    Path(image_path): Path<String>,
    Query(params): Query<ResizeParams>,
) -> Result<Response, StatusCode> {
    let base_dir = env::var("IMAGE_DIR").unwrap_or_else(|_| "images".to_string());
    let full_path = FilePath::new(&base_dir).join(&image_path);
    info!("Attempting to resize image: {:?}", full_path);

    if !full_path.starts_with(&base_dir) {
        error!("Forbidden path: {:?}", full_path);
        return Err(StatusCode::FORBIDDEN);
    }

    let reader = match ImageReader::open(&full_path) {
        Ok(reader) => reader,
        Err(e) => {
            error!("Failed to open image file: {:?}, error: {}", full_path, e);
            return Err(StatusCode::NOT_FOUND);
        }
    };

    let reader = match reader.with_guessed_format() {
        Ok(reader) => reader,
        Err(e) => {
            error!("Failed to guess image format: {:?}, error: {}", full_path, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let format = reader.format().unwrap_or(image::ImageFormat::Png);

    let img = match reader.decode() {
        Ok(img) => img,
        Err(e) => {
            error!("Failed to decode image: {:?}, error: {}", full_path, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let resized_img = if params.preserve_aspect_ratio.unwrap_or(false) {
        img.resize(params.width, params.height, FilterType::Lanczos3)
    } else {
        img.resize_exact(params.width, params.height, FilterType::Lanczos3)
    };

    let mut buffer = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buffer);

    if let Err(e) = resized_img.write_to(&mut cursor, format) {
        error!("Failed to encode image: {:?}, error: {}", full_path, e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let content_type = match format {
        image::ImageFormat::Png => "image/png",
        image::ImageFormat::Jpeg => "image/jpeg",
        image::ImageFormat::Gif => "image/gif",
        _ => "application/octet-stream",
    };

    info!("Successfully resized image: {:?}", full_path);
    Ok((
        [(header::CONTENT_TYPE, content_type)],
        buffer,
    )
        .into_response())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "imscale_service=info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let app = Router::new()
        .route("/images/{image_path}", get(resize_image))
        .fallback_service(ServeDir::new("public"));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    info!("Listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
