//! Bounded record reads for provider processes and discovery.
pub struct BoundedLine {
    pub text: String,
    pub truncated: bool,
}

/// Read one newline-delimited record without allowing `lines()` to allocate an
/// attacker-controlled line before the caller's total-output cap can run.
pub async fn next_bounded_line<R>(
    reader: &mut R,
    limit: usize,
) -> std::io::Result<Option<BoundedLine>>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    use tokio::io::AsyncBufReadExt as _;
    let mut bytes = Vec::with_capacity(limit.min(64 * 1024));
    let mut truncated = false;
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return if bytes.is_empty() && !truncated {
                Ok(None)
            } else {
                Ok(Some(BoundedLine {
                    text: String::from_utf8_lossy(&bytes).into_owned(),
                    truncated,
                }))
            };
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(available.len(), |position| position + 1);
        let content_len = newline.unwrap_or(available.len());
        let remaining = limit.saturating_sub(bytes.len());
        let keep = content_len.min(remaining);
        bytes.extend_from_slice(&available[..keep]);
        truncated |= keep < content_len;
        reader.consume(consumed);
        if newline.is_some() {
            if bytes.last() == Some(&b'\r') {
                bytes.pop();
            }
            return Ok(Some(BoundedLine {
                text: String::from_utf8_lossy(&bytes).into_owned(),
                truncated,
            }));
        }
    }
}
