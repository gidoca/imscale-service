use axum::{
    extract::{Path, Query},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response, Json},
    routing::get,
    Router,
};
use chrono::{DateTime, Utc};
use image::{self, imageops::FilterType, ImageDecoder, ImageReader, DynamicImage};
use serde::Deserialize;
use serde_json::json;
use std::env;
use std::fs;
use std::path::Path as FilePath;
use std::time::SystemTime;
use tower_http::services::ServeDir;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use urlencoding;

#[derive(Deserialize)]
struct ResizeParams {
    width: Option<u32>,
    height: Option<u32>,
    preserve_aspect_ratio: Option<bool>,
}

fn get_entry_type(path: &FilePath) -> &str {
    if path.is_dir() {
        return "directory";
    }

    match path.extension().and_then(|s| s.to_str()) {
        Some(ext) => match ext.to_lowercase().as_str() {
            "jpg" | "jpeg" | "png" | "gif" | "bmp" | "ico" | "tiff" | "webp" | "avif" => "image",
            _ => "file",
        },
        None => "file",
    }
}

async fn list_handler(Path(path): Path<String>) -> Result<Json<serde_json::Value>, StatusCode> {
    let base_dir = env::var("IMAGE_DIR").unwrap_or_else(|_| "images".to_string());
    // Construct the full path
    let full_path = if path.is_empty() {
        FilePath::new(&base_dir).to_path_buf()
    } else {
        let decoded_path = urlencoding::decode(&path).unwrap_or_else(|_| path.clone().into());
        FilePath::new(&base_dir).join(&*decoded_path)
    };
    
    info!("Attempting to list path: {:?}", full_path);

    // Ensure the path starts with the base directory
    if !full_path.starts_with(&base_dir) || path.starts_with(".") {
        error!("Forbidden path: {:?}", full_path);
        return Err(StatusCode::FORBIDDEN);
    }

    let metadata = match fs::metadata(&full_path) {
        Ok(m) => m,
        Err(_) => return Err(StatusCode::NOT_FOUND),
    };

    if metadata.is_dir() {
        let mut entries = Vec::new();
        for entry in fs::read_dir(&full_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
            let entry = entry.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let path = entry.path();
            let name = path.file_name().unwrap().to_str().unwrap().to_string();

            if name.starts_with(".") {
                continue;
            }

            let entry_metadata = match fs::metadata(&path) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let entry_type = get_entry_type(&path);
            let modified_time: DateTime<Utc> = entry_metadata.modified().unwrap_or(SystemTime::now()).into();
            entries.push(json!({
                "name": name,
                "type": entry_type,
                "size": entry_metadata.len(),
                "modified": modified_time.to_rfc3339(),
            }));
        }
        Ok(Json(json!(entries)))
    } else {
        let modified_time: DateTime<Utc> = metadata.modified().unwrap_or(SystemTime::now()).into();
        
        let (width, height) = match image::image_dimensions(&full_path) {
            Ok((w, h)) => (w, h),
            Err(e) => {
                error!("Failed to read image dimensions for {:?}: {}", full_path, e);
                (0, 0)
            }
        };

        let download_url = format!("/download/{}", path);

        Ok(Json(json!({
            "name": full_path.file_name().unwrap().to_str().unwrap(),
            "size": metadata.len(),
            "modified": modified_time.to_rfc3339(),
            "width": width,
            "height": height,
            "download_url": download_url,
            "type": get_entry_type(&full_path),
        })))
    }
}

async fn download_handler(Path(path): Path<String>, params: Query<ResizeParams>) -> Result<Response, StatusCode> {
    if path.split('/').any(|segment| segment.starts_with(".")) {
        error!("Forbidden path: {:?}", path);
        return Err(StatusCode::FORBIDDEN);
    }

    let base_dir = env::var("IMAGE_DIR").unwrap_or_else(|_| "images".to_string());
    let full_path = FilePath::new(&base_dir).join(&path);
    info!("Attempting to download image: {:?}", full_path);

    if !full_path.starts_with(&base_dir) {
        error!("Forbidden path: {:?}", full_path);
        return Err(StatusCode::FORBIDDEN);
    }

    let metadata = match fs::metadata(&full_path) {
        Ok(metadata) => metadata,
        Err(e) => {
            error!("Failed to get metadata for file: {:?}, error: {}", full_path, e);
            return Err(StatusCode::NOT_FOUND);
        }
    };

    let modified_time: DateTime<Utc> = metadata.modified().unwrap_or(SystemTime::now()).into();

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

    let mut decoder = match reader.into_decoder() {
        Ok(img) => img,
        Err(e) => {
            error!("Failed to decode image: {:?}, error: {}", full_path, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    let orientation = match decoder.orientation() {
        Ok(img) => img,
        Err(e) => {
            error!("Failed to decode image orientation of image: {:?}, error: {}", full_path, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    let mut img = match DynamicImage::from_decoder(decoder) {
        Ok(img) => img,
        Err(e) => {
            error!("Failed to decode image: {:?}, error: {}", full_path, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    img.apply_orientation(orientation);

    let processed_img = if let (Some(width), Some(height)) = (params.width, params.height) {
        if params.preserve_aspect_ratio.unwrap_or(false) {
            img.resize(width, height, FilterType::Lanczos3)
        } else {
            img.resize_exact(width, height, FilterType::Lanczos3)
        }
    } else {
        img
    };

    let mut buffer = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buffer);

    if let Err(e) = processed_img.write_to(&mut cursor, format) {
        error!("Failed to encode image: {:?}, error: {}", full_path, e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let content_type = match format {
        image::ImageFormat::Png => "image/png",
        image::ImageFormat::Jpeg => "image/jpeg",
        image::ImageFormat::Gif => "image/gif",
        image::ImageFormat::WebP => "image/webp",
        image::ImageFormat::Avif => "image/avif",
        image::ImageFormat::Tiff => "image/tiff",
        _ => "application/octet-stream",
    };

    let headers = {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
        headers.insert(
            header::LAST_MODIFIED,
            HeaderValue::from_str(&modified_time.to_rfc2822()).unwrap(),
        );
        headers.insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000"),
        );
        headers
    };

    info!("Successfully resized image: {:?}", full_path);
    Ok((
        headers,
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
        .route("/list/{*path}", get(list_handler))
        .route("/list/", get(|| list_handler(Path("".to_string()))))
        .route("/download/{*path}", get(download_handler))
        .fallback_service(ServeDir::new("public"));

    let port = env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    info!("Listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
