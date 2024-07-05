// src/main.rs

use tokio::join;
use dotenv::dotenv;
use std::env;
use webhooks_server::webhooks::run_webhook_server;
use webhooks_server::telegram::run_telegram_bot;



use std::fs;
//use std::io::Write;
use log::info;
use log4rs::{
    append::file::FileAppender, config::{Appender, Config, Root}, encode::pattern::PatternEncoder,
};
#[tokio::main]
async fn main() {
    dotenv().ok(); // Load environment variables from .env file
    
    let log_file_path = env::var("LOG_FILE_PATH").unwrap_or_else(|_| "logs/webhooks_server.log".to_string());

    // Ensure the logs directory exists
    fs::create_dir_all("logs").expect("Failed to create logs directory");

    // Configure file logging
    let logfile = FileAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{d} - {l} - {m}{n}")))
        .build(log_file_path)
        .unwrap();

    // Build the logger configuration
    let config = Config::builder()
        .appender(Appender::builder().build("logfile", Box::new(logfile)))
        .build(Root::builder().appender("logfile").build(log::LevelFilter::Info))
        .unwrap();

    // Initialize logger
    log4rs::init_config(config).unwrap();

    info!("Logging started");

    // Ensure environment variables are set
    let _openai_key = env::var("OPENAI_KEY").expect("OPENAI_KEY not set");
    let _teloxide_token = env::var("TELOXIDE_TOKEN").expect("TELOXIDE_TOKEN not set");

    info!("Environment variables loaded");
    info!("Starting webhook server and telegram bot...");

    let webhook_server = tokio::spawn(async {
        run_webhook_server().await;
        info!("Webhook server started");
    });

    let telegram_bot = tokio::spawn(async {
        run_telegram_bot().await;
        info!("Telegram bot started");
    });

    let _ = join!(webhook_server, telegram_bot);
}