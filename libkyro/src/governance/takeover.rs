use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

static IS_TAKEOVER_ACTIVE: AtomicBool = AtomicBool::new(false);
static TAKEOVER_REASON: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::new()));
static TAKEOVER_TIMESTAMP: Lazy<Mutex<i64>> = Lazy::new(|| Mutex::new(0));

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TakeoverStatus {
    pub is_active: bool,
    pub reason: String,
    pub timestamp: i64,
}

pub struct TakeoverController;

impl TakeoverController {
    /// Detects if an action or page title contains sensitive markers requiring human takeover
    pub fn detect_sensitive_checkpoint(context: &str) -> Option<String> {
        let lower = context.to_lowercase();
        let triggers = [
            (
                "2fa",
                "Two-Factor Authentication (2FA) verification requested",
            ),
            (
                "mfa",
                "Multi-Factor Authentication (MFA) checkpoint detected",
            ),
            ("captcha", "CAPTCHA challenge detected on screen"),
            ("password", "Password entry field or sensitive login prompt"),
            ("credit card", "Payment / Credit Card billing entry"),
            ("cvv", "Card Security Code (CVV) field"),
            ("one-time code", "One-Time Password (OTP) verification"),
            ("authenticator", "Authenticator verification prompt"),
        ];

        for (keyword, reason) in triggers {
            if lower.contains(keyword) {
                return Some(reason.to_string());
            }
        }
        None
    }

    pub fn request(reason: &str) {
        *TAKEOVER_REASON.lock().unwrap() = reason.to_string();
        *TAKEOVER_TIMESTAMP.lock().unwrap() = chrono::Utc::now().timestamp_millis();
        IS_TAKEOVER_ACTIVE.store(true, Ordering::SeqCst);
    }

    pub fn resume() {
        IS_TAKEOVER_ACTIVE.store(false, Ordering::SeqCst);
        *TAKEOVER_REASON.lock().unwrap() = String::new();
    }

    pub fn is_active() -> bool {
        IS_TAKEOVER_ACTIVE.load(Ordering::SeqCst)
    }

    pub fn get_status() -> TakeoverStatus {
        TakeoverStatus {
            is_active: IS_TAKEOVER_ACTIVE.load(Ordering::SeqCst),
            reason: TAKEOVER_REASON.lock().unwrap().clone(),
            timestamp: *TAKEOVER_TIMESTAMP.lock().unwrap(),
        }
    }
}
