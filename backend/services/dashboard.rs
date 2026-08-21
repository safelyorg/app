use sqlx::{Error, Pool, Postgres, query, query_scalar};
use uuid::Uuid;

/// It disconnects a user's Google account from their Safely account
/// the exact opposite of link_google_account.
///
/// It updates the user's row, clearing their Google connection and returns success,
/// with nothing meaningful inside it
pub async fn unlink_google_account(pool: &Pool<Postgres>, user_id: Uuid) -> Result<(), Error> {
    query("UPDATE users SET google_id = NULL WHERE id = $1")
        .bind(user_id)
        .execute(pool)
        .await?;

    Ok(())
}

/// When someone deletes their account, this removes everything that's genuinely,
/// only theirs — but keeps their fraud reports and analyses around (just disconnected from their name),
/// so deleting your account doesn't quietly weaken protection for everyone else.
///
/// It starts a transaction, gets their email first, since it's needed later,
/// disconnects their analyses from their identity, without deleting them and
/// asks does the exact same thing for their fraud reports, genuinely deletes
/// their sessions and deletes their magic links, matched by email, not user ID.
/// It finally deletes the actual account row itself and commits by making
/// everything above genuinely permanent, all at once.
pub async fn delete_user_account(pool: &Pool<Postgres>, user_id: Uuid) -> Result<(), Error> {
    let mut tx = pool.begin().await?;

    let email: String = query_scalar("SELECT email FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await?;

    query("UPDATE analysis SET user_id = NULL WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    query("UPDATE fraud_reports SET user_id = NULL WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    query("DELETE FROM sessions WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    query("DELETE FROM magic_links WHERE email = $1")
        .bind(&email)
        .execute(&mut *tx)
        .await?;

    query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(())
}
