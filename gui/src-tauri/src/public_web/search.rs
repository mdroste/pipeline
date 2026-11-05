//! Public search without a model-provider credential. RSS is parsed as data,
//! never rendered as HTML. Retrieval uses the same pinned-DNS, redirect and
//! body limits as page reads; search snippets remain untrusted sources.
use super::*;

pub(crate) async fn search(query: &str) -> WebResult<Value> {
    let query = query.trim();
    if query.is_empty() || query.len() > 2000 || query.chars().any(char::is_control) {
        return Err(invalid(
            "Use a search query through 2,000 bytes without control characters",
        ));
    }
    let mut endpoint = Url::parse("https://www.bing.com/search").unwrap();
    endpoint
        .query_pairs_mut()
        .append_pair("format", "rss")
        .append_pair("q", query);
    let response = download(endpoint.as_str(), 2 * 1024 * 1024).await?;
    let results = parse(&response.bytes)?;
    Ok(
        json!({"query":query,"provider":"Bing","results":results,"retrievedAt":chrono::Utc::now().to_rfc3339(),"authority":"Untrusted search results. Read the original pages before relying on their claims; cite the original URLs."}),
    )
}

fn prefix(text: &str, limit: usize) -> &str {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn parse(bytes: &[u8]) -> WebResult<Vec<Value>> {
    use quick_xml::{events::Event, Reader};
    let mut reader = Reader::from_reader(bytes);
    let mut field = String::new();
    let mut item: Option<Value> = None;
    let mut results = Vec::new();
    let mut rss = false;
    loop {
        match reader
            .read_event()
            .map_err(|_| invalid("Search returned an invalid response"))?
        {
            Event::Start(tag) => match tag.name().as_ref() {
                b"rss" => rss = true,
                b"item" => item = Some(json!({"title":"","url":"","snippet":""})),
                b"title" if item.is_some() => field = "title".into(),
                b"link" if item.is_some() => field = "url".into(),
                b"description" if item.is_some() => field = "snippet".into(),
                _ => (),
            },
            Event::Text(text) => {
                if let Some(item) = &mut item {
                    if !field.is_empty() {
                        let text = text
                            .xml10_content()
                            .map_err(|_| invalid("Invalid search text"))?;
                        let mut content = item[&field].as_str().unwrap_or_default().to_owned();
                        content.push_str(&text);
                        item[&field] = json!(content);
                    }
                }
            }
            Event::GeneralRef(reference) => {
                if let Some(item) = &mut item {
                    if !field.is_empty() {
                        let name = reference
                            .decode()
                            .map_err(|_| invalid("Invalid search entity"))?;
                        let decoded = quick_xml::escape::unescape(&format!("&{name};"))
                            .map_err(|_| invalid("Invalid search entity"))?
                            .into_owned();
                        let mut content = item[&field].as_str().unwrap_or_default().to_owned();
                        content.push_str(&decoded);
                        item[&field] = json!(content);
                    }
                }
            }
            Event::CData(text) => {
                if let Some(item) = &mut item {
                    if !field.is_empty() {
                        let text = text.decode().map_err(|_| invalid("Invalid search text"))?;
                        let mut content = item[&field].as_str().unwrap_or_default().to_owned();
                        content.push_str(&text);
                        item[&field] = json!(content);
                    }
                }
            }
            Event::End(tag) => {
                if tag.name().as_ref() == b"item" {
                    if let Some(mut item) = item.take() {
                        if url(item["url"].as_str().unwrap_or_default()).is_ok()
                            && results.len() < 10
                        {
                            for key in ["title", "snippet"] {
                                let text = ammonia::Builder::default()
                                    .tags(Default::default())
                                    .clean(item[key].as_str().unwrap_or_default())
                                    .to_string();
                                item[key] = json!(prefix(&text, 2048));
                            }
                            results.push(item);
                        }
                    }
                }
                field.clear();
            }
            Event::DocType(_) => return Err(invalid("Search returned an unsupported document")),
            Event::Eof => break,
            _ => (),
        }
    }
    if !rss {
        return Err(invalid(
            "Search is unavailable or requires verification. Retry later.",
        ));
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "Requires public internet; no model calls"]
    async fn public_search_returns_citable_results() {
        let result = search("macroeconomics research").await.unwrap();
        assert!(!result["results"].as_array().unwrap().is_empty());
        assert!(result["results"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| url(r["url"].as_str().unwrap()).is_ok()));
    }

    #[test]
    fn parses_citable_results_and_rejects_html_challenges() {
        let results=parse(br#"<rss><channel><item><title>A &amp; B</title><link>https://example.org/?a=1&amp;b=2</link><description><![CDATA[<b>Study</b>]]></description></item><item><link>file:///secret</link></item></channel></rss>"#).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["url"], "https://example.org/?a=1&b=2");
        assert_eq!(results[0]["snippet"], "Study");
        assert!(parse(b"<html>Verify you are human</html>").is_err());
        assert!(parse(b"<!DOCTYPE rss><rss/>").is_err());
    }
}
