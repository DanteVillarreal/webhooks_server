
use warp::Filter;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
struct WebhookPayload {
    event: String,
    data: String,
}




#[tokio::main]
async fn main() {
    // POST /webhook
    let webhook_route = warp::path("webhook")
        .and(warp::post())
        .and(warp::body::json())
        .map(|payload: WebhookPayload| {
            println!("Received webhook: {:?}", payload);
            warp::reply::json(&payload)
        });

    warp::serve(webhook_route)
        .run(([127, 0, 0, 1], 3030))
        .await;
}