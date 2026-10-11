//! Connecting a Discord / Hugging Face / GitHub account.
//!
//! Each provider redirects back to `http://localhost:53630/callback/<short>`
//! (the URLs registered with the providers). We listen there, exchange the
//! authorization code for a token, fetch the profile, and hand the result to
//! the Worker.
//!
//! The OAuth client secrets live in the Worker, never in the app, so this file
//! only builds the authorization URL.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde::Deserialize;

use crate::account::Provider;

/// Must match the redirect URIs registered with Discord / HF / GitHub.
pub const CALLBACK_PORT: u16 = 53630;
pub const CALLBACK_BASE: &str = "http://localhost:53630";

#[derive(Debug, Clone, PartialEq)]
pub struct OAuthProfile {
    pub username: Option<String>,
    pub avatar_url: Option<String>,
    pub access_token: Option<String>,
}

/// Anything the callback handler can report back to the UI.
#[derive(Debug)]
pub enum Callback {
    Done { provider: Provider, profile: OAuthProfile },
    Failed { provider: Provider, error: String },
}

/// Builds the URL the browser should be sent to.
pub fn authorize_url(provider: Provider) -> Result<String, String> {
    let redirect = format!("{CALLBACK_BASE}{}", provider.callback_path());
    let query = match provider {
        Provider::Huggingface => format!(
            "client_id={}&redirect_uri={}&response_type=code&scope=openid%20profile%20read-user&state={}",
            HF_CLIENT_ID,
            urlencode(&redirect),
            state_for(provider),
        ),
        Provider::Discord => format!(
            "client_id={}&redirect_uri={}&response_type=code&scope=identify%20email&state={}",
            DC_CLIENT_ID,
            urlencode(&redirect),
            state_for(provider),
        ),
        Provider::Github => format!(
            "client_id={}&redirect_uri={}&scope=read:user&state={}",
            GH_CLIENT_ID,
            urlencode(&redirect),
            state_for(provider),
        ),
    };

    let base = match provider {
        Provider::Huggingface => "https://huggingface.co/oauth/authorize",
        Provider::Discord => "https://discord.com/oauth2/authorize",
        Provider::Github => "https://github.com/login/oauth/authorize",
    };
    Ok(format!("{base}?{query}"))
}

/// OAuth *client ids* only. These are public by design - the matching secrets
/// live in the Worker and never ship with the app.
const HF_CLIENT_ID: &str = "2311f9c8-10a6-445f-a784-06587faf418b";
const DC_CLIENT_ID: &str = "1558860522009530388";
const GH_CLIENT_ID: &str = "Iv23liOO2o5ENlt0ySsE";

fn state_for(provider: Provider) -> &'static str {
    match provider {
        Provider::Huggingface => "projectrem-hf",
        Provider::Discord => "projectrem-dc",
        Provider::Github => "projectrem-gh",
    }
}

fn urlencode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// A tiny one-shot HTTP listener bound to the callback port.
///
/// It only understands what an OAuth redirect needs: one GET, one response.
/// Nothing here blocks the UI thread: [`CallbackServer::poll`] is called once
/// per frame and returns the query string as soon as the browser arrives.
pub struct CallbackServer {
    rx: Receiver<HashMap<String, String>>,
    provider: Provider,
    /// Kept alive so the socket stays bound for the duration of the flow.
    _listener: Arc<TcpListener>,
}

impl CallbackServer {
    /// Bind the port. Fails with a clear message if something else owns it,
    /// which is usually a second copy of the app still running.
    pub fn bind(provider: Provider) -> Result<Self, String> {
        let listener = TcpListener::bind(("127.0.0.1", CALLBACK_PORT)).map_err(|e| {
            format!("Could not listen on port {CALLBACK_PORT}: {e}. Is ProjectRem already open?")
        })?;

        let (tx, rx) = mpsc::channel();
        let accept = Arc::new(listener);
        let bound = Arc::clone(&accept);

        thread::spawn(move || {
            let Ok((mut socket, _)) = bound.accept() else { return };
            let Ok(clone) = socket.try_clone() else { return };
            let mut reader = BufReader::new(clone);

            let mut request_line = String::new();
            if reader.read_line(&mut request_line).is_err() {
                return;
            }
            // Drain headers so the client does not see a connection reset.
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 2 {
                line.clear();
            }

            let path = request_line.split_whitespace().nth(1).unwrap_or("/").to_owned();
            let params = parse_query(&path);

            let declined = params.get("error").is_some();
            let message = if declined {
                "Authorization was declined. You can close this tab."
            } else {
                "Signed in. You can close this tab and go back to ProjectRem."
            };
            let page = format!(
                "<!doctype html><html><head><meta charset=\"utf-8\">\
                 <title>ProjectRem</title></head>\
                 <body style=\"font-family:system-ui;background:#16171a;color:#dbdee1;padding:48px\">\
                 <h1 style=\"font-size:20px\">ProjectRem</h1><p>{message}</p></body></html>"
            );
            let _ = socket.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{page}",
                    page.len()
                )
                .as_bytes(),
            );
            let _ = socket.flush();
            let _ = tx.send(params);
        });

        Ok(Self { rx, provider, _listener: accept })
    }

    pub fn provider(&self) -> Provider {
        self.provider
    }

    /// `Some(params)` once the browser has redirected back.
    pub fn poll(&self) -> Option<HashMap<String, String>> {
        self.rx.try_recv().ok()
    }
}

/// Validate the redirect and turn the code into a profile.
///
/// Blocking; run it on a worker thread.
pub fn finish(provider: Provider, params: &HashMap<String, String>) -> Callback {
    if let Some(error) = params.get("error") {
        return Callback::Failed { provider, error: error.clone() };
    }
    let Some(code) = params.get("code") else {
        return Callback::Failed {
            provider,
            error: "The provider did not return a code.".to_owned(),
        };
    };
    if let Some(expected) = params.get("state") {
        if expected != state_for(provider) {
            return Callback::Failed {
                provider,
                error: "State mismatch, aborting.".to_owned(),
            };
        }
    }
    match exchange(provider, code) {
        Ok(profile) => Callback::Done { provider, profile },
        Err(error) => Callback::Failed { provider, error },
    }
}
fn parse_query(path: &str) -> HashMap<String, String> {
    let Some((_, query)) = path.split_once('?') else { return HashMap::new() };
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(k, v)| (k.to_owned(), percent_decode(v)))
        .collect()
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("00");
                out.push(u8::from_str_radix(hex, 16).unwrap_or(b'%'));
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Swap the code for a token and read the profile.
///
/// The token exchange happens *in the Worker*, so the client secret never has
/// to be present on this machine.
fn exchange(provider: Provider, code: &str) -> Result<OAuthProfile, String> {
    let redirect = format!("{CALLBACK_BASE}{}", provider.callback_path());
    let body = serde_json::json!({
        "provider": provider.code(),
        "code": code,
        "redirect_uri": redirect,
    });

    let http = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;

    let response = http
        .post(format!("{}/api/oauth/exchange", crate::account::base_url()))
        .json(&body)
        .send()
        .map_err(|e| format!("Could not reach the ProjectRem server: {e}"))?;

    let status = response.status();
    let text = response.text().map_err(|e| e.to_string())?;
    if !status.is_success() {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(error) = value.get("error").and_then(|v| v.as_str()) {
                return Err(error.to_owned());
            }
        }
        return Err(format!("HTTP {}", status.as_u16()));
    }

    #[derive(Deserialize)]
    struct Reply {
        #[serde(default)]
        username: Option<String>,
        #[serde(default)]
        avatar_url: Option<String>,
        #[serde(default)]
        access_token: Option<String>,
    }

    let reply: Reply =
        serde_json::from_str(&text).map_err(|e| format!("Unexpected reply: {e}"))?;
    Ok(OAuthProfile {
        username: reply.username,
        avatar_url: reply.avatar_url,
        access_token: reply.access_token,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_parsing() {
        let q = parse_query("/callback/hf?code=abc&state=projectrem-hf");
        assert_eq!(q.get("code").map(String::as_str), Some("abc"));
        assert_eq!(q.get("state").map(String::as_str), Some("projectrem-hf"));
    }

    #[test]
    fn percent_decoding() {
        assert_eq!(percent_decode("a%20b+c"), "a b c");
    }

    #[test]
    fn authorize_urls_carry_the_registered_redirect() {
        let hf = authorize_url(Provider::Huggingface).unwrap();
        assert!(hf.contains("redirect_uri=http%3A%2F%2Flocalhost%3A53630%2Fcallback%2Fhf"));
        let dc = authorize_url(Provider::Discord).unwrap();
        assert!(dc.contains("redirect_uri=http%3A%2F%2Flocalhost%3A53630%2Fcallback%2Fdc"));
        let gh = authorize_url(Provider::Github).unwrap();
        assert!(gh.contains("redirect_uri=http%3A%2F%2Flocalhost%3A53630%2Fcallback%2Fgh"));
    }

    #[test]
    fn every_provider_has_a_distinct_state_and_callback() {
        let states: Vec<&str> = Provider::ALL.iter().map(|p| state_for(*p)).collect();
        let mut unique = states.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(states.len(), unique.len(), "states must not be reused across providers");

        let paths: Vec<String> = Provider::ALL.iter().map(|p| p.callback_path().to_owned()).collect();
        let mut unique = paths.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(paths.len(), unique.len(), "each provider needs its own redirect path");
    }
}