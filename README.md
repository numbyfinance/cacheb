# cacheb
Compile time cache busting for static assets in web applications.

## Features

- Content-based hashes ensure browsers load the latest version
- Seamless integration with templates
- Files are organized in modules matching your directories
- Compiler catches typos in file references

## Example usage

Add `cacheb` to `[build-dependencies]`.

```toml
[build-dependencies]
cacheb = "0.1"
```

Create a `build.rs` to trigger `cacheb` whenever your static files change.

```rust
fn main() {
    println!("cargo:rerun-if-changed=static/");
    cacheb::codegen(
        &PathBuf::from("src/statics.rs"),  // Path to generate file
        &[PathBuf::from("static")],        // Path(s) to static assets
        &[]                                // Additional files to use
    ).unwrap();
}
```

Reference your static assets in templates with automatic cache busting, eg. Maud:

```rust
use statics;

fn page() -> maud::Markup {
    html! {
        head {
            link rel="stylesheet" href=(statics::styles::main_css);
            link rel="icon" href=(statics::favicon_ico);
        }
        body {
            img src=(statics::images::logo_png) alt="Logo";
            script src=(statics::scripts::app_js) {}
        }
    }
}
```

Implement a handler to serve your static files, eg. Axum:

```rust
#[derive(TypedPath, Deserialize)]
#[typed_path("/static/{*path}")]
pub struct StaticFilePath {
    pub path: String,
}

pub async fn static_path(StaticFilePath { path }: StaticFilePath) -> impl IntoResponse {
    let data = StaticFile::get(&format!("/static/{}", path));

    if let Some(data) = data {
        let file = match tokio::fs::File::open(data.file_name).await {
            Ok(file) => file,
            Err(_) => {
                return Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::empty())
                    .unwrap();
            }
        };

        return Response::builder()
            .status(StatusCode::OK)
            .header(
                header::CONTENT_TYPE,
                HeaderValue::from_str(data.mime.as_ref()).unwrap(),
            )
            .body(Body::from_stream(ReaderStream::new(file)))
            .unwrap();
    }

    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::empty())
        .unwrap()
}

let app = Router::new()
    .route("/static/{*path}", get(static_path));
```
