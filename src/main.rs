// src/main.rs

use tokio::join;
use dotenv::dotenv;
use std::env;
use webhooks_server::webhooks::run_webhook_server;
use webhooks_server::telegram::run_telegram_bot;

#[tokio::main]
// async fn main() {
//     dotenv().ok(); // Load environment variables from .env file
    
//     // Ensure environment variables are set
//     let _ = env::var("OPENAI_KEY").expect("OPENAI_KEY not set");
//     let _ = env::var("TELOXIDE_TOKEN").expect("TELOXIDE_TOKEN not set");

//     let webhook_server = tokio::spawn(async {
//         run_webhook_server().await;
//     });

//     let telegram_bot = tokio::spawn(async {
//         run_telegram_bot().await;
//     });

//     let _ = join!(webhook_server, telegram_bot);
// }

async fn main() {
    dotenv().ok(); // Load environment variables from .env file
    
    // Ensure environment variables are set
    let _ = env::var("OPENAI_KEY").expect("OPENAI_KEY not set");
    let _ = env::var("TELOXIDE_TOKEN").expect("TELOXIDE_TOKEN not set");

    let webhook_server = tokio::spawn(async {
        run_webhook_server().await;
    });

    let telegram_bot = tokio::spawn(async {
        run_telegram_bot().await;
    });

    let _ = join!(webhook_server, telegram_bot);
}