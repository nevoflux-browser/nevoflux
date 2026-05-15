//! JSONL file writer for developer trace output.

use crate::trace::models::FullTraceSpan;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Appends full trace spans to a JSONL file.
pub struct TraceFileWriter {
    file_path: PathBuf,
}

impl TraceFileWriter {
    /// Create a new writer for the given session.
    /// File path: `{traces_dir}/{date}-{session_id}.jsonl`
    pub fn new(traces_dir: &Path, session_id: &str) -> std::io::Result<Self> {
        fs::create_dir_all(traces_dir)?;
        let date = chrono::Utc::now().format("%Y-%m-%d");
        let file_path = traces_dir.join(format!("{}-{}.jsonl", date, session_id));
        Ok(Self { file_path })
    }

    /// Append a span to the JSONL file.
    pub fn append(&self, span: &FullTraceSpan) -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path)?;
        let line = serde_json::to_string(span).map_err(std::io::Error::other)?;
        writeln!(file, "{}", line)?;
        Ok(())
    }

    /// Get the file path.
    pub fn path(&self) -> &Path {
        &self.file_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::models::{FullTraceSpan, SpanType};
    use tempfile::TempDir;

    fn sample_span() -> FullTraceSpan {
        FullTraceSpan {
            ts: "2026-02-04T10:00:01Z".to_string(),
            session: "test-session".to_string(),
            iter: 0,
            span_type: SpanType::ToolExec,
            tool: Some("write_file".to_string()),
            params: Some(serde_json::json!({"path": "/tmp/test"})),
            request: None,
            response: None,
            result: Some(serde_json::json!({"success": true})),
            duration_ms: 15,
            success: true,
        }
    }

    #[test]
    fn test_writer_creates_file_and_appends() {
        let tmp = TempDir::new().unwrap();
        let writer = TraceFileWriter::new(tmp.path(), "sess-001").unwrap();

        writer.append(&sample_span()).unwrap();
        writer.append(&sample_span()).unwrap();

        let content = std::fs::read_to_string(writer.path()).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);

        // Each line should be valid JSON
        let _: FullTraceSpan = serde_json::from_str(lines[0]).unwrap();
    }

    #[test]
    fn test_writer_creates_directory() {
        let tmp = TempDir::new().unwrap();
        let nested = tmp.path().join("traces").join("nested");
        let writer = TraceFileWriter::new(&nested, "sess-001").unwrap();

        writer.append(&sample_span()).unwrap();
        assert!(writer.path().exists());
    }
}
