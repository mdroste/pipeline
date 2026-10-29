//! Crossref lookups for the reference check. Requests are sequential and
//! paced below Crossref's public limits (one list query per second, a few
//! single-record lookups per second), bounded in size and time, and never
//! follow redirects. No key or contact address is sent.

use super::super::fold::{fold, tokens, Fold};
use serde_json::Value;
use std::time::{Duration, Instant};

const API: &str = "https://api.crossref.org";
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const LIST_SPACING: Duration = Duration::from_millis(1_100);
const SINGLE_SPACING: Duration = Duration::from_millis(250);
const MAX_RETRY_AFTER: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Work {
    pub doi: String,
    pub title: String,
    pub families: Vec<String>,
    pub year: Option<u16>,
    pub container: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// `retraction`, `withdrawal`, `expression_of_concern`, `correction`, ...
    pub kind: String,
    pub notice_doi: String,
    pub source: String,
}

pub struct Client {
    http: reqwest::Client,
    last_list: Option<Instant>,
    last_single: Option<Instant>,
    pub requests: u32,
}

fn text_field(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::Array(items)) => items
            .first()
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        Some(Value::String(text)) => text.clone(),
        _ => String::new(),
    }
}

fn issued_year(value: &Value) -> Option<u16> {
    for key in ["issued", "published-print", "published-online", "created"] {
        let year = value
            .pointer(&format!("/{key}/date-parts/0/0"))
            .and_then(Value::as_u64)
            .and_then(|year| u16::try_from(year).ok());
        if year.is_some() {
            return year;
        }
    }
    None
}

pub fn parse_work(value: &Value) -> Work {
    Work {
        doi: value
            .get("DOI")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase(),
        title: text_field(value, "title"),
        families: value
            .get("author")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|author| author.get("family").and_then(Value::as_str))
            .map(str::to_string)
            .take(20)
            .collect(),
        year: issued_year(value),
        container: text_field(value, "container-title"),
        kind: value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    }
}

/// Notices in a `works?filter=updates:` response that update `doi`.
pub fn parse_notices(response: &Value, doi: &str) -> Vec<Notice> {
    let mut notices = Vec::new();
    for item in response
        .pointer("/message/items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let notice_doi = item
            .get("DOI")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase();
        for update in item
            .get("update-to")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let target = update
                .get("DOI")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_ascii_lowercase();
            if target != doi {
                continue;
            }
            let kind = update
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("update")
                .replace(['-', ' '], "_")
                .to_ascii_lowercase();
            let notice = Notice {
                kind,
                notice_doi: notice_doi.clone(),
                source: update
                    .get("source")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            };
            if !notices.contains(&notice) {
                notices.push(notice);
            }
        }
    }
    notices
}

#[derive(Debug)]
pub enum LookupError {
    Cancelled,
    Failed(String),
}

impl Client {
    pub fn new() -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!(
                "Pipeline/",
                env!("CARGO_PKG_VERSION"),
                " (https://github.com/mdroste/pipeline; reference check)"
            ))
            .build()
            .map_err(|error| format!("Could not start the Crossref client: {error}"))?;
        Ok(Self {
            http,
            last_list: None,
            last_single: None,
            requests: 0,
        })
    }

    async fn pace(&mut self, list: bool) {
        let (last, spacing) = if list {
            (self.last_list, LIST_SPACING)
        } else {
            (self.last_single, SINGLE_SPACING)
        };
        if let Some(last) = last {
            let elapsed = last.elapsed();
            if elapsed < spacing {
                tokio::time::sleep(spacing - elapsed).await;
            }
        }
        if list {
            self.last_list = Some(Instant::now());
        } else {
            self.last_single = Some(Instant::now());
        }
    }

    /// GET with pacing, one retry after a rate-limit response, and a body cap.
    /// `Ok(None)` is a 404.
    async fn get(&mut self, url: reqwest::Url, list: bool) -> Result<Option<Value>, LookupError> {
        for attempt in 0..2 {
            if crate::commands::is_cancelled() {
                return Err(LookupError::Cancelled);
            }
            self.pace(list).await;
            self.requests += 1;
            let response = self.http.get(url.clone()).send().await.map_err(|error| {
                LookupError::Failed(format!("Crossref request failed: {error}"))
            })?;
            let status = response.status();
            if status.as_u16() == 404 {
                return Ok(None);
            }
            if (status.as_u16() == 429 || status.is_server_error()) && attempt == 0 {
                let wait = response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.trim().parse::<u64>().ok())
                    .map(Duration::from_secs)
                    .unwrap_or(Duration::from_secs(3))
                    .min(MAX_RETRY_AFTER);
                tokio::time::sleep(wait).await;
                continue;
            }
            if !status.is_success() {
                return Err(LookupError::Failed(format!("Crossref returned {status}")));
            }
            let mut body = Vec::new();
            let mut response = response;
            while let Some(chunk) = response.chunk().await.map_err(|error| {
                LookupError::Failed(format!("Crossref response failed: {error}"))
            })? {
                body.extend_from_slice(&chunk);
                if body.len() > MAX_RESPONSE_BYTES {
                    return Err(LookupError::Failed(
                        "Crossref response exceeded the size limit".into(),
                    ));
                }
            }
            return serde_json::from_slice(&body)
                .map(Some)
                .map_err(|_| LookupError::Failed("Crossref returned unreadable JSON".into()));
        }
        Err(LookupError::Failed(
            "Crossref is rate limiting requests".into(),
        ))
    }

    pub async fn work(&mut self, doi: &str) -> Result<Option<Work>, LookupError> {
        let mut url = reqwest::Url::parse(API).expect("valid base URL");
        url.path_segments_mut()
            .expect("base URL has a path")
            .extend(["works", doi]);
        Ok(self
            .get(url, false)
            .await?
            .and_then(|value| value.get("message").map(parse_work)))
    }

    pub async fn search(&mut self, citation: &str) -> Result<Vec<Work>, LookupError> {
        let mut url = reqwest::Url::parse(&format!("{API}/works")).expect("valid URL");
        let query: String = citation.chars().take(400).collect();
        url.query_pairs_mut()
            .append_pair("query.bibliographic", &query)
            .append_pair("rows", "3");
        Ok(self
            .get(url, true)
            .await?
            .and_then(|value| value.pointer("/message/items").cloned())
            .and_then(|items| items.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .map(parse_work)
            .collect())
    }

    pub async fn notices(&mut self, doi: &str) -> Result<Vec<Notice>, LookupError> {
        let mut url = reqwest::Url::parse(&format!("{API}/works")).expect("valid URL");
        url.query_pairs_mut()
            .append_pair("filter", &format!("updates:{doi}"))
            .append_pair("rows", "20");
        Ok(self
            .get(url, true)
            .await?
            .map(|value| parse_notices(&value, doi))
            .unwrap_or_default())
    }
}

fn token_set(text: &str) -> std::collections::HashSet<String> {
    let folded = fold(text, Fold::Latex).text;
    tokens(&folded)
        .into_iter()
        .map(|(_, token)| token.to_string())
        .filter(|token| token.chars().count() >= 2)
        .collect()
}

/// Share of a candidate title's words that appear in the reference text.
pub fn title_coverage(title: &str, reference: &str) -> f64 {
    let title_tokens = token_set(title);
    if title_tokens.is_empty() {
        return 0.0;
    }
    let reference_tokens = token_set(reference);
    let shared = title_tokens
        .iter()
        .filter(|token| reference_tokens.contains(*token))
        .count();
    shared as f64 / title_tokens.len() as f64
}

/// Whether one of the candidate's first three authors appears in the text.
pub fn author_present(work: &Work, reference: &str) -> bool {
    if work.families.is_empty() {
        return true;
    }
    let reference_tokens = token_set(reference);
    work.families.iter().take(3).any(|family| {
        let family = token_set(family);
        !family.is_empty() && family.iter().all(|token| reference_tokens.contains(token))
    })
}

/// Working-paper or preprint wording in a reference, used to recognize a
/// cited version that differs from the published record.
pub fn looks_unpublished(reference: &str) -> bool {
    let lower = reference.to_lowercase();
    [
        "working paper",
        "discussion paper",
        "mimeo",
        "preprint",
        "arxiv",
        "ssrn",
        "nber",
        "unpublished",
        "manuscript",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}
