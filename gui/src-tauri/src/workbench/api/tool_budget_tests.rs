//! Long turns against the request limit, and text a stopped turn had streamed.
use super::tests::{connection, read_tool, scripted_server, scripted_turn};
use super::*;

/// A tool result carrying one image of about `bytes`, as a rendered page has.
fn page(number: usize, bytes: usize) -> Value {
    // Sized as base64 and marked, so a test can tell the pages apart.
    let data = format!("PAGE{number:04}{}", "A".repeat(bytes * 4 / 3));
    json!({"success":true,"contentItems":[
        {"type":"inputText","text":format!("page-{number}-result")},
        {"type":"inputImage","imageUrl":format!("data:image/jpeg;base64,{data}")}
    ]})
}

/// The messages of a turn that has read `pages` pages, one per round.
fn read_pages(provider: Provider, pages: usize, bytes: usize) -> Vec<Value> {
    let mut messages = wire::initial(provider, "instructions", "question");
    for number in 0..pages {
        let call = wire::Call {
            id: format!("call-{number}"),
            name: "workbench_paper_page".into(),
            response_id: None,
            arguments: json!({}),
        };
        messages.push(json!({"role":"assistant","content":format!("Reading page {number}")}));
        messages.extend(wire::tool_results(
            provider,
            vec![(&call, page(number, bytes))],
        ));
    }
    messages
}

const PROVIDERS: [Provider; 4] = [
    Provider::OpenAi,
    Provider::Local,
    Provider::Anthropic,
    Provider::Google,
];

#[test]
fn a_long_page_read_stays_inside_the_request_limit_and_keeps_its_newest_pages() {
    for provider in PROVIDERS {
        let conn = connection(provider);
        // Twenty pages, at the size that used to end a turn after about ten.
        let mut messages = read_pages(provider, 20, 600 * 1024);
        assert!(messages.iter().map(|m| m.to_string().len()).sum::<usize>() > wire::MAX_BODY);
        wire::make_room(&conn, "instructions", &mut messages, &[read_tool()]);
        let body = wire::body(&conn, "instructions", &messages, &[read_tool()]);
        let sent = body.to_string();
        assert!(sent.len() <= wire::MAX_BODY, "{provider:?}: {}", sent.len());
        assert!(sent.contains("was left out"), "{provider:?}");
        let image = |number: usize| sent.contains(&format!("PAGE{number:04}"));
        assert!(!image(1), "{provider:?} kept the oldest page");
        assert!(image(19), "{provider:?} dropped the newest page");
        // The text of every result stays, so the model can ask for a page again.
        for number in 0..20 {
            assert!(
                sent.contains(&format!("page-{number}-result")),
                "{provider:?}"
            );
        }
    }
}

#[test]
fn every_model_call_of_a_turn_makes_room_first() {
    let source = include_str!("drive.rs");
    let round = &source[source.find("for round in 0..32").unwrap()..];
    let room = round.find("wire::make_room(").unwrap();
    assert!(room < round.find("wire::body(").unwrap());
}

#[test]
fn a_request_that_fits_is_left_exactly_as_it_was() {
    for provider in PROVIDERS {
        let mut messages = read_pages(provider, 3, 1024);
        let before = messages.clone();
        wire::make_room(
            &connection(provider),
            "instructions",
            &mut messages,
            &[read_tool()],
        );
        assert_eq!(messages, before);
    }
}

#[test]
fn many_small_images_are_held_to_what_every_provider_accepts() {
    for provider in PROVIDERS {
        let mut messages = read_pages(provider, 30, 1024);
        wire::make_room(
            &connection(provider),
            "instructions",
            &mut messages,
            &[read_tool()],
        );
        let sent = serde_json::to_string(&messages).unwrap();
        assert_eq!(sent.matches("was left out").count(), 10, "{provider:?}");
    }
}

#[test]
fn signed_assistant_parts_are_never_touched() {
    let mut messages = read_pages(Provider::Google, 20, 600 * 1024);
    let signed = json!({"role":"model","parts":[{"functionCall":{"name":"x","args":{}},"thoughtSignature":"signed"}]});
    messages.insert(1, signed.clone());
    wire::make_room(
        &connection(Provider::Google),
        "instructions",
        &mut messages,
        &[],
    );
    assert_eq!(messages[1], signed);
    // A Gemini function response keeps its place, followed only by media.
    for message in messages.iter().filter(|m| m["role"] == "user") {
        let parts = message["parts"].as_array().unwrap();
        assert!(parts
            .iter()
            .all(|part| part.get("text").is_none() || message == &messages[0]));
    }
}

#[test]
fn a_result_too_large_for_a_request_goes_back_to_the_model() {
    let small = page(1, 1024);
    assert_eq!(drive::bounded_result(small.clone()), small);
    let scanned = drive::bounded_result(page(2, 3 * 1024 * 1024 + 512 * 1024));
    assert_eq!(scanned["success"], false);
    let text = scanned["contentItems"][0]["text"].as_str().unwrap();
    assert!(text.contains("Do not repeat"), "{text}");
    assert!(scanned.to_string().len() < 1024);
}

/// Streams `chunks` of a reply, then closes or holds the connection.
fn delta(text: &str) -> String {
    format!(
        "data: {}\n\n",
        json!({"choices":[{"index":0,"delta":{"content":text}}]})
    )
}

#[tokio::test]
async fn a_lost_connection_keeps_the_text_that_had_streamed() {
    let (endpoint, server) =
        scripted_server(vec![("200 OK", delta("The first half") + &delta(" of it"))]).await;
    let mut conn = connection(Provider::Local);
    conn.endpoint = endpoint;
    let (store, session, turn) = scripted_turn("lost-reply");
    let (_cancel, receiver) = watch::channel(false);
    let events = crate::workbench::turn_loop::RecordedEvents::default();
    Box::pin(drive::run(
        &wire::client().unwrap(),
        &conn,
        &events,
        &turn,
        "instructions",
        "question",
        &[],
        drive::turn_clock(),
        receiver,
    ))
    .await
    .unwrap_err();
    server.await.unwrap();
    turn.save_unfinished(&events).await;

    let items = store.conversation_snapshot(&session).unwrap().items;
    let item = items.last().unwrap();
    assert_eq!(item.payload["text"], "The first half of it");
    assert_eq!(item.payload["phase"], "incomplete");
    assert_eq!(item.provider_item_id, format!("{}-assistant-0", turn.id));
    assert!(item.is_final);
    assert!(crate::workbench::history::recent_context(&store, &session)
        .unwrap()
        .contains("The first half of it"));
    // Saved once: a second save has nothing left to write.
    turn.save_unfinished(&events).await;
    assert_eq!(
        store.conversation_snapshot(&session).unwrap().items.len(),
        items.len()
    );
}

#[tokio::test]
async fn stop_keeps_the_text_that_had_streamed() {
    use std::time::Duration;
    let endpoint = super::tests::trickling_server(
        vec![delta("A partial answer")],
        Duration::ZERO,
        Duration::from_secs(30),
    )
    .await;
    let mut conn = connection(Provider::Local);
    conn.endpoint = endpoint;
    let (store, session, turn) = scripted_turn("stopped-reply");
    let (cancel, receiver) = watch::channel(false);
    let events = crate::workbench::turn_loop::RecordedEvents::default();
    let stop = async {
        while !events
            .events
            .lock()
            .unwrap()
            .iter()
            .any(|event| matches!(event, NormalizedEvent::AgentMessageDelta { .. }))
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        cancel.send(true).unwrap();
    };
    let client = wire::client().unwrap();
    let specs: [Value; 0] = [];
    let run = Box::pin(drive::run(
        &client,
        &conn,
        &events,
        &turn,
        "instructions",
        "question",
        &specs,
        drive::turn_clock(),
        receiver,
    ));
    let (result, ()) =
        tokio::time::timeout(Duration::from_secs(10), async { tokio::join!(run, stop) })
            .await
            .unwrap();
    assert_eq!(result, Err("interrupted".into()));
    turn.save_unfinished(&events).await;
    let items = store.conversation_snapshot(&session).unwrap().items;
    assert_eq!(items.last().unwrap().payload["text"], "A partial answer");
    assert_eq!(items.last().unwrap().payload["phase"], "incomplete");
}

#[tokio::test]
async fn a_saved_reply_leaves_nothing_for_a_later_failure_to_repeat() {
    let (endpoint, server) = scripted_server(vec![
        (
            "200 OK",
            format!(
                "{}data: [DONE]\n\n",
                delta("I will read it.")
                    + &format!(
                        "data: {}\n\n",
                        json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call-0","type":"function","function":{"name":"scripted_read","arguments":"{\"path\":\"a.tex\"}"}}]}}]})
                    )
                    + &format!(
                        "data: {}\n\n",
                        json!({"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]})
                    )
            ),
        ),
        ("500 Internal Server Error", "{}".to_string()),
    ])
    .await;
    let mut conn = connection(Provider::Local);
    conn.endpoint = endpoint;
    let (_store, _session, turn) = scripted_turn("saved-reply");
    let (_cancel, receiver) = watch::channel(false);
    let events = crate::workbench::turn_loop::RecordedEvents::default();
    Box::pin(drive::run(
        &wire::client().unwrap(),
        &conn,
        &events,
        &turn,
        "instructions",
        "question",
        &[read_tool()],
        drive::turn_clock(),
        receiver,
    ))
    .await
    .unwrap_err();
    server.await.unwrap();
    assert!(turn.unsaved.take().is_empty());
}

#[test]
fn a_failed_capture_warns_and_leaves_the_turn_alone() {
    let events = crate::workbench::turn_loop::RecordedEvents::default();
    crate::workbench::conversation_tools::report_capture(&events, 7, Ok(()));
    assert!(events.events.lock().unwrap().is_empty());
    crate::workbench::conversation_tools::report_capture(
        &events,
        7,
        Err(WorkbenchError::invalid("A file is over 32 MB")),
    );
    let recorded = events.events.lock().unwrap();
    let [NormalizedEvent::ProtocolWarning { epoch: 7, message }] = recorded.as_slice() else {
        panic!("{recorded:?}");
    };
    assert!(message.contains("A file is over 32 MB"), "{message}");
    assert!(message.contains("refresh the working copy"), "{message}");
}
