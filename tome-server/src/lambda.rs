use lambda_http::Error;
use sea_orm::DatabaseConnection;

pub async fn run_lambda(db: DatabaseConnection) -> Result<(), Error> {
    let app = crate::server::build_router(db);
    lambda_http::run(app).await
}
