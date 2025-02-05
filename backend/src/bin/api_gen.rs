use std::fs;
use utoipa::OpenApi;

fn main() {
    let doc = backend::api::ApiDoc::openapi().to_pretty_json().unwrap();
    fs::write("./clients/backend_openapi.json", doc).unwrap();
}
