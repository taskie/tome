use std::sync::Arc;

use lambda_http::Error;

use tome_db::store_trait::MetadataStore;

pub async fn run_lambda(store: Arc<dyn MetadataStore>) -> Result<(), Error> {
    let app = crate::server::build_router(store);
    lambda_http::run(app).await
}
