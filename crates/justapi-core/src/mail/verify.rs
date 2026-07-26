use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::RwLock;

/// In-memory store for email verification tokens.
/// In production, use Redis or the database instead.
pub struct VerificationStore {
    pub(crate) tokens: Arc<RwLock<HashMap<String, VerificationEntry>>>,
}

#[derive(Clone)]
pub(crate) struct VerificationEntry {
    pub email: String,
    pub purpose: String,
    pub created_at: u64,
    pub used: bool,
}

impl VerificationStore {
    pub fn new() -> Self {
        Self { tokens: Arc::new(RwLock::new(HashMap::new())) }
    }

    /// Generate a new verification token and store it.
    pub async fn create_token(&self, email: &str, purpose: &str, _ttl_secs: u64) -> String {
        use rand::Rng;
        let token: String = rand::thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();

        let mut store = self.tokens.write().await;
        store.insert(
            token.clone(),
            VerificationEntry {
                email: email.to_string(),
                purpose: purpose.to_string(),
                created_at: now,
                used: false,
            },
        );

        token
    }

    /// Verify a token: returns the email if valid, None otherwise.
    /// Consumes the token (one-time use).
    pub async fn verify(&self, token: &str, purpose: &str, max_age_secs: u64) -> Option<String> {
        let mut store = self.tokens.write().await;
        let mut entry = match store.get(token) {
            Some(e) => e.clone(),
            None => return None,
        };

        if entry.used {
            return None;
        }
        if entry.purpose != purpose {
            return None;
        }

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        if now - entry.created_at > max_age_secs {
            store.remove(token);
            return None;
        }

        let email = entry.email.clone();
        entry.used = true;
        store.insert(token.to_string(), entry);

        Some(email)
    }

    /// Clean up expired tokens from the store.
    pub async fn cleanup(&self, max_age_secs: u64) {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let mut store = self.tokens.write().await;
        store.retain(|_, entry| now - entry.created_at <= max_age_secs);
    }
}

impl Default for VerificationStore {
    fn default() -> Self {
        Self::new()
    }
}
