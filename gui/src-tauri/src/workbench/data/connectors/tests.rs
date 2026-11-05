//! Connector parsing from recorded responses (trimmed from live replies on
//! 2026-10-28). Live requests are `#[ignore]`d and run only when qualifying.
use super::*;
use crate::workbench::store::CreateWorkspaceRequest;

const KEY: &str = "abcdefghij0123456789abcdefghij01";
const AT: &str = "2026-10-28T12:00:00+00:00";

fn ok(body: &str) -> Fetched {
    Fetched {
        status: 200,
        body: body.as_bytes().to_vec(),
    }
}
fn status(code: u16, body: &str) -> Fetched {
    Fetched {
        status: code,
        ..ok(body)
    }
}
fn fred() -> FredRequest {
    FredRequest {
        workspace_id: "w".into(),
        series_id: "GDP".into(),
        vintage: "2020-01-01".into(),
        api_key: KEY.into(),
        operation_id: "op".into(),
    }
}
const FRED_SERIES: &str = r#"{"realtime_start":"2020-01-01","realtime_end":"2020-01-01","seriess":[{"id":"GDP","realtime_start":"2020-01-01","realtime_end":"2020-01-01","title":"Gross Domestic Product","observation_start":"1947-01-01","observation_end":"2019-07-01","frequency":"Quarterly","frequency_short":"Q","units":"Billions of Dollars","units_short":"Bil. of $","seasonal_adjustment":"Seasonally Adjusted Annual Rate","last_updated":"2019-12-20 07:56:02-06","popularity":93,"notes":"BEA Account Code: A191RC"}]}"#;
const FRED_OBSERVATIONS: &str = r#"{"realtime_start":"2020-01-01","realtime_end":"2020-01-01","observation_start":"1600-01-01","observation_end":"9999-12-31","units":"lin","output_type":1,"file_type":"json","order_by":"observation_date","sort_order":"asc","count":3,"offset":0,"limit":100000,"observations":[{"realtime_start":"2020-01-01","realtime_end":"2020-01-01","date":"1947-01-01","value":"243.164"},{"realtime_start":"2020-01-01","realtime_end":"2020-01-01","date":"1947-04-01","value":"."},{"realtime_start":"2020-01-01","realtime_end":"2020-01-01","date":"1947-07-01","value":"250.1\"x"}]}"#;

#[test]
fn fred_keeps_its_csv_and_acquisition_fields_and_never_records_the_key() {
    let r = fred();
    let requests = r.requests().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|q| q.url.as_str().contains(KEY)));
    assert!(
        requests[1].url.as_str().contains("limit=100000")
            && requests[1].url.as_str().contains("units=lin")
    );
    let c = r
        .capture(vec![ok(FRED_SERIES), ok(FRED_OBSERVATIONS)], AT)
        .unwrap();
    assert_eq!(
        String::from_utf8(c.csv).unwrap(),
        "date,value,realtime_start,realtime_end\n\"1947-01-01\",\"243.164\",\"2020-01-01\",\"2020-01-01\"\n\"1947-04-01\",\".\",\"2020-01-01\",\"2020-01-01\"\n\"1947-07-01\",\"250.1\"\"x\",\"2020-01-01\",\"2020-01-01\"\n"
    );
    assert_eq!(c.title, "GDP · 2020-01-01");
    let a = serde_json::to_value(&c.acquisition).unwrap();
    assert_eq!(
        a,
        json!({"provider":"FRED/ALFRED","source":"https://fred.stlouisfed.org/series/GDP","retrievedAt":AT,"requestedVintage":"2020-01-01","returnedVintage":"2020-01-01","seriesIds":["GDP"],"units":"Billions of Dollars","frequency":"Quarterly","transformation":"lin (untransformed)"})
    );
    let p = &c.provenance;
    assert_eq!(p.notice.as_deref(), Some(fred::NOTICE));
    assert!(fred::NOTICE.starts_with("This product uses the FRED® API but is not endorsed or certified by the Federal Reserve Bank of St. Louis."));
    assert!(fred::NOTICE.contains("agree to be bound by the FRED® API Terms of Use"));
    assert_eq!(p.last_updated.as_deref(), Some("2019-12-20 07:56:02-06"));
    assert!(
        p.citation
            .contains("Gross Domestic Product [GDP], vintage 2020-01-01")
            && p.citation.contains("October 28, 2026")
    );
    assert_eq!(p.terms_reviewed, TERMS_REVIEWED);
    let recorded = serde_json::to_string(p).unwrap() + &r.query().to_string();
    assert!(!recorded.contains(KEY), "the key is never recorded");
    assert_eq!(p.request_urls.len(), 2);
}
#[test]
fn fred_refuses_bad_input_truncation_and_redacts_errors() {
    let mut r = fred();
    r.series_id = "GDP;DROP".into();
    assert!(r.requests().is_err());
    let mut r = fred();
    r.vintage = "2020-13-01".into();
    assert!(r.requests().is_err());
    let mut r = fred();
    r.api_key = "short".into();
    assert!(match r.requests() {
        Err(e) => e.message.contains("API key"),
        Ok(_) => false,
    });
    let r = fred();
    let truncated = FRED_OBSERVATIONS.replace("\"count\":3", "\"count\":4");
    assert!(r
        .capture(vec![ok(FRED_SERIES), ok(&truncated)], AT)
        .unwrap_err()
        .message
        .contains("truncated"));
    let error = format!(
        r#"{{"error_code":400,"error_message":"Bad Request. The api_key {KEY} is not registered."}}"#
    );
    let e = r
        .capture(vec![status(400, &error), ok(FRED_OBSERVATIONS)], AT)
        .unwrap_err()
        .message;
    assert!(
        e.contains("HTTP 400") && e.contains("not registered") && !e.contains(KEY),
        "{e}"
    );
    assert!(r.capture(vec![ok(FRED_SERIES)], AT).is_err());
}

const DBNOMICS: &str = r#"{"_meta":{"args":{"dataset_code":"WEO:2024-10","provider_code":"IMF","series_code":"USA.NGDP_RPCH.pcent_change"},"version":"22.1.17"},"dataset":{"code":"WEO:2024-10","dimensions_codes_order":["weo-country","weo-subject","unit"],"dimensions_labels":{"unit":"Unit","weo-country":"WEO Country","weo-subject":"WEO Subject"},"dimensions_values_labels":{"unit":{"pcent_change":"Percent change"},"weo-country":{"USA":"United States"},"weo-subject":{"NGDP_RPCH":"Gross domestic product, constant prices"}},"indexed_at":"2024-10-31T02:17:18.509Z","name":"World Economic Outlook by countries","nb_series":8624,"provider_code":"IMF","provider_name":"International Monetary Fund"},"errors":null,"provider":{"code":"IMF","indexed_at":"2025-09-04T02:25:25.926Z","name":"International Monetary Fund","region":"World","slug":"imf","terms_of_use":"http://datahelp.imf.org/tos","website":"https://www.imf.org/"},"series":{"docs":[{"@frequency":"annual","dataset_code":"WEO:2024-10","dataset_name":"World Economic Outlook by countries","dimensions":{"unit":"pcent_change","weo-country":"USA","weo-subject":"NGDP_RPCH"},"indexed_at":"2024-10-31T02:17:18.509Z","period":["1980","1981","1982"],"period_start_day":["1980-01-01","1981-01-01","1982-01-01"],"provider_code":"IMF","series_code":"USA.NGDP_RPCH.pcent_change","series_name":"United States – Gross domestic product, constant prices (NGDP_RPCH) – Percent change","value":[-0.257,"NA",-1.803]}],"facets":null,"limit":1000,"num_found":1,"offset":0}}"#;
const DBNOMICS_404: &str =
    r#"{"_meta":{"version":"22.1.17"},"message":"Series 'IMF/WEO:2024-10/NOPE.XX' not found"}"#;

fn dbnomics(id: &str) -> DbnomicsQuery {
    DbnomicsQuery {
        series_id: id.into(),
    }
}
#[test]
fn dbnomics_series_keeps_na_observations_and_records_provider_terms() {
    let q = dbnomics(" IMF/WEO:2024-10/USA.NGDP_RPCH.pcent_change ");
    assert_eq!(
        q.requests().unwrap()[0].url.as_str(),
        "https://api.db.nomics.world/v22/series/IMF/WEO:2024-10/USA.NGDP_RPCH.pcent_change?observations=1&metadata=true&format=json"
    );
    let c = q.capture(vec![ok(DBNOMICS)], AT).unwrap();
    assert_eq!(
        String::from_utf8(c.csv.clone()).unwrap(),
        "period,period_start_day,value\n\"1980\",\"1980-01-01\",\"-0.257\"\n\"1981\",\"1981-01-01\",\"NA\"\n\"1982\",\"1982-01-01\",\"-1.803\"\n"
    );
    assert_eq!(c.acquisition.provider, "DBnomics");
    assert_eq!(c.acquisition.units.as_deref(), Some("Percent change"));
    assert_eq!(c.acquisition.frequency.as_deref(), Some("annual"));
    assert_eq!(
        c.acquisition.series_ids,
        vec!["IMF/WEO:2024-10/USA.NGDP_RPCH.pcent_change".to_string()]
    );
    let p = &c.provenance;
    assert_eq!(p.connector, "dbnomics");
    assert_eq!(p.last_updated.as_deref(), Some("2024-10-31T02:17:18.509Z"));
    assert!(
        p.licence.contains("Open Database License")
            && p.licence.contains("http://datahelp.imf.org/tos")
    );
    let notice = p.notice.as_deref().unwrap();
    assert!(notice.contains("DBnomics") && notice.contains("International Monetary Fund"));
    assert!(notice.contains("permission for bulk automated download"));
    assert!(p
        .citation
        .contains("World Economic Outlook by countries (WEO:2024-10)"));
    assert_eq!(p.terms_url, "https://docs.db.nomics.world/legal-terms");
    let profile = crate::workbench::data::csv::profile(&c.csv, "csv").unwrap();
    assert_eq!((profile.rows, profile.columns[2].missing), (3, 1));
}
#[test]
fn dbnomics_refuses_bad_ids_missing_series_and_ragged_values() {
    for id in [
        "IMF/WEO",
        "IMF/WEO:2024-10/USA NGDP",
        "IMF//X",
        "a/b/c/d",
        "IMF/WEO/%2e%2e",
    ] {
        assert!(dbnomics(id).requests().is_err(), "{id}");
    }
    let q = dbnomics("IMF/WEO:2024-10/NOPE.XX");
    let e = q
        .capture(vec![status(404, DBNOMICS_404)], AT)
        .unwrap_err()
        .message;
    assert!(e.contains("HTTP 404") && e.contains("not found"), "{e}");
    let two = DBNOMICS.replace("\"num_found\":1", "\"num_found\":2");
    assert!(q.capture(vec![ok(&two)], AT).is_err());
    let ragged = DBNOMICS.replace("[-0.257,\"NA\",-1.803]", "[-0.257]");
    assert!(q
        .capture(vec![ok(&ragged)], AT)
        .unwrap_err()
        .message
        .contains("different lengths"));
    let text = DBNOMICS.replace("\"NA\",", "\"n/a\",");
    assert!(q.capture(vec![ok(&text)], AT).is_err());
    assert!(q
        .capture(vec![ok("<html>")], AT)
        .unwrap_err()
        .message
        .contains("malformed JSON"));
}

const WB_INDICATOR: &str = r#"[{"page":1,"pages":1,"per_page":"50","total":1},[{"id":"SL.UEM.TOTL.ZS","name":"Unemployment, total (% of total labor force) (modeled ILO estimate)","unit":"","source":{"id":"2","value":"World Development Indicators"},"sourceNote":"Unemployment refers to the share of the labor force that is without work but available for and seeking employment.","sourceOrganization":"ILO Modelled Estimates database (ILOEST), International Labour Organization (ILO)","topics":[{"id":"10","value":"Social Protection & Labor"}]}]]"#;
const WB_DATA: &str = r#"[{"page":1,"pages":1,"per_page":20000,"total":3,"sourceid":"2","lastupdated":"2026-10-08"},[{"indicator":{"id":"SL.UEM.TOTL.ZS","value":"Unemployment, total (% of total labor force) (modeled ILO estimate)"},"country":{"id":"SS","value":"South Sudan"},"countryiso3code":"SSD","date":"2012","value":12.803,"unit":"","obs_status":"","decimal":1},{"indicator":{"id":"SL.UEM.TOTL.ZS","value":"Unemployment, total (% of total labor force) (modeled ILO estimate)"},"country":{"id":"US","value":"United States"},"countryiso3code":"USA","date":"2012","value":8.069,"unit":"","obs_status":"","decimal":1},{"indicator":{"id":"SL.UEM.TOTL.ZS","value":"Unemployment, total (% of total labor force) (modeled ILO estimate)"},"country":{"id":"US","value":"United States"},"countryiso3code":"USA","date":"2011","value":null,"unit":"","obs_status":"","decimal":1}]]"#;
const WB_ERROR: &str = r#"[{"message":[{"id":"120","key":"Invalid value","value":"The provided parameter value is not valid"}]}]"#;

fn world_bank(countries: &str, dates: Option<&str>) -> WorldBankQuery {
    WorldBankQuery {
        indicator: "SL.UEM.TOTL.ZS".into(),
        countries: countries.into(),
        date_range: dates.map(str::to_string),
    }
}
#[test]
fn world_bank_indicator_records_cc_by_terms_and_null_values_as_missing() {
    let q = world_bank("usa; ssd", Some("2011:2012"));
    let urls: Vec<String> = q
        .requests()
        .unwrap()
        .into_iter()
        .map(|r| r.url.into())
        .collect();
    assert_eq!(
        urls[0],
        "https://api.worldbank.org/v2/indicator/SL.UEM.TOTL.ZS?format=json"
    );
    assert_eq!(
        urls[1],
        "https://api.worldbank.org/v2/country/USA;SSD/indicator/SL.UEM.TOTL.ZS?format=json&per_page=20000&date=2011%3A2012"
    );
    let c = q.capture(vec![ok(WB_INDICATOR), ok(WB_DATA)], AT).unwrap();
    let csv = String::from_utf8(c.csv.clone()).unwrap();
    assert!(csv.starts_with("country_id,country_iso3,country,date,value,obs_status,unit,decimal\n\"SS\",\"SSD\",\"South Sudan\",\"2012\",\"12.803\",\"\",\"\",\"1\"\n"));
    assert!(csv.ends_with("\"US\",\"USA\",\"United States\",\"2011\",\"\",\"\",\"\",\"1\"\n"));
    let profile = crate::workbench::data::csv::profile(&c.csv, "csv").unwrap();
    assert_eq!((profile.rows, profile.columns[4].missing), (3, 1));
    assert_eq!(
        c.title,
        "Unemployment, total (% of total labor force) (modeled ILO estimate) · USA;SSD"
    );
    let p = &c.provenance;
    assert_eq!(
        (p.connector.as_str(), p.last_updated.as_deref()),
        ("world_bank", Some("2026-10-08"))
    );
    assert!(
        p.licence.contains("CC BY 4.0") && p.licence.contains("International Labour Organization")
    );
    assert_eq!(
        p.notice.as_deref(),
        Some("Source: World Bank, World Development Indicators. Licensed under CC BY 4.0 unless the indicator's terms say otherwise.")
    );
    assert!(p
        .citation
        .contains("https://data.worldbank.org/indicator/SL.UEM.TOTL.ZS"));
    assert_eq!(c.acquisition.units, None);
    assert_eq!(
        world_bank("all", None).requests().unwrap()[1].url.path(),
        "/v2/country/all/indicator/SL.UEM.TOTL.ZS"
    );
}
#[test]
fn world_bank_refuses_invalid_queries_api_errors_and_partial_pages() {
    for (countries, dates) in [
        ("", None),
        ("USA;TOOLONG", None),
        ("US-A", None),
        ("USA", Some("2010-2012")),
        ("USA", Some("2010:2011:2012")),
        ("USA", Some("2010M13x")),
    ] {
        assert!(
            world_bank(countries, dates).requests().is_err(),
            "{countries} {dates:?}"
        );
    }
    assert!(world_bank("USA", Some("2010Q1:2012Q4")).requests().is_ok());
    let mut bad = world_bank("USA", None);
    bad.indicator = "NY GDP".into();
    assert!(bad.requests().is_err());
    let q = world_bank("USA", None);
    assert!(q
        .capture(vec![ok(WB_ERROR), ok(WB_DATA)], AT)
        .unwrap_err()
        .message
        .contains("Invalid value"));
    let partial = WB_DATA
        .replace("\"total\":3", "\"total\":30000")
        .replace("\"pages\":1", "\"pages\":2");
    assert!(q
        .capture(vec![ok(WB_INDICATOR), ok(&partial)], AT)
        .unwrap_err()
        .message
        .contains("narrow"));
    let empty =
        r#"[{"page":0,"pages":0,"per_page":20000,"total":0,"lastupdated":"2026-10-08"},null]"#;
    let none = q.capture(vec![ok(WB_INDICATOR), ok(empty)], AT).unwrap();
    assert_eq!(
        crate::workbench::data::csv::profile(&none.csv, "csv")
            .unwrap()
            .rows,
        0
    );
    assert!(q
        .capture(vec![ok(WB_INDICATOR), status(503, "busy")], AT)
        .unwrap_err()
        .message
        .contains("HTTP 503"));
}

#[test]
fn series_requests_deserialize_and_record_captures_or_failures() {
    let request: SeriesRequest = serde_json::from_value(json!({"workspaceId":"w","operationId":"op","query":{"connector":"world_bank","indicator":"SL.UEM.TOTL.ZS","countries":"USA","dateRange":null}})).unwrap();
    assert!(matches!(request.query, SeriesQuery::WorldBank(_)));
    let request: SeriesRequest = serde_json::from_value(json!({"workspaceId":"w","operationId":"op","query":{"connector":"dbnomics","seriesId":"IMF/WEO:2024-10/USA.NGDP_RPCH.pcent_change"}})).unwrap();
    assert!(
        serde_json::from_value::<SeriesQuery>(json!({"connector":"bls","seriesId":"x"})).is_err()
    );

    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_at(&temp.path().join("store")).unwrap();
    let ws = store
        .create_workspace(CreateWorkspaceRequest {
            name: "Connectors".into(),
            root: None,
            operation_id: "workspace".into(),
        })
        .unwrap()
        .record
        .id;
    let SeriesQuery::Dbnomics(q) = &request.query else {
        unreachable!()
    };
    let capture = q.capture(vec![ok(DBNOMICS)], AT);
    let record = record_series(&store, &ws, &request.query, capture, "capture").unwrap();
    assert_eq!(record.kind, "dataset");
    assert_eq!(record.body["connector"]["connector"], "dbnomics");
    assert!(record.body["connector"]["notice"]
        .as_str()
        .unwrap()
        .contains("DBnomics"));
    assert_eq!(record.body["acquisition"]["provider"], "DBnomics");
    assert!(record.body["limitations"]
        .to_string()
        .contains("repeat its notice"));
    let failed = record_series(
        &store,
        &ws,
        &request.query,
        Err(WorkbenchError::invalid("offline")),
        "failed",
    )
    .unwrap();
    assert_eq!(failed.kind, "acquisition");
    assert_eq!(failed.body["state"], "failed");
    assert_eq!(failed.body["provider"], "DBnomics");
    assert_eq!(
        failed.body["query"]["seriesId"],
        "IMF/WEO:2024-10/USA.NGDP_RPCH.pcent_change"
    );
}

#[tokio::test]
#[ignore = "live DBnomics API; run explicitly when qualifying connectors"]
async fn live_dbnomics_series() {
    let c = fetch(&dbnomics("IMF/WEO:2024-10/USA.NGDP_RPCH.pcent_change"))
        .await
        .unwrap();
    assert!(c.csv.len() > 100);
    assert!(c.provenance.notice.unwrap().contains("DBnomics"));
}
#[tokio::test]
#[ignore = "live World Bank API; run explicitly when qualifying connectors"]
async fn live_world_bank_indicator() {
    let c = fetch(&world_bank("USA;BRA", Some("2015:2020")))
        .await
        .unwrap();
    assert!(String::from_utf8(c.csv).unwrap().contains("\"BRA\""));
    assert!(c.provenance.licence.contains("CC BY 4.0"));
}
