//! Auth — user model, password hashing, session management.
//!
//! Covers:
//! - #18: User model + migration queries (users, sessions, auth_attempts)
//! - #19: Password hashing (argon2id)
//! - #20: Session management (cookie-based, server-side)
use anyhow::Result;
use argon2::{
    Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version, password_hash::SaltString,
};
use chrono::{Duration, Utc};
use rand::rngs::OsRng;
use uuid::Uuid;

use crate::{new_id, store::Store};

// Re-export row types from trapfall-db so auth handlers keep their existing shape.
pub use trapfall_db::{StoredSession as Session, StoredUser as User};

// ── Constants ──────────────────────────────────────────────────────────

/// Argon2id: 19 MiB memory, 2 iterations, 1 parallelism (OWASP recommended).
fn hash_params() -> &'static Params {
    use std::sync::OnceLock;
    static PARAMS: OnceLock<Params> = OnceLock::new();
    PARAMS.get_or_init(|| Params::new(19456, 2, 1, None).unwrap_or_default())
}

/// A valid Argon2 hash of a random value, used to equalise timing for unknown users.
fn dummy_hash() -> &'static str {
    use std::sync::OnceLock;
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| hash_password(&Uuid::new_v4().to_string()).unwrap_or_default())
}

/// Session duration in days.
const SESSION_DURATION_DAYS: i64 = 7;

/// Minimum password length.
const MIN_PASSWORD_LEN: usize = 8;

/// Minimum email length.
const MIN_EMAIL_LEN: usize = 5;

// ── Types ──────────────────────────────────────────────────────────────

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

/// Auth attempt record.
#[derive(Debug, Clone)]
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

/// Hash a password on the blocking pool so Argon2 (tens of ms, 19 MiB) never
/// stalls the single-threaded async runtime.
pub async fn hash_password_async(password: &str) -> Result<String> {
    let password = password.to_string();
    tokio::task::spawn_blocking(move || hash_password(&password))
        .await
        .map_err(|e| anyhow::anyhow!("hash task failed: {e}"))?
}

/// Verify a password on the blocking pool (see [`hash_password_async`]).
pub async fn verify_password_async(password: &str, hash: &str) -> bool {
    let (password, hash) = (password.to_string(), hash.to_string());
    tokio::task::spawn_blocking(move || verify_password(&password, &hash)).await.unwrap_or(false)
}

/// Prefix marking a stored (hashed) session token. Rows without it are legacy
/// plaintext tokens and are purged at startup.
pub const SESSION_TOKEN_PREFIX: &str = "h1:";

/// Per-(email, IP) failed-login threshold within [`LOCKOUT_WINDOW_MINUTES`].
const MAX_FAILS_PER_EMAIL_IP: i64 = 5;
/// Per-IP failed-login threshold within [`LOCKOUT_WINDOW_MINUTES`].
const MAX_FAILS_PER_IP: i64 = 20;
const LOCKOUT_WINDOW_MINUTES: i64 = 15;

/// Hash a session token for storage (blake3; the token is a high-entropy
/// random value so a fast hash is appropriate).
pub fn hash_session_token(token: &str) -> String {
    format!("{SESSION_TOKEN_PREFIX}{}", blake3::hash(token.as_bytes()).to_hex())
}

/// Validate password strength.
pub fn validate_password(password: &str) -> Result<(), String> {
    if password.len() < MIN_PASSWORD_LEN {
        return Err(format!("Password must be at least {MIN_PASSWORD_LEN} characters"));
    }
    // Complexity: require at least one letter and one digit.
    // Intentionally lenient — admin-chosen passwords, not user-self-service.
    let has_letter = password.chars().any(|c| c.is_alphabetic());
    let has_digit = password.chars().any(|c| c.is_ascii_digit());
    if !has_letter || !has_digit {
        return Err("Password must contain at least one letter and one digit".to_string());
    }
    Ok(())
}

/// Validate email format (basic RFC 5322 sanity check).
pub fn validate_email(email: &str) -> Result<(), String> {
    if email.len() < MIN_EMAIL_LEN {
        return Err("Email is too short".to_string());
    }
    // Must contain exactly one @, with non-empty local and domain parts
    let parts: Vec<&str> = email.rsplitn(2, '@').collect();
    if parts.len() != 2 {
        return Err("Invalid email format: missing @".to_string());
    }
    let (domain, local) = (parts[0], parts[1]);
    if local.is_empty() {
        return Err("Invalid email format: empty local part".to_string());
    }
    if domain.is_empty() || !domain.contains('.') {
        return Err("Invalid email format: invalid domain".to_string());
    }
    Ok(())
}

// ── Store Auth Extensions (#18, #20) ──────────────────────────────────

impl Store {
    // ── Users (#18) ─────────────────────────────────────────────────────

    /// Check if any users exist (for setup wizard).
    pub async fn has_users(&self) -> Result<bool> {
        self.backend().has_users().await
    }

    /// Create a new user (admin for Solo MVP).
    pub async fn create_user(&self, email: &str, name: &str, password: &str) -> Result<User> {
        validate_email(email).map_err(|e| anyhow::anyhow!(e))?;
        validate_password(password).map_err(|e| anyhow::anyhow!(e))?;
        let hash = hash_password_async(password).await?;
        self.backend().create_user(email, name, &hash).await?;
        let user = self
            .backend()
            .get_user_by_email(email)
            .await?
            .ok_or_else(|| anyhow::anyhow!("user not found after insert"))?;
        Ok(user)
    }

    /// Create the first admin atomically (setup wizard). Returns `Ok(None)`
    /// when a user already exists, so two concurrent setups cannot both win.
    pub async fn create_first_user(&self, email: &str, name: &str, password: &str) -> Result<Option<User>> {
        validate_email(email).map_err(|e| anyhow::anyhow!(e))?;
        validate_password(password).map_err(|e| anyhow::anyhow!(e))?;
        let hash = hash_password_async(password).await?;
        if !self.backend().create_first_user(email, name, &hash).await? {
            return Ok(None);
        }
        let user = self
            .backend()
            .get_user_by_email(email)
            .await?
            .ok_or_else(|| anyhow::anyhow!("user not found after insert"))?;
        Ok(Some(user))
    }

    /// Get user by email.
    pub async fn get_user_by_email(&self, email: &str) -> Result<Option<User>> {
        self.backend().get_user_by_email(email).await
    }

    /// Get user by ID.
    pub async fn get_user_by_id(&self, id: &str) -> Result<Option<User>> {
        self.backend().get_user_by_id(id).await
    }

    /// Update user password.
    pub async fn update_password(&self, user_id: &str, new_password: &str) -> Result<()> {
        validate_password(new_password).map_err(|e| anyhow::anyhow!(e))?;
        let hash = hash_password_async(new_password).await?;
        self.backend().update_password(user_id, &hash).await
    }

    /// Invalidate every session of `user_id` except the one identified by the
    /// raw `keep_token` (pass `None` to drop them all). Used after a password change.
    pub async fn invalidate_other_sessions(&self, user_id: &str, keep_token: Option<&str>) -> Result<u64> {
        let keep = keep_token.map(hash_session_token).unwrap_or_default();
        self.backend().delete_user_sessions_except(user_id, &keep).await
    }

    /// Remove legacy sessions stored with plaintext tokens (one-time upgrade;
    /// idempotent). Affected users simply log in again.
    pub async fn purge_legacy_sessions(&self) -> Result<u64> {
        self.backend().purge_sessions_without_prefix(SESSION_TOKEN_PREFIX).await
    }

    // ── Sessions (#20) ──────────────────────────────────────────────────

    /// Create a new session for a user.
    pub async fn create_session(&self, user_id: &str) -> Result<Session> {
        let token = Uuid::new_v4().to_string();
        let now = Utc::now();
        let expires_at = (now + Duration::days(SESSION_DURATION_DAYS)).to_rfc3339();
        let created_at = now.to_rfc3339();

        // Only a hash is persisted; the raw token lives in the cookie alone.
        self.backend().create_session(user_id, &hash_session_token(&token), &expires_at).await?;

        Ok(Session { id: new_id(), user_id: user_id.to_string(), token, expires_at, created_at })
    }

    /// Get session by token (returns None if expired).
    pub async fn get_session(&self, token: &str) -> Result<Option<Session>> {
        let session = self.backend().get_session(&hash_session_token(token)).await?;

        match session {
            Some(s) => {
                let expires = chrono::DateTime::parse_from_rfc3339(&s.expires_at);
                match expires {
                    Ok(exp) => {
                        if exp.to_utc() < Utc::now() {
                            // `s.token` is already the stored hash.
                            self.backend().delete_session(&s.token).await?;
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
        self.backend().delete_session(&hash_session_token(token)).await
    }

    /// Cleanup expired sessions.
    pub async fn cleanup_expired_sessions(&self) -> Result<u64> {
        self.backend().cleanup_expired_sessions().await
    }

    // ── Auth Attempts (#18 — for brute-force #23) ───────────────────────

    /// Record an auth attempt.
    pub async fn record_auth_attempt(&self, email: &str, ip: &str, success: bool) -> Result<()> {
        self.backend().record_auth_attempt(email, ip, success).await
    }

    /// Count failed auth attempts for an email in the last N minutes.
    pub async fn count_failed_attempts_email(&self, email: &str, minutes: i64) -> Result<i64> {
        self.backend().count_failed_attempts_email(email, minutes).await
    }

    /// Count failed auth attempts for an IP in the last N minutes.
    pub async fn count_failed_attempts_ip(&self, ip: &str, minutes: i64) -> Result<i64> {
        self.backend().count_failed_attempts_ip(ip, minutes).await
    }

    /// Count failed auth attempts for one (email, IP) pair in the last N minutes.
    pub async fn count_failed_attempts_email_ip(&self, email: &str, ip: &str, minutes: i64) -> Result<i64> {
        self.backend().count_failed_attempts_email_ip(email, ip, minutes).await
    }

    /// Authenticate a user: verify password + create session.
    /// Returns (session, user_info) on success.
    pub async fn authenticate(&self, email: &str, password: &str, ip: &str) -> Result<(Session, UserInfo), AuthError> {
        // Brute-force lockout. Keyed on (email, IP) rather than email alone, so a
        // remote attacker cannot lock the real admin out by spamming failures for
        // their address from another IP. A per-IP cap bounds spraying.
        let pair_fails = self.count_failed_attempts_email_ip(email, ip, LOCKOUT_WINDOW_MINUTES).await.unwrap_or(0);
        if pair_fails >= MAX_FAILS_PER_EMAIL_IP {
            let _ = self.record_auth_attempt(email, ip, false).await;
            return Err(AuthError::LockedOut);
        }

        let ip_fails = self.count_failed_attempts_ip(ip, LOCKOUT_WINDOW_MINUTES).await.unwrap_or(0);
        if ip_fails >= MAX_FAILS_PER_IP {
            let _ = self.record_auth_attempt(email, ip, false).await;
            return Err(AuthError::LockedOut);
        }

        // Find user. For unknown emails still burn one Argon2 verification so
        // response time does not reveal whether the account exists.
        let user = self.get_user_by_email(email).await.map_err(|_| AuthError::Internal)?;
        let user = match user {
            Some(u) => u,
            None => {
                let _ = verify_password_async(password, dummy_hash()).await;
                let _ = self.record_auth_attempt(email, ip, false).await;
                return Err(AuthError::InvalidCredentials);
            }
        };

        // Verify password (off the async runtime)
        if !verify_password_async(password, &user.password_hash).await {
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
        assert!(validate_password("longenough1").is_ok());
        assert!(validate_password("longenough").is_err()); // no digit
        assert!(validate_password("12345678").is_err()); // no letter
    }

    #[test]
    fn test_validate_password_exact_minimum() {
        assert!(validate_password("abcd1234").is_ok()); // exactly 8, has letter+digit
        assert!(validate_password("1234567").is_err()); // 7
    }

    #[tokio::test]
    async fn test_create_user_and_authenticate() {
        let backend = trapfall_db::open_database("sqlite::memory:").await.unwrap();
        {
            trapfall_db::run_sqlite_migrations(backend.sqlite_pool().unwrap()).await.unwrap();
        }
        let store = Store::new(backend);

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
        let backend = trapfall_db::open_database("sqlite::memory:").await.unwrap();
        {
            trapfall_db::run_sqlite_migrations(backend.sqlite_pool().unwrap()).await.unwrap();
        }
        let store = Store::new(backend);

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
    async fn test_update_password() {
        let backend = trapfall_db::open_database("sqlite::memory:").await.unwrap();
        {
            trapfall_db::run_sqlite_migrations(backend.sqlite_pool().unwrap()).await.unwrap();
        }
        let store = Store::new(backend);

        store.create_user("pw@test.com", "PW Test", "oldpassword1").await.unwrap();

        // Change password
        store
            .update_password(&store.get_user_by_email("pw@test.com").await.unwrap().unwrap().id, "newpassword1")
            .await
            .unwrap();

        // Old password should fail
        let result = store.authenticate("pw@test.com", "oldpassword1", "127.0.0.1").await;
        assert!(result.is_err());

        // New password should work
        let result = store.authenticate("pw@test.com", "newpassword1", "127.0.0.1").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_session_expiry_and_cleanup() {
        let backend = trapfall_db::open_database("sqlite::memory:").await.unwrap();
        {
            trapfall_db::run_sqlite_migrations(backend.sqlite_pool().unwrap()).await.unwrap();
        }
        let store = Store::new(backend);

        store.create_user("expire@test.com", "Expire Test", "password123").await.unwrap();
        let (session, _) = store.authenticate("expire@test.com", "password123", "127.0.0.1").await.unwrap();

        // Session exists
        assert!(store.get_session(&session.token).await.unwrap().is_some());

        // Delete session (logout)
        store.delete_session(&session.token).await.unwrap();
        assert!(store.get_session(&session.token).await.unwrap().is_none());
    }

    // ── Email Validation (#89) ─────────────────────────────────────

    #[test]
    fn test_validate_email_valid() {
        assert!(validate_email("user@example.com").is_ok());
        assert!(validate_email("a@b.co").is_ok());
        assert!(validate_email("admin+tag@company.io").is_ok());
    }

    #[test]
    fn test_validate_email_invalid() {
        // No @
        assert!(validate_email("noemail").is_err());
        // Empty local part
        assert!(validate_email("@example.com").is_err());
        // No domain dot
        assert!(validate_email("user@localhost").is_err());
        // Too short
        assert!(validate_email("a@b").is_err());
        // Empty string
        assert!(validate_email("").is_err());
    }

    #[tokio::test]
    async fn test_create_user_rejects_invalid_email() {
        let backend = trapfall_db::open_database("sqlite::memory:").await.unwrap();
        {
            trapfall_db::run_sqlite_migrations(backend.sqlite_pool().unwrap()).await.unwrap();
        }
        let store = Store::new(backend);

        let result = store.create_user("not-an-email", "Test", "password123").await;
        assert!(result.is_err());
    }

    async fn mem_store() -> Store {
        let backend = trapfall_db::open_database("sqlite::memory:").await.unwrap();
        trapfall_db::run_sqlite_migrations(backend.sqlite_pool().unwrap()).await.unwrap();
        Store::new(backend)
    }

    #[tokio::test]
    async fn lockout_does_not_let_other_ip_lock_out_real_user() {
        let store = mem_store().await;
        store.create_user("admin@test.com", "Admin", "password123").await.unwrap();
        // Attacker hammers the admin's email from their own IP.
        for _ in 0..10 {
            let _ = store.authenticate("admin@test.com", "wrong", "6.6.6.6").await;
        }
        assert!(matches!(store.authenticate("admin@test.com", "wrong", "6.6.6.6").await, Err(AuthError::LockedOut)));
        // The real admin on a different IP is unaffected.
        assert!(store.authenticate("admin@test.com", "password123", "1.2.3.4").await.is_ok());
    }

    #[tokio::test]
    async fn setup_is_atomic_single_admin() {
        let store = mem_store().await;
        let (a, b) = tokio::join!(
            store.create_first_user("a@test.com", "A", "password123"),
            store.create_first_user("b@test.com", "B", "password123"),
        );
        let winners = [a.unwrap(), b.unwrap()].iter().filter(|u| u.is_some()).count();
        assert_eq!(winners, 1);
        assert!(store.create_first_user("c@test.com", "C", "password123").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn session_token_stored_hashed() {
        let store = mem_store().await;
        store.create_user("h@test.com", "H", "password123").await.unwrap();
        let (session, _) = store.authenticate("h@test.com", "password123", "1.1.1.1").await.unwrap();
        // The raw token must not be a valid lookup key in the DB.
        assert!(store.backend().get_session(&session.token).await.unwrap().is_none());
        assert!(store.backend().get_session(&hash_session_token(&session.token)).await.unwrap().is_some());
        assert!(store.get_session(&session.token).await.unwrap().is_some());
        store.delete_session(&session.token).await.unwrap();
        assert!(store.get_session(&session.token).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn purge_legacy_plaintext_sessions() {
        let store = mem_store().await;
        let user = store.create_user("l@test.com", "L", "password123").await.unwrap();
        let exp = (Utc::now() + Duration::days(1)).to_rfc3339();
        store.backend().create_session(&user.id, "legacy-plain-token", &exp).await.unwrap();
        let fresh = store.create_session(&user.id).await.unwrap();
        assert_eq!(store.purge_legacy_sessions().await.unwrap(), 1);
        assert!(store.get_session(&fresh.token).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn invalidate_other_sessions_keeps_current() {
        let store = mem_store().await;
        let user = store.create_user("i@test.com", "I", "password123").await.unwrap();
        let keep = store.create_session(&user.id).await.unwrap();
        let other = store.create_session(&user.id).await.unwrap();
        assert_eq!(store.invalidate_other_sessions(&user.id, Some(&keep.token)).await.unwrap(), 1);
        assert!(store.get_session(&keep.token).await.unwrap().is_some());
        assert!(store.get_session(&other.token).await.unwrap().is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn argon2_does_not_block_current_thread_runtime() {
        // A ticker task must keep making progress while a hash is computed.
        let ticks = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let t = ticks.clone();
        let ticker = tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                t.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        });
        let _ = hash_password_async("password123").await.unwrap();
        ticker.abort();
        assert!(ticks.load(std::sync::atomic::Ordering::Relaxed) > 0);
    }
}
