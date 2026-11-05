//! DBnomics (Cepremap) series capture: one series by provider/dataset/series code.
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

const API: &str = "https://api.db.nomics.world/v22/series";
const TERMS_URL: &str = "https://docs.db.nomics.world/legal-terms";
const MAX_OBSERVATIONS: usize = 100_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DbnomicsQuery {
    /// `PROVIDER/DATASET/SERIES`, as shown on db.nomics.world.
    pub series_id: String,
}
impl DbnomicsQuery {
    fn parts(&self) -> WorkbenchResult<[&str; 3]> {
        let invalid = || {
            WorkbenchError::invalid(
                "Enter a DBnomics series ID as provider/dataset/series, for example IMF/WEO:2024-10/USA.NGDP_RPCH.pcent_change",
            )
        };
        let parts: [&str; 3] = self
            .series_id
            .trim()
            .split('/')
            .collect::<Vec<_>>()
            .try_into()
            .map_err(|_| invalid())?;
        if parts.iter().any(|p| {
            p.is_empty()
                || p.len() > 200
                || !p
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-:+@~".contains(&b))
        }) {
            return Err(invalid());
        }
        Ok(parts)
    }
    fn id(&self) -> WorkbenchResult<String> {
        Ok(self.parts()?.join("/"))
    }
}
/// Label of a dimension value from the dataset's dimension dictionary.
fn dimension_label(dataset: &Value, doc: &Value, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        let code = doc["dimensions"][name].as_str()?;
        Some(
            dataset["dimensions_values_labels"][name][code]
                .as_str()
                .unwrap_or(code)
                .to_string(),
        )
    })
}
impl DataConnector for DbnomicsQuery {
    fn provider(&self) -> &'static str {
        "DBnomics"
    }
    fn query(&self) -> Value {
        json!({"seriesId":self.series_id.trim()})
    }
    fn requests(&self) -> WorkbenchResult<Vec<Request>> {
        let mut url = reqwest::Url::parse(API)
            .map_err(|_| WorkbenchError::invalid("Invalid DBnomics endpoint"))?;
        url.path_segments_mut()
            .map_err(|_| WorkbenchError::invalid("Invalid DBnomics endpoint"))?
            .extend(self.parts()?);
        url.query_pairs_mut()
            .append_pair("observations", "1")
            .append_pair("metadata", "true")
            .append_pair("format", "json");
        Ok(vec![Request {
            url,
            max_bytes: 16 * 1024 * 1024,
        }])
    }
    fn capture(&self, responses: Vec<Fetched>, retrieved_at: &str) -> WorkbenchResult<Capture> {
        let response = responses
            .into_iter()
            .next()
            .ok_or_else(|| WorkbenchError::invalid("DBnomics response is missing"))?;
        if !(200..300).contains(&response.status) {
            let message = serde_json::from_slice::<Value>(&response.body)
                .ok()
                .and_then(|v| v["message"].as_str().map(str::to_string));
            return Err(status_error("DBnomics", &response, message.as_deref()));
        }
        let v = parse_json("DBnomics", &response)?;
        let docs = v["series"]["docs"]
            .as_array()
            .ok_or_else(|| WorkbenchError::invalid("DBnomics returned no series"))?;
        if docs.len() != 1 || v["series"]["num_found"].as_u64().is_some_and(|n| n != 1) {
            return Err(WorkbenchError::invalid(
                "DBnomics did not return exactly one series; check the series ID",
            ));
        }
        let doc = &docs[0];
        let empty = Vec::new();
        let periods = doc["period"].as_array().unwrap_or(&empty);
        let starts = doc["period_start_day"].as_array().unwrap_or(&empty);
        let values = doc["value"]
            .as_array()
            .ok_or_else(|| WorkbenchError::invalid("DBnomics returned a series without values"))?;
        if periods.len() != values.len() || (!starts.is_empty() && starts.len() != values.len()) {
            return Err(WorkbenchError::invalid(
                "DBnomics returned periods and values of different lengths",
            ));
        }
        if values.len() > MAX_OBSERVATIONS {
            return Err(WorkbenchError::invalid(
                "DBnomics series exceeds 100,000 observations",
            ));
        }
        if values
            .iter()
            .any(|x| !(x.is_number() || x.is_null() || x.as_str() == Some("NA")))
        {
            return Err(WorkbenchError::invalid(
                "DBnomics returned an observation that is neither a number nor NA",
            ));
        }
        let rows = (0..values.len()).map(|i| {
            vec![
                scalar(&periods[i]),
                starts.get(i).map(scalar).unwrap_or_default(),
                // DBnomics writes missing observations as NA, which stays missing.
                if values[i].is_null() {
                    "NA".into()
                } else {
                    scalar(&values[i])
                },
            ]
        });
        let csv = csv(&["period", "period_start_day", "value"], rows);
        let id = self.id()?;
        let dataset = &v["dataset"];
        let provider = &v["provider"];
        let text = |value: &Value, fallback: &str| value.as_str().unwrap_or(fallback).to_string();
        let provider_code = text(&doc["provider_code"], self.parts()?[0]);
        let provider_name = text(
            &provider["name"],
            &text(&dataset["provider_name"], &provider_code),
        );
        let dataset_code = text(&doc["dataset_code"], self.parts()?[1]);
        let dataset_name = text(&doc["dataset_name"], &text(&dataset["name"], &dataset_code));
        let series_name = text(&doc["series_name"], &id);
        let provider_terms = provider["terms_of_use"].as_str().map(str::to_string);
        let mut licence = "DBnomics aggregated data are distributed under the Open Database License (ODbL) 1.0; data keep the licence and terms of use of the original provider".to_string();
        match &provider_terms {
            Some(url) => licence.push_str(&format!(" ({provider_name}: {url}).")),
            None => licence.push('.'),
        }
        let mut notice = format!(
            "Data retrieved through DBnomics (Cepremap), https://db.nomics.world. Original source: {provider_name} ({provider_code}); the provider's own terms apply."
        );
        if provider_code.eq_ignore_ascii_case("IMF") {
            notice.push_str(" Source: International Monetary Fund. The IMF requires permission for bulk automated download and for using its data to train models; capture series on demand only.");
        }
        let url = format!("https://db.nomics.world/{id}");
        Ok(Capture {
            title: format!("{series_name} · DBnomics"),
            csv,
            acquisition: DataAcquisition {
                provider: "DBnomics".into(),
                source: url.clone(),
                retrieved_at: retrieved_at.into(),
                requested_vintage: None,
                returned_vintage: None,
                series_ids: vec![id.clone()],
                units: dimension_label(dataset, doc, &["unit", "UNIT", "UNIT_MEASURE", "unit_measure"]),
                frequency: doc["@frequency"].as_str().map(str::to_string),
                transformation: None,
            },
            provenance: ConnectorProvenance {
                connector: "dbnomics".into(),
                query: self.query(),
                request_urls: self.requests()?.into_iter().map(|r| r.url.into()).collect(),
                last_updated: doc["indexed_at"]
                    .as_str()
                    .or_else(|| dataset["indexed_at"].as_str())
                    .map(str::to_string),
                licence,
                notice: Some(notice),
                citation: format!(
                    "{provider_name}, {dataset_name} ({dataset_code}), series {series_name} [{id}]. Retrieved via DBnomics, {url}, {}.",
                    citation_date(retrieved_at)
                ),
                terms_url: TERMS_URL.into(),
                terms_reviewed: super::TERMS_REVIEWED.into(),
            },
        })
    }
}
