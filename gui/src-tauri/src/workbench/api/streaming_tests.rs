use super::*;

fn assemble(provider: Provider, events: &[Value], done: bool) -> (wire::Reply, String) {
    let mut bytes = events
        .iter()
        .map(|v| format!("data: {v}\r\n\r\n"))
        .collect::<String>();
    if done {
        bytes.push_str("data: [DONE]\r\n\r\n");
    }
    let mut stream = streaming::Stream::new(provider);
    let mut text = String::new();
    // Every UTF-8 and CRLF boundary is split, including JSON argument fragments.
    for byte in bytes.as_bytes() {
        stream
            .push(&[*byte], &mut |delta| text.push_str(delta))
            .unwrap();
    }
    (
        wire::parse(provider, stream.finish().unwrap()).unwrap(),
        text,
    )
}

#[test]
fn openai_and_local_stream_text_tools_and_final_usage() {
    for provider in [Provider::OpenAi, Provider::Local] {
        let (reply, text) = assemble(
            provider,
            &[
                json!({"choices":[{"index":0,"delta":{"content":"Café ","tool_calls":[{"index":0,"id":"call","type":"function","function":{"name":"project_read","arguments":"{\"pa"}}]}}]}),
                json!({"choices":[{"index":0,"delta":{"content":"paper","tool_calls":[{"index":0,"function":{"arguments":"th\":\"paper.tex\"}"}}]},"finish_reason":"tool_calls"}]}),
                json!({"choices":[],"usage":{"total_tokens":12}}),
            ],
            true,
        );
        assert_eq!(text, "Café paper");
        assert_eq!(reply.text, text);
        assert_eq!(reply.calls[0].arguments, json!({"path":"paper.tex"}));
        assert_eq!(reply.usage["total_tokens"], 12);
    }
}

#[test]
fn anthropic_stream_retains_thinking_signatures_and_partial_tool_json() {
    let (reply, text) = assemble(
        Provider::Anthropic,
        &[
            json!({"type":"message_start","message":{"usage":{"input_tokens":10}}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"private"}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"signed"}}),
            json!({"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Reading"}}),
            json!({"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"call","name":"project_read","input":{}}}),
            json!({"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"{\"path\":"}}),
            json!({"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"\"paper.tex\"}"}}),
            json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":20}}),
            json!({"type":"message_stop"}),
        ],
        false,
    );
    assert_eq!(text, "Reading");
    assert_eq!(reply.text, text);
    assert_eq!(reply.message["content"][0]["signature"], "signed");
    assert_eq!(reply.calls[0].arguments["path"], "paper.tex");
    assert_eq!(reply.usage, json!({"input_tokens":10,"output_tokens":20}));
}

#[test]
fn gemini_stream_preserves_each_signed_part_without_showing_thoughts() {
    let (reply, text) = assemble(
        Provider::Google,
        &[
            json!({"candidates":[{"content":{"parts":[{"thought":true,"text":"private","thoughtSignature":"one"}]}}]}),
            json!({"candidates":[{"content":{"parts":[{"text":"Reading"},{"functionCall":{"name":"project_read","args":{"path":"paper.tex"}},"thoughtSignature":"two"}]},"finishReason":"STOP"}],"usageMetadata":{"totalTokenCount":9}}),
        ],
        false,
    );
    assert_eq!(text, "Reading");
    assert_eq!(reply.text, text);
    assert_eq!(reply.message["parts"][2]["thoughtSignature"], "two");
    assert_eq!(reply.calls.len(), 1);
}

#[test]
fn incomplete_or_failed_streams_never_yield_executable_calls() {
    for provider in [
        Provider::OpenAi,
        Provider::Anthropic,
        Provider::Google,
        Provider::Local,
    ] {
        let mut stream = streaming::Stream::new(provider);
        assert!(stream
            .push(
                b"data: {\"error\":{\"message\":\"secret\"}}\n\n",
                &mut |_| {}
            )
            .unwrap_err()
            .find("secret")
            .is_none());
        let mut stream = streaming::Stream::new(provider);
        stream.push(b"data: {", &mut |_| {}).unwrap();
        assert!(stream.finish().is_err());
    }
    let mut stream = streaming::Stream::new(Provider::Local);
    stream
        .push(
            b"data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n",
            &mut |_| {},
        )
        .unwrap();
    assert!(stream.finish().is_err());
    let mut stream = streaming::Stream::new(Provider::Local);
    assert!(stream
        .push(
            b"data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":999999}]}}]}\n\n",
            &mut |_| {}
        )
        .is_err());
}

#[test]
fn streamed_refusal_is_a_visible_terminal_response() {
    let (reply, text) = assemble(
        Provider::OpenAi,
        &[
            json!({"choices":[{"index":0,"delta":{"refusal":"Cannot help with that request."},"finish_reason":"stop"}]}),
        ],
        true,
    );
    assert_eq!(reply.text, text);
    assert_eq!(text, "Cannot help with that request.");
}
