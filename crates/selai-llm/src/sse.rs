//! Minimal Server-Sent Events parser: `event:` and `data:` fields, blank-line
//! dispatch, comments ignored. Lines are capped so a hostile server cannot make
//! us buffer without bound.

use crate::LlmError;

const MAX_LINE_BYTES: usize = 1 << 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

#[derive(Debug, Default)]
pub struct SseParser {
    buf: Vec<u8>,
    event: Option<String>,
    data: Vec<String>,
}

impl SseParser {
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<SseEvent>, LlmError> {
        self.buf.extend_from_slice(chunk);
        let mut out = Vec::new();
        while let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
            let mut line: Vec<u8> = self.buf.drain(..=pos).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let line = String::from_utf8(line)
                .map_err(|_| LlmError::protocol("stream contained invalid UTF-8"))?;
            if line.is_empty() {
                if let Some(ev) = self.dispatch() {
                    out.push(ev);
                }
                continue;
            }
            self.field(&line);
        }
        if self.buf.len() > MAX_LINE_BYTES {
            return Err(LlmError::protocol("stream line exceeds the size limit"));
        }
        Ok(out)
    }

    /// Dispatches a final event that was not followed by a blank line.
    pub fn finish(&mut self) -> Option<SseEvent> {
        if !self.buf.is_empty() {
            let rest = std::mem::take(&mut self.buf);
            let line = String::from_utf8_lossy(&rest).into_owned();
            self.field(line.trim_end_matches('\r'));
        }
        self.dispatch()
    }

    fn field(&mut self, line: &str) {
        if line.starts_with(':') {
            return;
        }
        let (name, value) = match line.split_once(':') {
            Some((n, v)) => (n, v.strip_prefix(' ').unwrap_or(v)),
            None => (line, ""),
        };
        match name {
            "event" => self.event = Some(value.to_owned()),
            "data" => self.data.push(value.to_owned()),
            _ => {}
        }
    }

    fn dispatch(&mut self) -> Option<SseEvent> {
        if self.event.is_none() && self.data.is_empty() {
            return None;
        }
        Some(SseEvent {
            event: self.event.take(),
            data: std::mem::take(&mut self.data).join("\n"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_events_across_arbitrary_chunk_boundaries() {
        let stream = b"event: a\ndata: {\"x\":1}\n\n: keepalive\n\ndata: second\r\n\r\n";
        for cut in 1..stream.len() {
            let mut p = SseParser::default();
            let mut events = p.push(&stream[..cut]).expect("push");
            events.extend(p.push(&stream[cut..]).expect("push"));
            assert_eq!(events.len(), 2, "cut at {cut}");
            assert_eq!(events[0].event.as_deref(), Some("a"));
            assert_eq!(events[0].data, "{\"x\":1}");
            assert_eq!(events[1].data, "second");
        }
    }

    #[test]
    fn final_event_without_blank_line_is_flushed() {
        let mut p = SseParser::default();
        assert!(p.push(b"data: [DONE]").expect("push").is_empty());
        assert_eq!(p.finish().expect("flush").data, "[DONE]");
    }

    #[test]
    fn oversized_line_is_refused() {
        let mut p = SseParser::default();
        let big = vec![b'a'; MAX_LINE_BYTES + 1];
        assert!(p.push(&big).is_err());
    }
}
