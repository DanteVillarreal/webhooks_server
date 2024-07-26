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




    // GET /
        let html = tokio::fs::read_to_string("/home/ubuntu/html_connect/index.html").await.expect("Unable to read file");
        let html_route = warp::path::end()
            .map(move || warp::reply::html(html.clone()));

    
    // Combine routes:
        let routes = warp::any().map(|| "Hello, World!");
        //let routes = webhook_route.or(html_route);

    // Load SSL keys and certs
        let cert_path = "/etc/letsencrypt/live/merivilla.com/fullchain.pem";
        let key_path = "/home/ubuntu/new_certs/pkcs8.key";

    // Read the cert and private key file into memory
        let cert_contents = std::fs::read(cert_path).expect("failed to read cert file");
        let key_contents = std::fs::read(key_path).expect("Failed to read private key file");


    // warp::serve(routes)
    // .run(([0, 0, 0, 0], 80))
    // .await;


    warp::serve(routes)
    .tls()
    .cert(cert_path)
    .key(key_path)
    .run(([0, 0, 0, 0], 443))
    .await;



    //     let cert_file = &mut std::io::BufReader::new(std::fs::File::open(cert_path).expect("Certificate file not found"));
    //     let key_file = &mut std::io::BufReader::new(std::fs::File::open(key_path).expect("Key file not found"));
    
    

    //     let cert_chain = rustls_pemfile::certs(cert_file)
    //     .filter_map(Result::ok)
    //     .map(rustls::Certificate)
    //     .collect::<Vec<_>>();

    // let mut keys = rustls_pemfile::pkcs8_private_keys(key_file)
    //     .filter_map(Result::ok)
    //     .map(rustls::PrivateKey)
    //     .collect::<Vec<_>>();

    
    
    //     if keys.is_empty() {
    //         panic!("No valid private keys found!");
    //     }

    //     let config = rustls::ServerConfig::builder()
    //     .with_safe_defaults()
    //     .with_no_client_auth()
    //     .with_single_cert(cert_chain, keys.remove(0))
    //     .expect("Failed to create config");

    //     let config = Arc::new(config);

    // // Configure warp to run with TLS
    //     let tls = warp::tls()
    //         .cert_path(cert_path)
    //         .key_path(key_path);

    //     //listening to port 443 because 80 is for http, 443 is standard for https
    //     let listener = TcpListener::bind("0.0.0.0:443").await.expect("TCP listener failed to bind");



    // // Starts the warp server    
    // warp::serve(webhook_route)
    //     .tls(tls)
    //     .run_incoming(TlsConfigBuilder::from(config).context(warp::service(webhook_route.into_service())).std_listener(listener))
    //     .await;

}

