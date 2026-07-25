use std::time::{SystemTime, UNIX_EPOCH};

use super::verify::{VerificationEntry, VerificationStore};

impl VerificationStore {
    /// Generate a numeric OTP (6 digits) and store it.
    pub async fn create_otp(&self, email: &str, purpose: &str, _ttl_secs: u64) -> String {
        use rand::Rng;
        let otp: String = format!("{:06}", rand::thread_rng().gen_range(0..1_000_000));
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let mut store = self.tokens.write().await;
        store.insert(
            otp.clone(),
            VerificationEntry {
                email: email.to_string(),
                purpose: purpose.to_string(),
                created_at: now,
                used: false,
            },
        );
        otp
    }
}
