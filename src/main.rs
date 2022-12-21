mod cloud_watch_logger;

use axum::{response::Html, routing::get, Router};

use std::net::SocketAddr;

use rusoto_core::Region;
use rusoto_iam::{GetUserRequest, Iam, IamClient};
use rusoto_logs::CloudWatchLogsClient;

use crate::cloud_watch_logger::CloudWatchLogger;

#[macro_use]
extern crate log;

#[tokio::main]
async fn main() {
    std::env::set_var("RUST_LOG", "info");
    let iam_client = IamClient::new(Region::UsEast1);

    let client = CloudWatchLogsClient::new(rusoto_signature::region::Region::ApNortheast1);
    let logger = CloudWatchLogger::new(client);

    log::set_boxed_logger(Box::new(logger.build())).unwrap();
    let filter = log::LevelFilter::Info;
    log::set_max_level(filter);

    let user = iam_client
        .get_user(GetUserRequest { user_name: None })
        .await
        .expect("should get user");

    info!("{:?}", user.user.user_name);

    let app = Router::new().route("/", get(handler));

    let addr = SocketAddr::from(([127, 0, 0, 1], 3001));
    println!("listening on {}", addr);
    trace!("Commencing yak shaving");

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn handler() -> Html<&'static str> {
    debug!("aaa");
    trace!("trace");
    info!("Info");
    warn!("Warn");
    error!("error");
    println!("printIn");
    Html("<h1>Hello, World!</h1>")
}
