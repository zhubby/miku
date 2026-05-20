use axum::body::Body;
use axum::http::header::CONTENT_TYPE;
use axum::http::{Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "web-dist"]
struct WebAssets;

pub(crate) async fn serve(method: Method, uri: Uri) -> Response {
    if !matches!(method, Method::GET | Method::HEAD) {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }

    let path = uri.path();
    if path.starts_with("/api/") || path == "/api" {
        return StatusCode::NOT_FOUND.into_response();
    }

    let asset_path = asset_path(path);
    let fallback_to_index = asset_path.is_none();
    let asset_path = asset_path.unwrap_or("index.html");

    match WebAssets::get(asset_path).or_else(|| {
        fallback_to_index
            .then(|| WebAssets::get("index.html"))
            .flatten()
    }) {
        Some(asset) => asset_response(method, asset_path, asset.data.into_owned()),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn asset_path(path: &str) -> Option<&str> {
    let trimmed = path.trim_start_matches('/');
    if trimmed.is_empty() {
        return Some("index.html");
    }
    if trimmed.contains("..") {
        return None;
    }
    if trimmed
        .rsplit('/')
        .next()
        .is_some_and(|segment| segment.contains('.'))
    {
        Some(trimmed)
    } else {
        None
    }
}

fn asset_response(method: Method, path: &str, data: Vec<u8>) -> Response {
    let content_type = mime_guess::from_path(path)
        .first_or_octet_stream()
        .essence_str()
        .to_owned();
    let body = if method == Method::HEAD {
        Body::empty()
    } else {
        Body::from(data)
    };

    ([(CONTENT_TYPE, content_type)], body).into_response()
}

#[cfg(test)]
mod tests {
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::router;
    use crate::test_support::DummyServices;

    #[tokio::test]
    async fn root_serves_embedded_index_html() {
        let response = router(std::sync::Arc::new(DummyServices))
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-type"], "text/html");

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert!(std::str::from_utf8(&body).unwrap().contains("miku-canvas"));
    }

    #[tokio::test]
    async fn wasm_asset_uses_wasm_content_type() {
        let response = router(std::sync::Arc::new(DummyServices))
            .oneshot(
                Request::builder()
                    .uri("/miku_web_bg.wasm")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-type"], "application/wasm");
    }

    #[tokio::test]
    async fn unknown_non_api_path_falls_back_to_index_html() {
        let response = router(std::sync::Arc::new(DummyServices))
            .oneshot(
                Request::builder()
                    .uri("/clusters/local/workloads")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert!(std::str::from_utf8(&body).unwrap().contains("miku-canvas"));
    }

    #[tokio::test]
    async fn unknown_api_path_does_not_fall_back_to_index_html() {
        let response = router(std::sync::Arc::new(DummyServices))
            .oneshot(
                Request::builder()
                    .uri("/api/unknown")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
