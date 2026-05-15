use std::collections::HashMap;

use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};

/// Metadata extracted from the blockquote header of a soul document.
#[derive(Debug, Clone, Default)]
pub struct DocumentMetadata {
    /// The protection level string, e.g. "L0-L1 | Safety boundaries immutable".
    pub protection_level: String,
    /// The last updated timestamp, if present.
    pub last_updated: Option<String>,
}

/// Parse a Markdown document into sections keyed by their `## ` heading text.
///
/// Each section's value is the extracted plain-text content of that section.
/// Paragraph and list item boundaries are normalized to newlines.
/// Only splits on level-2 (`##`) headings.
///
/// This output is suitable for content existence checks but NOT for
/// round-trip reconstruction of the original Markdown.
pub fn parse_sections(md: &str) -> HashMap<String, String> {
    let parser = Parser::new(md);

    let mut sections = HashMap::new();
    let mut current_heading: Option<String> = None;
    let mut heading_text = String::new();
    let mut in_h2_heading = false;
    let mut section_content = String::new();

    for event in parser {
        match event {
            Event::Start(Tag::Heading {
                level: HeadingLevel::H2,
                ..
            }) => {
                // If we were already collecting a section, store it
                if let Some(heading) = current_heading.take() {
                    sections.insert(heading, std::mem::take(&mut section_content));
                }
                in_h2_heading = true;
                heading_text.clear();
            }
            Event::End(TagEnd::Heading(HeadingLevel::H2)) => {
                in_h2_heading = false;
                current_heading = Some(heading_text.trim().to_string());
            }
            Event::Text(text) => {
                if in_h2_heading {
                    heading_text.push_str(&text);
                } else if current_heading.is_some() {
                    section_content.push_str(&text);
                }
            }
            Event::Code(code) => {
                if in_h2_heading {
                    heading_text.push_str(&code);
                } else if current_heading.is_some() {
                    section_content.push_str(&code);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if current_heading.is_some() && !in_h2_heading {
                    section_content.push('\n');
                }
            }
            Event::End(TagEnd::Paragraph) | Event::End(TagEnd::Item) => {
                if current_heading.is_some() && !in_h2_heading {
                    section_content.push('\n');
                }
            }
            _ => {}
        }
    }

    // Store the last section
    if let Some(heading) = current_heading.take() {
        sections.insert(heading, section_content);
    }

    sections
}

/// Extract metadata from the blockquote lines at the top of a document.
///
/// Looks for `> Protection level: ...` and `> Last updated: ...` lines
/// within the first blockquote of the document.
pub fn parse_metadata(md: &str) -> DocumentMetadata {
    let parser = Parser::new(md);

    let mut metadata = DocumentMetadata::default();
    let mut in_blockquote = false;
    let mut blockquote_done = false;

    for event in parser {
        if blockquote_done {
            break;
        }
        match event {
            Event::Start(Tag::BlockQuote(_)) => {
                in_blockquote = true;
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                in_blockquote = false;
                blockquote_done = true;
            }
            Event::Text(text) if in_blockquote => {
                let text = text.trim();
                if let Some(rest) = text.strip_prefix("Protection level:") {
                    metadata.protection_level = rest.trim().to_string();
                } else if let Some(rest) = text.strip_prefix("Last updated:") {
                    metadata.last_updated = Some(rest.trim().to_string());
                }
            }
            _ => {}
        }
    }

    metadata
}

/// Find the named `## ` section and append `content` at the end of that section.
///
/// Returns the full updated document. The content is inserted just before
/// the next `## ` heading or at EOF.
pub fn insert_into_section(md: &str, section: &str, content: &str) -> String {
    let section_header = format!("## {}", section);
    let lines: Vec<&str> = md.lines().collect();

    // Find the section header line
    let section_start = match lines
        .iter()
        .position(|line| *line == section_header.as_str())
    {
        Some(pos) => pos,
        None => return md.to_string(), // Section not found, return unchanged
    };

    // Find the end of the section: next `## ` heading or end of file
    let section_end = lines
        .iter()
        .enumerate()
        .skip(section_start + 1)
        .find(|(_, line)| line.starts_with("## "))
        .map(|(i, _)| i)
        .unwrap_or(lines.len());

    // Build the updated content
    let mut result = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i == section_end {
            // Insert new content before the next section header
            result.push_str(content);
            if !content.ends_with('\n') {
                result.push('\n');
            }
            result.push('\n');
        }
        result.push_str(line);
        result.push('\n');
    }

    // If section_end == lines.len(), append at the end
    if section_end == lines.len() {
        result.push_str(content);
        if !content.ends_with('\n') {
            result.push('\n');
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sections_from_tools_md() {
        let md = r#"# NevoFlux Tools

> Protection level: L3

## MCP Tool Inventory

Some tools here.

## Site Adaptation Graph

### github.com
- **Trust level**: trusted

### google.com
- **Trust level**: trusted

## Runtime Parameters

### Timing Parameters
- Default operation interval: 500ms
"#;

        let sections = parse_sections(md);
        assert!(sections.contains_key("MCP Tool Inventory"));
        assert!(sections.contains_key("Site Adaptation Graph"));
        assert!(sections.contains_key("Runtime Parameters"));
        assert!(sections["Site Adaptation Graph"].contains("github.com"));
    }

    #[test]
    fn parse_metadata_from_header() {
        let md = r#"# NevoFlux Soul

> Protection level: L0-L1 | Safety boundaries immutable
> Last updated: 2026-02-17T10:00:00Z

## Core Values
"#;
        let meta = parse_metadata(md);
        assert!(meta.protection_level.contains("L0-L1"));
        assert!(meta.last_updated.is_some());
    }

    #[test]
    fn insert_content_into_section() {
        let md = "# Doc\n\n## Section A\n\nContent A\n\n## Section B\n\nContent B\n";
        let result = insert_into_section(md, "Section A", "\nNew content\n");
        assert!(result.contains("Content A"));
        assert!(result.contains("New content"));
        assert!(result.contains("Section B"));
    }
}
