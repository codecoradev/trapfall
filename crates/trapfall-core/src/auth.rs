//! Auth — user model, password hashing, session management.
//!
//! Covers:
//! - #18: User model + migration queries (users, sessions, auth_attempts)
//! - #19: Password hashing (argon2id)
//! - #20: Session management (cookie-based, server-side)
use anyhow::Result;
use argon2::{
    password_hash::SaltString, Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version,
};
use chrono::{Duration, Utc};
use rand::rngs::OsRng;
use uuid::Uuid;

use crate::{new_id, store::Store};

// ── Constants ──────────────────────────────────────────────────────────

/// Argon2id: 19 MiB memory, 2 iterations, 1 parallelism (OWASP recommended).
fn hash_params() -> &'static Params {
    use std::sync::OnceLock;
    static PARAMS: OnceLock<Params> = OnceLock::new();
    PARAMS.get_or_init(|| Params::new(19456, 2, 1, None).unwrap_or_default())
}

/// Session duration in days.
const SESSION_DURATION_DAYS: i64 = 7;

/// Minimum password length.
const MIN_PASSWORD_LEN: usize = 8;

// ── Types ──────────────────────────────────────────────────────────────

/// User record.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: String,
    pub email: String,
    pub name: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub role: String,
    pub created_at: String,
}

/// Safe user info (no password_hash).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
    pub name: String,
    pub role: String,
    pub created_at: String,
}

impl From<User> for UserInfo {
    fn from(u: User) -> Self {
        Self { id: u.id, email: u.email, name: u.name, role: u.role, created_at: u.created_at }
    }
}

/// Session record.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct Session {
    pub id: String,
    pub user_id: String,
    pub token: String,
    pub expires_at: String,
    pub created_at: String,
}

/// Auth attempt record.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AuthAttempt {
    pub id: String,
    pub email: String,
    pub ip: String,
    pub success: bool,
    pub created_at: String,
}

// ── Password Hashing (#19) ────────────────────────────────────────────

/// Hash a password with argon2id.
pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, hash_params().clone());
    let hash = argon2.hash_password(password.as_bytes(), &salt).map_err(|e| anyhow::anyhow!("Hash failed: {e}"))?;
    Ok(hash.to_string())
}

/// Verify a password against an argon2id hash.
pub fn verify_password(password: &str, hash: &str) -> bool {
    let parsed = match PasswordHash::new(hash) {
        Ok(h) => h,
        Err(_) => return false,
    };
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, hash_params().clone());
    argon2.verify_password(password.as_bytes(), &parsed).is_ok()
}

/// Validate password strength.
pub fn validate_password(password: &str) -> Result<(), String> {
    if password.len() < MIN_PASSWORD_LEN {
        return Err(format!("Password must be at least {MIN_PASSWORD_LEN} characters"));
    }
    Ok(())
}

// ── Store Auth Extensions (#18, #20) ──────────────────────────────────

impl Store {
    // ── Users (#18) ────────────────────────────────────────────────────

    /// Check if any users exist (for setup wizard).
    pub async fn has_users(&self) -> Result<bool> {
        let row: Option<(i64,)> = sqlx::query_as("SELECT COUNT(*) FROM users").fetch_optional(self.pool()).await?;
        Ok(row.map(|(c,)| c > 0).unwrap_or(false))
    }

    /// Create a new user (admin for Solo MVP).
    pub async fn create_user(&self, email: &str, name: &str, password: &str) -> Result<User> {
        validate_password(password).map_err(|e| anyhow::anyhow!(e))?;
        let hash = hash_password(password)?;
        let id = new_id();
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO users (id, email, name, password_hash, role, created_at) VALUES (?, ?, ?, ?, 'admin', ?)",
        )
        .bind(&id)
        .bind(email)
        .bind(name)
        .bind(&hash)
        .bind(&now)
        .execute(self.pool())
        .await?;

        Ok(User {
            id,
            email: email.to_string(),
            name: name.to_string(),
            password_hash: hash,
            role: "admin".to_string(),
            created_at: now,
        })
    }

    /// Get user by email.
    pub async fn get_user_by_email(&self, email: &str) -> Result<Option<User>> {
        let user = sqlx::query_as::<_, User>(
            "SELECT id, email, name, password_hash, role, created_at FROM users WHERE email = ?",
        )
        .bind(email)
        .fetch_optional(self.pool())
        .await?;
        Ok(user)
    }

    /// Get user by ID.
    pub async fn get_user_by_id(&self, id: &str) -> Result<Option<User>> {
        let user = sqlx::query_as::<_, User>(
            "SELECT id, email, name, password_hash, role, created_at FROM users WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await?;
        Ok(user)
    }

    // ── Sessions (#20) ─────────────────────────────────────────────────

    /// Create a new session for a user.
    pub async fn create_session(&self, user_id: &str) -> Result<Session> {
        let id = new_id();
        let token = Uuid::new_v4().to_string();
        let now = Utc::now();
        let expires_at = (now + Duration::days(SESSION_DURATION_DAYS)).to_rfc3339();
        let created_at = now.to_rfc3339();

        sqlx::query("INSERT INTO sessions (id, user_id, token, expires_at, created_at) VALUES (?, ?, ?, ?, ?)")
            .bind(&id)
            .bind(user_id)
            .bind(&token)
            .bind(&expires_at)
            .bind(&created_at)
            .execute(self.pool())
            .await?;

        Ok(Session { id, user_id: user_id.to_string(), token, expires_at, created_at })
    }

    /// Get session by token (returns None if expired).
    pub async fn get_session(&self, token: &str) -> Result<Option<Session>> {
        let session = sqlx::query_as::<_, Session>(
            "SELECT id, user_id, token, expires_at, created_at FROM sessions WHERE token = ?",
        )
        .bind(token)
        .fetch_optional(self.pool())
        .await?;

        match session {
            Some(s) => {
                // Check expiry
                let expires = chrono::DateTime::parse_from_rfc3339(&s.expires_at);
                match expires {
                    Ok(exp) => {
                        if exp.to_utc() < Utc::now() {
                            // Expired — delete and return None
                            self.delete_session(&s.token).await?;
                            Ok(None)
                        } else {
                            Ok(Some(s))
                        }
                    }
                    Err(_) => Ok(None),
                }
            }
            None => Ok(None),
        }
    }

    /// Delete a session (logout).
    pub async fn delete_session(&self, token: &str) -> Result<()> {
        sqlx::query("DELETE FROM sessions WHERE token = ?").bind(token).execute(self.pool()).await?;
        Ok(())
    }

    /// Cleanup expired sessions.
    pub async fn cleanup_expired_sessions(&self) -> Result<u64> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query("DELETE FROM sessions WHERE expires_at < ?").bind(&now).execute(self.pool()).await?;
        Ok(result.rows_affected())
    }

    // ── Auth Attempts (#18 — for brute-force #23) ──────────────────────

    /// Record an auth attempt.
    pub async fn record_auth_attempt(&self, email: &str, ip: &str, success: bool) -> Result<()> {
        let id = new_id();
        let now = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO auth_attempts (id, email, ip, success, created_at) VALUES (?, ?, ?, ?, ?)")
            .bind(&id)
            .bind(email)
            .bind(ip)
            .bind(success)
            .bind(&now)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Count failed auth attempts for an email in the last N minutes.
    pub async fn count_failed_attempts_email(&self, email: &str, minutes: i64) -> Result<i64> {
        let cutoff = (Utc::now() - Duration::minutes(minutes)).to_rfc3339();
        let row: Option<(i64,)> =
            sqlx::query_as("SELECT COUNT(*) FROM auth_attempts WHERE email = ? AND success = 0 AND created_at > ?")
                .bind(email)
                .bind(&cutoff)
                .fetch_optional(self.pool())
                .await?;
        Ok(row.map(|(c,)| c).unwrap_or(0))
    }

    /// Count failed auth attempts for an IP in the last N minutes.
    pub async fn count_failed_attempts_ip(&self, ip: &str, minutes: i64) -> Result<i64> {
        let cutoff = (Utc::now() - Duration::minutes(minutes)).to_rfc3339();
        let row: Option<(i64,)> =
            sqlx::query_as("SELECT COUNT(*) FROM auth_attempts WHERE ip = ? AND success = 0 AND created_at > ?")
                .bind(ip)
                .bind(&cutoff)
                .fetch_optional(self.pool())
                .await?;
        Ok(row.map(|(c,)| c).unwrap_or(0))
    }

    /// Authenticate a user: verify password + create session.
    /// Returns (session, user_info) on success.
    pub async fn authenticate(&self, email: &str, password: &str, ip: &str) -> Result<(Session, UserInfo), AuthError> {
        // Check brute-force lockout
        let email_fails = self.count_failed_attempts_email(email, 15).await.unwrap_or(0);
        if email_fails >= 5 {
            // Record attempt even when locked out
            let _ = self.record_auth_attempt(email, ip, false).await;
            return Err(AuthError::LockedOut);
        }

        let ip_fails = self.count_failed_attempts_ip(ip, 15).await.unwrap_or(0);
        if ip_fails >= 20 {
            let _ = self.record_auth_attempt(email, ip, false).await;
            return Err(AuthError::LockedOut);
        }

        // Find user
        let user = self.get_user_by_email(email).await.map_err(|_| AuthError::Internal)?;
        let user = match user {
            Some(u) => u,
            None => {
                let _ = self.record_auth_attempt(email, ip, false).await;
                return Err(AuthError::InvalidCredentials);
            }
        };

        // Verify password
        if !verify_password(password, &user.password_hash) {
            let _ = self.record_auth_attempt(email, ip, false).await;
            return Err(AuthError::InvalidCredentials);
        }

        // Success — record + create session
        let _ = self.record_auth_attempt(email, ip, true).await;
        let session = self.create_session(&user.id).await.map_err(|_| AuthError::Internal)?;

        Ok((session, user.into()))
    }
}

// ── Error Types ────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Invalid credentials")]
    InvalidCredentials,
    #[error("Account temporarily locked")]
    LockedOut,
    #[error("Internal error")]
    Internal,
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_and_verify_password() {
        let password = "test_password_123";
        let hash = hash_password(password).unwrap();
        assert!(verify_password(password, &hash));
        assert!(!verify_password("wrong_password", &hash));
    }

    #[test]
    fn test_verify_invalid_hash() {
        assert!(!verify_password("any", "not_a_hash"));
    }

    #[test]
    fn test_validate_password_too_short() {
        assert!(validate_password("short").is_err());
        assert!(validate_password("longenough").is_ok());
    }

    #[test]
    fn test_validate_password_exact_minimum() {
        assert!(validate_password("12345678").is_ok()); // exactly 8
        assert!(validate_password("1234567").is_err()); // 7
    }

    #[tokio::test]
    async fn test_create_user_and_authenticate() {
        let pool = crate::open_pool("sqlite::memory:").await.unwrap();
        crate::run_migrations(&pool).await.unwrap();
        let store = Store::new(pool);

        // No users initially
        assert!(!store.has_users().await.unwrap());

        // Create user
        let user = store.create_user("admin@test.com", "Admin", "password123").await.unwrap();
        assert_eq!(user.email, "admin@test.com");
        assert_eq!(user.role, "admin");
        assert!(store.has_users().await.unwrap());

        // Authenticate successfully
        let (session, info) = store.authenticate("admin@test.com", "password123", "127.0.0.1").await.unwrap();
        assert_eq!(info.email, "admin@test.com");
        assert_eq!(session.user_id, user.id);

        // Get session by token
        let found = store.get_session(&session.token).await.unwrap();
        assert!(found.is_some());

        // Wrong password
        let result = store.authenticate("admin@test.com", "wrong", "127.0.0.1").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_brute_force_lockout() {
        let pool = crate::open_pool("sqlite::memory:").await.unwrap();
        crate::run_migrations(&pool).await.unwrap();
        let store = Store::new(pool);

        store.create_user("lock@test.com", "Lock Test", "password123").await.unwrap();

        // 5 failed attempts
        for _ in 0..5 {
            let _ = store.authenticate("lock@test.com", "wrong", "127.0.0.1").await;
        }

        // 6th attempt should be locked out
        let result = store.authenticate("lock@test.com", "password123", "127.0.0.1").await;
        assert!(matches!(result, Err(AuthError::LockedOut)));
    }

    #[tokio::test]
    async fn test_session_expiry_and_cleanup() {
        let pool = crate::open_pool("sqlite::memory:").await.unwrap();
        crate::run_migrations(&pool).await.unwrap();
        let store = Store::new(pool);

        store.create_user("expire@test.com", "Expire Test", "password123").await.unwrap();
        let (session, _) = store.authenticate("expire@test.com", "password123", "127.0.0.1").await.unwrap();

        // Session exists
        assert!(store.get_session(&session.token).await.unwrap().is_some());

        // Delete session (logout)
        store.delete_session(&session.token).await.unwrap();
        assert!(store.get_session(&session.token).await.unwrap().is_none());
    }
}
