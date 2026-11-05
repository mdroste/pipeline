//! World Bank Indicators API v2: one indicator for selected economies.
use super::{
    citation_date, csv, parse_json, scalar, status_error, Capture, ConnectorProvenance,
    DataConnector, Request,
};
use crate::workbench::{
    acquisition::Fetched,
    data::DataAcquisition,
    store::{WorkbenchError, WorkbenchResult},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const API: &str = "https://api.worldbank.org/v2";
const TERMS_URL: &str = "https://datacatalog.worldbank.org/public-licenses";
/// One page per capture; larger results are refused rather than paged silently.
const PER_PAGE: usize = 20_000;
const MAX_ECONOMIES: usize = 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldBankQuery {
    /// Indicator code, for example `NY.GDP.MKTP.CD`.
    pub indicator: String,
    /// Economy codes separated by semicolons, commas or spaces, or `all`.
    pub countries: String,
    /// `YYYY`, `YYYY:YYYY`, or monthly/quarterly forms such as `2010M01:2012M12`.
    #[serde(default)]
    pub date_range: Option<String>,
}
fn date_part(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 4
        && b[..4].iter().all(u8::is_ascii_digit)
        && match &b[4..] {
            [] => true,
            [b'M', m @ ..] | [b'Q', m @ ..] => {
                (1..=2).contains(&m.len()) && m.iter().all(u8::is_ascii_digit)
            }
            _ => false,
        }
}
impl WorldBankQuery {
    fn indicator(&self) -> WorkbenchResult<&str> {
        let s = self.indicator.trim();
        if s.is_empty()
            || s.len() > 100
            || !s
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_')
        {
            return Err(WorkbenchError::invalid(
                "Enter a World Bank indicator code, for example NY.GDP.MKTP.CD",
            ));
        }
        Ok(s)
    }
    fn countries(&self) -> WorkbenchResult<String> {
        let codes: Vec<String> = self
            .countries
            .split([';', ',', ' '])
            .filter(|s| !s.is_empty())
            .map(str::to_ascii_uppercase)
            .collect();
        if codes == ["ALL"] {
            return Ok("all".into());
        }
        if codes.is_empty()
            || codes.len() > MAX_ECONOMIES
            || codes.iter().any(|c| {
                !(2..=3).contains(&c.len()) || !c.bytes().all(|b| b.is_ascii_alphanumeric())
            })
        {
            return Err(WorkbenchError::invalid(
                "Enter up to 60 World Bank economy codes (for example USA;BRA) or all",
            ));
        }
        Ok(codes.join(";"))
    }
    fn date(&self) -> WorkbenchResult<Option<String>> {
        let Some(range) = self
            .date_range
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            return Ok(None);
        };
        let parts: Vec<&str> = range.split(':').collect();
        if parts.len() > 2 || !parts.iter().all(|p| date_part(p)) {
            return Err(WorkbenchError::invalid(
                "Enter dates as YYYY or YYYY:YYYY (monthly 2010M01:2012M12, quarterly 2010Q1:2012Q4)",
            ));
        }
        Ok(Some(range.into()))
    }
}
/// The API reports errors as `[{"message":[{"id","key","value"}]}]` with HTTP 200.
fn api_message(v: &Value) -> Option<String> {
    let m = &v[0]["message"][0];
    m.is_object().then(|| {
        format!(
            "{}{}",
            m["key"].as_str().unwrap_or("Error"),
            m["value"]
                .as_str()
                .map(|s| format!(": {s}"))
                .unwrap_or_default()
        )
    })
}
impl DataConnector for WorldBankQuery {
    fn provider(&self) -> &'static str {
        "World Bank Indicators"
    }
    fn query(&self) -> Value {
        json!({"indicator":self.indicator.trim(),"countries":self.countries.trim(),"dateRange":self.date_range})
    }
    fn requests(&self) -> WorkbenchResult<Vec<Request>> {
        let invalid = || WorkbenchError::invalid("Invalid World Bank endpoint");
        let indicator = self.indicator()?;
        let mut meta = reqwest::Url::parse(API).map_err(|_| invalid())?;
        meta.path_segments_mut()
            .map_err(|_| invalid())?
            .extend(["indicator", indicator]);
        meta.query_pairs_mut().append_pair("format", "json");
        let mut data = reqwest::Url::parse(API).map_err(|_| invalid())?;
        data.path_segments_mut().map_err(|_| invalid())?.extend([
            "country",
            &self.countries()?,
            "indicator",
            indicator,
        ]);
        {
            let mut q = data.query_pairs_mut();
            q.append_pair("format", "json")
                .append_pair("per_page", &PER_PAGE.to_string());
            if let Some(date) = self.date()? {
                q.append_pair("date", &date);
            }
        }
        Ok(vec![
            Request {
                url: meta,
                max_bytes: 1024 * 1024,
            },
            Request {
                url: data,
                max_bytes: 16 * 1024 * 1024,
            },
        ])
    }
    fn capture(&self, responses: Vec<Fetched>, retrieved_at: &str) -> WorkbenchResult<Capture> {
        let [meta, data]: [Fetched; 2] = responses
            .try_into()
            .map_err(|_| WorkbenchError::invalid("World Bank response set is incomplete"))?;
        let mut parsed = Vec::new();
        for response in [&meta, &data] {
            let v = parse_json("World Bank", response);
            if !(200..300).contains(&response.status) {
                let message = v.as_ref().ok().and_then(api_message);
                return Err(status_error("World Bank", response, message.as_deref()));
            }
            let v = v?;
            if let Some(message) = api_message(&v) {
                return Err(WorkbenchError::invalid(format!(
                    "World Bank refused the request ({message}); check the indicator, economy codes and dates"
                )));
            }
            parsed.push(v);
        }
        let (meta, data) = (&parsed[0], &parsed[1]);
        let info = &meta[1][0];
        if !info.is_object() {
            return Err(WorkbenchError::invalid("World Bank has no such indicator"));
        }
        let header = &data[0];
        let empty = Vec::new();
        let rows = match &data[1] {
            Value::Null => &empty,
            Value::Array(rows) => rows,
            _ => {
                return Err(WorkbenchError::invalid(
                    "World Bank returned malformed observations",
                ))
            }
        };
        let number = |v: &Value| {
            v.as_u64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        };
        let total = number(&header["total"]).unwrap_or(0) as usize;
        if number(&header["pages"]).unwrap_or(1) > 1 || total != rows.len() {
            return Err(WorkbenchError::invalid(format!(
                "World Bank reports {total} observations but returned {}; narrow the economies or dates (at most {PER_PAGE} per capture)",
                rows.len()
            )));
        }
        if rows
            .iter()
            .any(|r| !(r["value"].is_number() || r["value"].is_null()))
        {
            return Err(WorkbenchError::invalid(
                "World Bank returned a non-numeric observation",
            ));
        }
        let csv = csv(
            &[
                "country_id",
                "country_iso3",
                "country",
                "date",
                "value",
                "obs_status",
                "unit",
                "decimal",
            ],
            rows.iter().map(|r| {
                vec![
                    scalar(&r["country"]["id"]),
                    scalar(&r["countryiso3code"]),
                    scalar(&r["country"]["value"]),
                    scalar(&r["date"]),
                    scalar(&r["value"]),
                    scalar(&r["obs_status"]),
                    scalar(&r["unit"]),
                    scalar(&r["decimal"]),
                ]
            }),
        );
        let indicator = self.indicator()?;
        let countries = self.countries()?;
        let name = info["name"].as_str().unwrap_or(indicator);
        let source = info["source"]["value"]
            .as_str()
            .unwrap_or("World Bank Indicators");
        let mut licence = "Creative Commons Attribution 4.0 International (CC BY 4.0), the World Bank's default licence for its open data; indicators from third parties may carry other terms.".to_string();
        if let Some(org) = info["sourceOrganization"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
        {
            licence.push_str(&format!(
                " Source organization: {}.",
                crate::workbench::search::prefix(org.trim(), 500)
            ));
        }
        let page = format!("https://data.worldbank.org/indicator/{indicator}");
        Ok(Capture {
            title: format!("{name} · {countries}"),
            csv,
            acquisition: DataAcquisition {
                provider: "World Bank Indicators".into(),
                source: page.clone(),
                retrieved_at: retrieved_at.into(),
                requested_vintage: None,
                returned_vintage: None,
                series_ids: vec![indicator.into()],
                units: info["unit"].as_str().filter(|s| !s.is_empty()).map(str::to_string),
                frequency: None,
                transformation: None,
            },
            provenance: ConnectorProvenance {
                connector: "world_bank".into(),
                query: self.query(),
                request_urls: self.requests()?.into_iter().map(|r| r.url.into()).collect(),
                last_updated: header["lastupdated"].as_str().map(str::to_string),
                licence,
                notice: Some(format!(
                    "Source: World Bank, {source}. Licensed under CC BY 4.0 unless the indicator's terms say otherwise."
                )),
                citation: format!(
                    "World Bank, {source}: {name} ({indicator}), economies {countries}; {page}, retrieved {}.",
                    citation_date(retrieved_at)
                ),
                terms_url: TERMS_URL.into(),
                terms_reviewed: super::TERMS_REVIEWED.into(),
            },
        })
    }
}
