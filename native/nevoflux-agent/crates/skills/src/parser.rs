//! Skill file parser.
//!
//! Parses markdown skill files with YAML frontmatter.
//!
//! # Format
//!
//! ```markdown
//! ---
//! name: skill-name
//! description: Brief description
//! tags:
//!   - tag1
//!   - tag2
//! ---
//!
//! # Skill Content
//!
//! The rest of the file is the skill content.
//! ```

use crate::error::{Result, SkillsError};
use crate::types::{Skill, SkillMetadata, SkillSource};
use std::path::Path;

/// Frontmatter delimiter.
const FRONTMATTER_DELIMITER: &str = "---";

/// Parse a skill from a string.
pub fn parse_skill(content: &str, source: SkillSource) -> Result<Skill> {
    let (metadata, body) = parse_frontmatter(content)?;

    Ok(Skill {
        metadata,
        content: body,
        source,
        file_path: None,
    })
}

/// Parse a skill from a file path.
pub fn parse_skill_file(path: &Path, source: SkillSource) -> Result<Skill> {
    let content = std::fs::read_to_string(path)?;
    let mut skill = parse_skill(&content, source)?;
    skill.file_path = Some(path.to_path_buf());

    // If name is empty, derive from filename
    if skill.metadata.name.is_empty() {
        if let Some(stem) = path.file_stem() {
            skill.metadata.name = stem.to_string_lossy().to_string();
        }
    }

    Ok(skill)
}

/// Parse YAML frontmatter from content.
///
/// Returns the parsed metadata and the remaining content.
fn parse_frontmatter(content: &str) -> Result<(SkillMetadata, String)> {
    let content = content.trim();

    // Check for frontmatter start
    if !content.starts_with(FRONTMATTER_DELIMITER) {
        // No frontmatter, treat entire content as body
        // Try to extract name from first heading
        let metadata = extract_metadata_from_content(content);
        return Ok((metadata, content.to_string()));
    }

    // Find the end of frontmatter
    let after_start = &content[FRONTMATTER_DELIMITER.len()..];
    let end_pos = after_start
        .find(&format!("\n{}", FRONTMATTER_DELIMITER))
        .ok_or_else(|| SkillsError::InvalidFrontmatter("Missing closing delimiter".into()))?;

    // Extract frontmatter YAML
    let yaml_content = &after_start[..end_pos].trim();

    // Parse YAML
    let metadata: SkillMetadata = serde_yaml::from_str(yaml_content)
        .map_err(|e| SkillsError::InvalidFrontmatter(format!("YAML parse error: {}", e)))?;

    // Extract body (after second delimiter)
    let body_start = FRONTMATTER_DELIMITER.len() + end_pos + 1 + FRONTMATTER_DELIMITER.len();
    let body = if body_start < content.len() {
        content[body_start..].trim().to_string()
    } else {
        String::new()
    };

    Ok((metadata, body))
}

/// Extract metadata from content without frontmatter.
fn extract_metadata_from_content(content: &str) -> SkillMetadata {
    let mut metadata = SkillMetadata::default();

    // Try to extract name from first heading
    for line in content.lines() {
        let line = line.trim();
        if let Some(title) = line.strip_prefix("# ") {
            metadata.name = title.trim().to_string();
            break;
        }
    }

    // Try to extract description from first paragraph
    let mut in_description = false;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("# ") {
            in_description = true;
            continue;
        }
        if in_description && !line.is_empty() && !line.starts_with('#') {
            metadata.description = line.to_string();
            break;
        }
    }

    metadata
}

/// Serialize a skill to markdown with frontmatter.
pub fn serialize_skill(skill: &Skill) -> Result<String> {
    let yaml = serde_yaml::to_string(&skill.metadata)?;

    Ok(format!("---\n{}---\n\n{}", yaml, skill.content))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_skill_with_frontmatter() {
        let content = r#"---
name: test-skill
description: A test skill
tags:
  - testing
---

# Test Skill

This is the content of the test skill.
"#;

        let skill = parse_skill(content, SkillSource::User).unwrap();
        assert_eq!(skill.metadata.name, "test-skill");
        assert_eq!(skill.metadata.description, "A test skill");
        assert_eq!(skill.metadata.tags, vec!["testing"]);
        assert!(skill.content.contains("This is the content"));
    }

    #[test]
    fn test_parse_skill_minimal_frontmatter() {
        let content = r#"---
name: minimal
---

Content here.
"#;

        let skill = parse_skill(content, SkillSource::User).unwrap();
        assert_eq!(skill.metadata.name, "minimal");
        assert!(skill.metadata.description.is_empty());
        assert!(skill.metadata.enabled); // default
        assert_eq!(skill.content, "Content here.");
    }

    #[test]
    fn test_parse_skill_no_frontmatter() {
        let content = r#"# My Skill

This is a skill without frontmatter.

It should still be parseable.
"#;

        let skill = parse_skill(content, SkillSource::User).unwrap();
        assert_eq!(skill.metadata.name, "My Skill");
        assert!(skill.content.contains("without frontmatter"));
    }

    #[test]
    fn test_parse_skill_no_frontmatter_extracts_description() {
        let content = r#"# Code Review

Review code for best practices and potential issues.

## Guidelines

Follow these guidelines...
"#;

        let skill = parse_skill(content, SkillSource::User).unwrap();
        assert_eq!(skill.metadata.name, "Code Review");
        assert!(skill.metadata.description.contains("Review code"));
    }

    #[test]
    fn test_parse_skill_empty_body() {
        let content = r#"---
name: empty-body
description: No body content
---
"#;

        let skill = parse_skill(content, SkillSource::User).unwrap();
        assert_eq!(skill.metadata.name, "empty-body");
        assert!(skill.content.is_empty());
    }

    #[test]
    fn test_parse_skill_unclosed_frontmatter() {
        let content = r#"---
name: broken
description: Missing end delimiter

# Content
"#;

        let result = parse_skill(content, SkillSource::User);
        assert!(matches!(result, Err(SkillsError::InvalidFrontmatter(_))));
    }

    #[test]
    fn test_parse_skill_invalid_yaml() {
        let content = r#"---
name: [invalid yaml
---

Content
"#;

        let result = parse_skill(content, SkillSource::User);
        assert!(matches!(result, Err(SkillsError::InvalidFrontmatter(_))));
    }

    #[test]
    fn test_parse_skill_full_metadata() {
        let content = r#"---
name: full-skill
description: A fully specified skill
version: "1.2.3"
author: Test Author
tags:
  - code
  - review
enabled: true
dependencies:
  - base-skill
triggers:
  - when reviewing code
---

# Full Skill

Content with all metadata.
"#;

        let skill = parse_skill(content, SkillSource::User).unwrap();
        assert_eq!(skill.metadata.name, "full-skill");
        assert_eq!(skill.metadata.version, Some("1.2.3".into()));
        assert_eq!(skill.metadata.author, Some("Test Author".into()));
        assert_eq!(skill.metadata.tags, vec!["code", "review"]);
        assert!(skill.metadata.enabled);
        assert_eq!(skill.metadata.dependencies, vec!["base-skill"]);
        assert_eq!(skill.metadata.triggers, vec!["when reviewing code"]);
    }

    #[test]
    fn test_parse_skill_disabled() {
        let content = r#"---
name: disabled-skill
enabled: false
---

Disabled content.
"#;

        let skill = parse_skill(content, SkillSource::User).unwrap();
        assert!(!skill.metadata.enabled);
    }

    #[test]
    fn test_parse_skill_source() {
        let content = "---\nname: test\n---\nContent";

        let user_skill = parse_skill(content, SkillSource::User).unwrap();
        assert_eq!(user_skill.source, SkillSource::User);

        let builtin_skill = parse_skill(content, SkillSource::Builtin).unwrap();
        assert_eq!(builtin_skill.source, SkillSource::Builtin);
    }

    #[test]
    fn test_parse_skill_file() {
        use std::io::Write;
        let temp_dir = tempfile::TempDir::new().unwrap();
        let skill_path = temp_dir.path().join("my-skill.md");

        let mut file = std::fs::File::create(&skill_path).unwrap();
        write!(
            file,
            r#"---
name: file-skill
description: Loaded from file
---

# File Skill

Content from file.
"#
        )
        .unwrap();

        let skill = parse_skill_file(&skill_path, SkillSource::User).unwrap();
        assert_eq!(skill.metadata.name, "file-skill");
        assert_eq!(skill.file_path, Some(skill_path));
    }

    #[test]
    fn test_parse_skill_file_name_from_filename() {
        use std::io::Write;
        let temp_dir = tempfile::TempDir::new().unwrap();
        let skill_path = temp_dir.path().join("code-review.md");

        let mut file = std::fs::File::create(&skill_path).unwrap();
        write!(
            file,
            r#"---
name: ""
description: Review code
---

Content.
"#
        )
        .unwrap();

        let skill = parse_skill_file(&skill_path, SkillSource::User).unwrap();
        // Name derived from filename since frontmatter has empty name
        assert_eq!(skill.metadata.name, "code-review");
    }

    #[test]
    fn test_parse_skill_file_not_found() {
        let result = parse_skill_file(Path::new("/nonexistent/skill.md"), SkillSource::User);
        assert!(matches!(result, Err(SkillsError::Io(_))));
    }

    #[test]
    fn test_serialize_skill() {
        let metadata = SkillMetadata::new("serialize-test")
            .with_description("Test serialization")
            .with_tag("test");
        let skill = Skill::new(metadata, "# Content\n\nBody text.");

        let output = serialize_skill(&skill).unwrap();

        assert!(output.starts_with("---\n"));
        assert!(output.contains("name: serialize-test"));
        assert!(output.contains("description: Test serialization"));
        assert!(output.contains("---\n\n# Content"));
    }

    #[test]
    fn test_parse_serialize_roundtrip() {
        let original = r#"---
name: roundtrip
description: Test roundtrip
tags:
  - test
---

# Roundtrip Test

Content preserved.
"#;

        let skill = parse_skill(original, SkillSource::User).unwrap();
        let serialized = serialize_skill(&skill).unwrap();
        let reparsed = parse_skill(&serialized, SkillSource::User).unwrap();

        assert_eq!(skill.metadata.name, reparsed.metadata.name);
        assert_eq!(skill.metadata.description, reparsed.metadata.description);
        // Content should be equivalent (might have whitespace differences)
        assert!(reparsed.content.contains("Content preserved"));
    }

    #[test]
    fn test_parse_skill_with_code_blocks() {
        let content = r#"---
name: code-skill
---

# Code Skill

Here's some code:

```rust
fn main() {
    println!("Hello");
}
```

And more text.
"#;

        let skill = parse_skill(content, SkillSource::User).unwrap();
        assert!(skill.content.contains("```rust"));
        assert!(skill.content.contains("fn main()"));
    }

    #[test]
    fn test_parse_skill_whitespace_handling() {
        let content = r#"

---
name: whitespace-test
---

   Content with leading spaces.

"#;

        let skill = parse_skill(content, SkillSource::User).unwrap();
        assert_eq!(skill.metadata.name, "whitespace-test");
        // Content should be trimmed
        assert!(skill.content.starts_with("Content"));
    }

    #[test]
    fn test_parse_skill_with_allowed_tools() {
        let content = r#"---
name: stitch-skill
description: A skill that requires Stitch tools
allowed-tools:
  - stitch*:*
  - Read
---

# Stitch Skill

This skill uses Stitch MCP tools.
"#;

        let skill = parse_skill(content, SkillSource::User).unwrap();
        assert_eq!(skill.metadata.name, "stitch-skill");
        assert_eq!(skill.metadata.allowed_tools, vec!["stitch*:*", "Read"]);
    }

    #[test]
    fn test_parse_skill_with_allowed_tools_snake_case() {
        // Test with snake_case variant (allowed_tools)
        let content = r#"---
name: notion-skill
allowed_tools:
  - notion:*
---

Content.
"#;

        let skill = parse_skill(content, SkillSource::User).unwrap();
        assert_eq!(skill.metadata.allowed_tools, vec!["notion:*"]);
    }

    #[test]
    fn test_parse_skill_without_allowed_tools() {
        // Skills without allowed-tools should have empty vec
        let content = r#"---
name: basic-skill
---

Content.
"#;

        let skill = parse_skill(content, SkillSource::User).unwrap();
        assert!(skill.metadata.allowed_tools.is_empty());
    }
}
