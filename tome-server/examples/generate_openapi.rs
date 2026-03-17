//! Generate the OpenAPI spec as pretty-printed JSON on stdout.
//!
//! ```bash
//! cargo run -p tome-server --example generate_openapi 2>/dev/null > docs/schema/openapi.json
//! ```

use tome_server::openapi::ApiDoc;
use utoipa::OpenApi as _;

fn main() {
    let doc = ApiDoc::openapi();
    let json = serde_json::to_string_pretty(&doc).expect("serialize OpenAPI spec");
    println!("{json}");
}
