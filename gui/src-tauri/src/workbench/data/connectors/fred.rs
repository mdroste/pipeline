//! FRED/ALFRED vintage capture with a per-request key that is never recorded.
use super::{
    citation_date, csv, parse_json, status_error, Capture, ConnectorProvenance, DataConnector,
    Request,
};
use crate::workbench::{
    acquisition::Fetched,
    data::DataAcquisition,
    store::{WorkbenchError, WorkbenchResult},
};
use serde::Deserialize;
use serde_json::{json, Value};

/// Required by the FRED® API Terms of Use wherever FRED data are shown.
pub(crate) const NOTICE: &str = "This product uses the FRED® API but is not endorsed or certified by the Federal Reserve Bank of St. Louis. By using FRED® data, you agree to be bound by the FRED® API Terms of Use (https://fred.stlouisfed.org/docs/api/terms_of_use.html).";
const TERMS_URL: &str = "https://fred.stlouisfed.org/docs/api/terms_of_use.html";
const SERIES: &str = "https://api.stlouisfed.org/fred/series";
const OBSERVATIONS: &str = "https://api.stlouisfed.org/fred/series/observations";

/// The key is held only for this request; this type deliberately has no `Debug`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FredRequest {
    pub workspace_id: String,
    pub series_id: String,
    pub vintage: String,
    pub api_key: String,
    pub operation_id: String,
}
impl FredRequest {
    fn validate(&self) -> WorkbenchResult<()> {
        if self.series_id.is_empty()
            || self.series_id.len() > 100
            || !self
                .series_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(WorkbenchError::invalid("Invalid FRED series ID"));
        }
        chrono::NaiveDate::parse_from_str(&self.vintage, "%Y-%m-%d")
            .map_err(|_| WorkbenchError::invalid("Choose an exact vintage date (YYYY-MM-DD)"))?;
        if self.api_key.len() != 32 || !self.api_key.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Err(WorkbenchError::invalid(
                "Enter a FRED API key; it is used only for this request",
            ));
        }
        Ok(())
    }
    fn params(&self) -> [(&str, &str); 4] {
        [
            ("series_id", self.series_id.as_str()),
            ("file_type", "json"),
            ("realtime_start", self.vintage.as_str()),
            ("realtime_end", self.vintage.as_str()),
        ]
    }
    /// Request URLs as recorded: the key is never included.
    fn display_urls(&self) -> Vec<String> {
        let params = self.params();
        let meta = reqwest::Url::parse_with_params(SERIES, params);
        let observations = reqwest::Url::parse_with_params(
            OBSERVATIONS,
            params
                .iter()
                .copied()
                .chain([("limit", "100000"), ("units", "lin")]),
        );
        [meta, observations]
            .into_iter()
            .filter_map(Result::ok)
            .map(String::from)
            .collect()
    }
}
impl DataConnector for FredRequest {
    fn provider(&self) -> &'static str {
        "FRED/ALFRED"
    }
    fn query(&self) -> Value {
        json!({"seriesId":self.series_id,"vintage":self.vintage})
    }
    fn requests(&self) -> WorkbenchResult<Vec<Request>> {
        self.validate()?;
        let keyed = || {
            self.params()
                .into_iter()
                .chain([("api_key", self.api_key.as_str())])
        };
        let invalid = |_| WorkbenchError::invalid("Invalid FRED request");
        Ok(vec![
            Request {
                url: reqwest::Url::parse_with_params(SERIES, keyed()).map_err(invalid)?,
                max_bytes: 1024 * 1024,
            },
            Request {
                url: reqwest::Url::parse_with_params(
                    OBSERVATIONS,
                    keyed().chain([("limit", "100000"), ("units", "lin")]),
                )
                .map_err(invalid)?,
                max_bytes: 16 * 1024 * 1024,
            },
        ])
    }
    fn capture(&self, responses: Vec<Fetched>, retrieved_at: &str) -> WorkbenchResult<Capture> {
        let [meta, observations]: [Fetched; 2] = responses
            .try_into()
            .map_err(|_| WorkbenchError::invalid("FRED response set is incomplete"))?;
        for response in [&meta, &observations] {
            if !(200..300).contains(&response.status) {
                // FRED reports errors as JSON; never echo the key even if it appears.
                let message = serde_json::from_slice::<Value>(&response.body)
                    .ok()
                    .and_then(|v| v["error_message"].as_str().map(str::to_string))
                    .map(|m| m.replace(&self.api_key, "[key]"));
                return Err(status_error("FRED", response, message.as_deref()));
            }
        }
        let meta = parse_json("FRED", &meta)
            .map_err(|_| WorkbenchError::invalid("Invalid FRED series metadata"))?;
        let v = parse_json("FRED", &observations)
            .map_err(|_| WorkbenchError::invalid("Invalid FRED observations"))?;
        let rows = v["observations"]
            .as_array()
            .ok_or_else(|| WorkbenchError::invalid("FRED returned no observation array"))?;
        if v["count"].as_u64() != Some(rows.len() as u64) {
            return Err(WorkbenchError::invalid(
                "FRED response is incomplete; this adapter refuses truncated series",
            ));
        }
        let keys = ["date", "value", "realtime_start", "realtime_end"];
        let csv = csv(
            &keys,
            rows.iter().map(|row| {
                keys.map(|k| row[k].as_str().unwrap_or("").to_string())
                    .to_vec()
            }),
        );
        let info = &meta["seriess"][0];
        let title = info["title"].as_str().unwrap_or(&self.series_id);
        let mut licence = "FRED® API Terms of Use. Some series are copyrighted by their owners and need the owner's permission for uses beyond personal use; check the series notes before sharing these data.".to_string();
        if info["notes"]
            .as_str()
            .is_some_and(|n| n.to_ascii_lowercase().contains("copyright"))
        {
            licence.push_str(" This series' notes include a copyright statement.");
        }
        Ok(Capture {
            title: format!("{} · {}", self.series_id, self.vintage),
            csv,
            acquisition: DataAcquisition {
                provider: "FRED/ALFRED".into(),
                source: format!("https://fred.stlouisfed.org/series/{}", self.series_id),
                retrieved_at: retrieved_at.into(),
                requested_vintage: Some(self.vintage.clone()),
                returned_vintage: v["realtime_start"].as_str().map(str::to_string),
                series_ids: vec![self.series_id.clone()],
                units: info["units"].as_str().map(str::to_string),
                frequency: info["frequency"].as_str().map(str::to_string),
                transformation: Some("lin (untransformed)".into()),
            },
            provenance: ConnectorProvenance {
                connector: "fred".into(),
                query: self.query(),
                request_urls: self.display_urls(),
                last_updated: info["last_updated"].as_str().map(str::to_string),
                licence,
                notice: Some(NOTICE.into()),
                citation: format!(
                    "{title} [{}], vintage {}, retrieved from ALFRED, Federal Reserve Bank of St. Louis; https://alfred.stlouisfed.org/series?seid={}, {}.",
                    self.series_id,
                    self.vintage,
                    self.series_id,
                    citation_date(retrieved_at)
                ),
                terms_url: TERMS_URL.into(),
                terms_reviewed: super::TERMS_REVIEWED.into(),
            },
        })
    }
}
