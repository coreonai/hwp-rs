use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
use hwp_rs::editor_doc::{editor_doc_to_hwp_doc, hwp_doc_to_editor_doc, EditorDoc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

struct AppState {
    document: Mutex<hwp_rs::model::HwpDocument>,
    input_path: PathBuf,
    title: String,
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: hwp_edit <file.hwp|file.hwpx>");
        std::process::exit(1);
    }

    let path = PathBuf::from(&args[1]);
    if !path.exists() {
        eprintln!("File not found: {}", path.display());
        std::process::exit(1);
    }

    eprintln!("Parsing {}…", path.display());
    let doc = match hwp_rs::parse(&path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to parse: {}", e);
            std::process::exit(1);
        }
    };

    let title = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    eprintln!(
        "Loaded: {} sections, {} paragraphs",
        doc.sections.len(),
        doc.sections.iter().map(|s| s.paragraphs.len()).sum::<usize>()
    );

    let state = Arc::new(AppState {
        document: Mutex::new(doc),
        input_path: path,
        title,
    });

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/api/document", get(document_handler))
        .route("/api/image/{id}", get(image_handler))
        .route("/api/save", post(save_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind");
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{}", addr);

    eprintln!("Editor running at {}", url);

    if let Err(e) = open::that(&url) {
        eprintln!("Could not open browser: {}. Open {} manually.", e, url);
    }

    axum::serve(listener, app).await.unwrap();
}

async fn index_handler() -> Html<&'static str> {
    Html(include_str!("../../assets/editor.html"))
}

async fn document_handler(State(state): State<Arc<AppState>>) -> Json<EditorDoc> {
    let doc = state.document.lock().unwrap();
    Json(hwp_doc_to_editor_doc(&doc, &state.title))
}

async fn image_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u16>,
) -> impl IntoResponse {
    let doc = state.document.lock().unwrap();
    let index = id.saturating_sub(1) as usize;

    if let Some(entry) = doc.bin_data.get(index) {
        let mime = mime_guess::from_path(&entry.name)
            .first_or_octet_stream()
            .to_string();
        let data = entry.data.clone();
        drop(doc);
        (StatusCode::OK, [(header::CONTENT_TYPE, mime)], data).into_response()
    } else {
        drop(doc);
        (StatusCode::NOT_FOUND, "Image not found").into_response()
    }
}

#[derive(Deserialize)]
struct SaveRequest {
    editor_doc: EditorDoc,
}

#[derive(Serialize)]
struct SaveResponse {
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

async fn save_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SaveRequest>,
) -> impl IntoResponse {
    let out_path = state.input_path.with_extension("hwpx");

    let result = {
        let original = state.document.lock().unwrap();
        let updated = editor_doc_to_hwp_doc(&body.editor_doc, &original);
        drop(original);

        // Write HWPX
        let file = match std::fs::File::create(&out_path) {
            Ok(f) => f,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(SaveResponse {
                        path: out_path.display().to_string(),
                        error: Some(e.to_string()),
                    }),
                );
            }
        };

        match hwp_rs::hwpx_writer::write_hwpx(&updated, file) {
            Ok(()) => {
                // Update stored document
                *state.document.lock().unwrap() = updated;
                Ok(())
            }
            Err(e) => Err(e.to_string()),
        }
    };

    match result {
        Ok(()) => {
            eprintln!("Saved to {}", out_path.display());
            (
                StatusCode::OK,
                Json(SaveResponse {
                    path: out_path.display().to_string(),
                    error: None,
                }),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SaveResponse {
                path: out_path.display().to_string(),
                error: Some(e),
            }),
        ),
    }
}
