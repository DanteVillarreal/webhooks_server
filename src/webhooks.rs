// src/webhooks.rs

use teloxide::payloads::DeleteMyCommandsSetters;
use warp::Filter;
use crate::{WebhookPayload, handle_message_handler};
use std::env;
use teloxide::types::Message as TeloxideMessage;
use crate::telegram::convert_teloxide_message_to_custom;
use crate::Message as CustomMessage;




use std::sync::Arc;
use tokio::net::TcpListener;
use std::fs::File;

// pub async fn run_webhook_server(pool: deadpool_postgres::Pool) {
//     let openai_key = env::var("OPENAI_KEY").expect("OPENAI_KEY not set");

//     // POST /webhook
//     let webhook_route = warp::path("webhook")
//         .and(warp::post())
//         .and(warp::body::json())
//         .and_then(move |payload: WebhookPayload| {
//             let openai_key = openai_key.clone();
//             async move {
//                 if let Some(ref message) = payload.message {

                    
//                     // Store the custom message
//                     let mut messages = crate::telegram::NEW_MESSAGES.lock().await;
//                     messages.push(*message);
                
//                     // Handle the original message
//                     handle_message_handler(message.clone(), openai_key).await;
//                 }
//                 Ok::<_, warp::Rejection>(warp::reply::json(&"OK"))
//             }
//         });

//     warp::serve(webhook_route)
//         .run(([0, 0, 0, 0], 80))
//         .await;
// }




pub async fn run_webhook_server(pool: deadpool_postgres::Pool) {
    log::info!("IS CODE GETTING HEREEEEEEEEEEEEEEE");
    let openai_key = env::var("OPENAI_KEY").expect("OPENAI_KEY not set");

    // POST /webhook
    let webhook_route = warp::path("webhook")
        .and(warp::post())
        .and(warp::body::json())
        .and_then(move |payload: WebhookPayload| {
            let openai_key = openai_key.clone();
            async move {
                log::info!("Webhook called with payload: {:?}", payload);
                if let Some(ref message) = payload.message {
                    handle_message_handler(message.clone(), openai_key).await;
                }
                Ok::<_, warp::Rejection>(warp::reply::json(&"OK"))
            }
        });





//--voner webhooks filters --//



    let inbound_message = warp::path("webhooks")
        .and(warp::path("inbound-message"))
        .and(warp::post())
        .and(warp::body::json())
        .map(|body: serde_json::Value| {
            println!("Received inbound message: {:?}", body);
            warp::reply::json(&body)
    });
    // Define the message status filter
    let message_status = warp::path("webhooks")
        .and(warp::path("message-status"))
        .and(warp::post())
        .and(warp::body::json())
        .map(|body: serde_json::Value| {
            println!("Received message status: {:?}", body);
            warp::reply::json(&body)
    });

//--^^voner webhooks filters ^^--//



    // GET /
        let html = tokio::fs::read_to_string("/home/ubuntu/html_connect/index.html").await.expect("Unable to read file");
        let html_route = warp::path::end()
            .map(move || warp::reply::html(html.clone()));

    
    // Combine routes:
        //un code comment this if you just want to see if a request comes in
            let routes = 
                warp::any()
                        .and_then(handle_request)
                        .recover(handle_rejection);
        
        //let routes = inbound_message.or(html_route).or(message_status);

    // Load SSL keys and certs
        let cert_path = "/etc/letsencrypt/live/merivilla.com/fullchain.pem";
        let key_path = "/etc/letsencrypt/live/merivilla.com/privkey.pem";


    log::info!("Starting the server...");

    warp::serve(routes)
        .tls()
        .cert_path(cert_path)
        .key_path(key_path)
        .run(([0, 0, 0, 0], 443))
        .await;




}
//un code comment this if you just want to see if a request comes in
    async fn handle_rejection(err: warp::Rejection) -> Result<impl warp::Reply, std::convert::Infallible> {
        log::error!("Request was rejected: {:?}", err);
        Ok(warp::reply::with_status("Internal Server Error", warp::http::StatusCode::INTERNAL_SERVER_ERROR))
    }
    async fn handle_request() -> Result<impl warp::Reply, warp::Rejection> {
        log::info!("Received a request");
        Ok("Hello, World!")
    }