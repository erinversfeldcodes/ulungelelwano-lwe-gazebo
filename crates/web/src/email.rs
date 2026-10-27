#![cfg(feature = "ssr")]

/// Sends emails. Falls back to logging to stdout when `RESEND_API_KEY`
/// isn't set, so local dev and CI never need a real Resend account —
/// mirrors the hermetic-by-default pattern `thestacks` uses for its own
/// email tests.
#[derive(Clone)]
pub enum EmailSender {
    Resend {
        api_key: String,
        from: String,
        client: reqwest::Client,
    },
    Console,
}

impl EmailSender {
    pub fn from_env() -> Self {
        match std::env::var("RESEND_API_KEY") {
            Ok(api_key) if !api_key.is_empty() => Self::Resend {
                api_key,
                from: std::env::var("EMAIL_FROM")
                    .unwrap_or_else(|_| "onboarding@resend.dev".to_string()),
                client: reqwest::Client::new(),
            },
            _ => Self::Console,
        }
    }

    pub async fn send_magic_link(&self, to: &str, link: &str) -> Result<(), String> {
        let html = format!(
            "<p>Click to sign in to Gazebo:</p><p><a href=\"{link}\">{link}</a></p>\
             <p>This link expires in 30 minutes.</p>"
        );
        self.send(to, "Sign in to Gazebo", &html).await
    }

    /// Sends a clearly-labeled, harmless email used only to prove the
    /// Resend integration actually works end to end — see
    /// `/internal/email-check`, which CI hits right after every deploy.
    ///
    /// Unlike `send_magic_link`, Console mode is a *failure* here, not a
    /// fallback: the whole point of this call is to prove real delivery
    /// works, so silently logging instead would make the deploy check
    /// pass even when `RESEND_API_KEY` is missing in an environment that's
    /// supposed to send real email.
    pub async fn send_test_email(&self, to: &str) -> Result<(), String> {
        if matches!(self, Self::Console) {
            return Err("RESEND_API_KEY is not set — still in Console mode".to_string());
        }
        let html = "<p>Automated deployment check for Gazebo — nothing to do here.</p>";
        self.send(to, "Gazebo deployment check", html).await
    }

    async fn send(&self, to: &str, subject: &str, html: &str) -> Result<(), String> {
        match self {
            Self::Console => {
                log::info!("would send \"{subject}\" to {to}: {html}");
                Ok(())
            }
            Self::Resend {
                api_key,
                from,
                client,
            } => {
                let response = client
                    .post("https://api.resend.com/emails")
                    .bearer_auth(api_key)
                    .json(&serde_json::json!({
                        "from": from,
                        "to": to,
                        "subject": subject,
                        "html": html,
                    }))
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;

                if response.status().is_success() {
                    Ok(())
                } else {
                    Err(format!(
                        "Resend returned {}: {}",
                        response.status(),
                        response.text().await.unwrap_or_default()
                    ))
                }
            }
        }
    }
}
