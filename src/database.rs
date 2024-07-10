// src/database.rs

pub async fn insert_user(pool: deadpool_postgres::Pool, user: crate::DBUser) -> Result<(), anyhow::Error> {
    // Get a client from the pool, handling the pool error explicitly
    let client = pool.get().await.map_err(|e| {
        log::error!("Failed to get client from pool: {:?}", e);
        anyhow::Error::new(e)
    })?;
    client.execute(
        "INSERT INTO users (user_id, first_name, last_name, username) VALUES ($1, $2, $3, $4)
         ON CONFLICT (user_id) DO UPDATE SET first_name = EXCLUDED.first_name, last_name = EXCLUDED.last_name, username = EXCLUDED.username",
        &[&user.id, &user.first_name, &user.last_name, &user.username]
    ).await?;
    Ok(())
}

pub async fn insert_thread(pool: deadpool_postgres::Pool, thread_id: &str, user_id: i64, openai_thread_id: &str) -> Result<(), anyhow::Error> {
    let client = pool.get().await.map_err(|e| {
        log::error!("Failed to get client from pool: {:?}", e);
        anyhow::Error::new(e)
    })?;
    
    client.execute(
        "INSERT INTO threads (thread_id, user_id, openai_thread_id) VALUES ($1, $2, $3)
         ON CONFLICT (thread_id) DO UPDATE SET user_id = EXCLUDED.user_id, openai_thread_id = EXCLUDED.openai_thread_id",
        &[&thread_id, &user_id, &openai_thread_id]
    ).await?;

    Ok(())
}

pub async fn insert_message(pool: deadpool_postgres::Pool, thread_id: &str, sender: &str, content: &str, message_type: &str) -> Result<(), anyhow::Error> {
    let client = pool.get().await.map_err(|e| {
        log::error!("Failed to get client from pool: {:?}", e);
        anyhow::Error::new(e)
    })?;
    
    client.execute(
        "INSERT INTO messages (thread_id, sender, content, message_type) VALUES ($1, $2, $3, $4)",
        &[&thread_id, &sender, &content, &message_type]
    ).await?;

    Ok(())
}

pub async fn get_thread_by_user_id(pool: deadpool_postgres::Pool, user_id: i64) -> Result<Option<String>, anyhow::Error> {
    // Get a client from the pool, handling the pool error explicitly
    let client = pool.get().await.map_err(|e| {
        log::error!("Failed to get client from pool: {:?}", e);
        anyhow::Error::new(e)
    })?;

    // Fetch the thread_id for the given user_id, if it exists
    let stmt = "SELECT thread_id FROM threads WHERE user_id = $1";
    let row = client.query_opt(stmt, &[&user_id]).await?;

    // Extract thread_id from the row, if it exists
    if let Some(row) = row {
        Ok(Some(row.get("thread_id")))
    } else {
        Ok(None)
    }
}