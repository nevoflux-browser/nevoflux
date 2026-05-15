//! MontyLinter - Regex-based detection of unsupported Python constructs.
//!
//! This is Layer 3 of the four-layer constraint pipeline. It scans
//! Python code line-by-line to detect constructs that Monty cannot
//! execute, returning actionable suggestions for each violation.
//!
//! Detects: class, match/case, with, import, yield, global, nonlocal.

/// A single violation found by the linter, with a concrete suggestion
/// for how to rewrite the offending construct.
#[derive(Debug, Clone)]
pub struct Violation {
    /// 1-based line number where the violation was found.
    pub line: usize,
    /// Name of the unsupported construct (e.g. "class", "import").
    pub construct: String,
    /// Concrete alternative suggestion for the user/LLM.
    pub suggestion: String,
}

/// Scans Python source code for constructs unsupported by the Monty
/// interpreter. Uses simple string matching on trimmed lines — no
/// external parser dependencies required.
pub struct MontyLinter;

impl MontyLinter {
    /// Check the given Python source for unsupported constructs.
    ///
    /// Returns a list of [`Violation`]s, one per detected issue. An empty
    /// vec means the code is clean (as far as the linter can tell).
    pub fn check(code: &str) -> Vec<Violation> {
        let mut violations = Vec::new();

        for (idx, line) in code.lines().enumerate() {
            let line_num = idx + 1;
            let trimmed = line.trim();

            // Skip blank lines and comments
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            // class definitions: `class Foo:` (must contain `:` to reduce false positives)
            if trimmed.starts_with("class ") && trimmed.contains(':') {
                violations.push(Violation {
                    line: line_num,
                    construct: "class".to_string(),
                    suggestion:
                        "Use dict + factory function: `def make_item(x): return {\"x\": x}`"
                            .to_string(),
                });
            }

            // match statements: `match <expr>:`
            if trimmed.starts_with("match ") && trimmed.contains(':') {
                violations.push(Violation {
                    line: line_num,
                    construct: "match".to_string(),
                    suggestion: "Use if/elif/else chain instead".to_string(),
                });
            }

            // with statements: `with <expr> as <name>:` or `with <expr>:`
            if trimmed.starts_with("with ") {
                violations.push(Violation {
                    line: line_num,
                    construct: "with".to_string(),
                    suggestion: "Use try/finally, or call tool function directly".to_string(),
                });
            }

            // yield / yield from
            // Note: may false-positive inside string literals; acceptable for this layer.
            if trimmed == "yield"
                || trimmed.starts_with("yield ")
                || trimmed.contains(" yield ")
                || trimmed.ends_with(" yield")
            {
                violations.push(Violation {
                    line: line_num,
                    construct: "yield".to_string(),
                    suggestion: "Use list.append() to collect results".to_string(),
                });
            }

            // global statements
            if trimmed.starts_with("global ") {
                violations.push(Violation {
                    line: line_num,
                    construct: "global".to_string(),
                    suggestion: "Use function parameters and return values instead".to_string(),
                });
            }

            // nonlocal statements
            if trimmed.starts_with("nonlocal ") {
                violations.push(Violation {
                    line: line_num,
                    construct: "nonlocal".to_string(),
                    suggestion: "Use function parameters and return values instead".to_string(),
                });
            }

            // import statements (Layer 2 strips these, but catch any that slip through)
            if trimmed.starts_with("import ") {
                violations.push(Violation {
                    line: line_num,
                    construct: "import".to_string(),
                    suggestion: "All tools are pre-injected; no imports needed".to_string(),
                });
            }
            if trimmed.starts_with("from ") && trimmed.contains(" import ") {
                violations.push(Violation {
                    line: line_num,
                    construct: "import".to_string(),
                    suggestion: "All tools are pre-injected; no imports needed".to_string(),
                });
            }
        }

        violations
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detects_class() {
        let code = "class Item:\n    def __init__(self):\n        pass";
        let violations = MontyLinter::check(code);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].construct.contains("class"));
        assert!(violations[0].suggestion.contains("dict"));
    }

    #[test]
    fn test_detects_match() {
        let code = "match x:\n    case 1:\n        pass";
        let violations = MontyLinter::check(code);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].suggestion.contains("if/elif"));
    }

    #[test]
    fn test_allows_async_await() {
        // async/await are now supported for orchestrate parallel execution
        let code = "async def fetch():\n    await get()";
        let violations = MontyLinter::check(code);
        assert!(violations.is_empty());
    }

    #[test]
    fn test_passes_valid_code() {
        let code = "def greet(name):\n    return f\"Hello, {name}!\"\n\nfor i in range(10):\n    print(greet(str(i)))";
        let violations = MontyLinter::check(code);
        assert!(violations.is_empty());
    }

    #[test]
    fn test_detects_with_statement() {
        let code = "with open(f) as h:\n    data = h.read()";
        let violations = MontyLinter::check(code);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].construct.contains("with"));
    }

    #[test]
    fn test_detects_yield() {
        let code = "def gen():\n    yield 1\n    yield from [2, 3]";
        let violations = MontyLinter::check(code);
        assert!(violations.len() >= 1);
    }

    #[test]
    fn test_detects_global_nonlocal() {
        let code = "def foo():\n    global x\n    nonlocal y";
        let violations = MontyLinter::check(code);
        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn test_detects_import() {
        let code = "import os\nfrom sys import argv";
        let violations = MontyLinter::check(code);
        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn test_indented_class() {
        let code = "def foo():\n    class Inner:\n        pass";
        let violations = MontyLinter::check(code);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].line, 2);
    }

    #[test]
    fn test_empty_input() {
        let violations = MontyLinter::check("");
        assert!(violations.is_empty());
    }

    #[test]
    fn test_comments_ignored() {
        let code = "# class Foo:\n# import os\ndef f():\n    pass";
        let violations = MontyLinter::check(code);
        assert!(violations.is_empty());
    }

    #[test]
    fn test_from_variable_not_import() {
        // `from_email = ...` has no ` import ` so should not trigger
        let code = "from_email = \"test@example.com\"";
        let violations = MontyLinter::check(code);
        assert!(violations.is_empty());
    }

    #[test]
    fn test_multiple_violations() {
        let code = "import os\nclass Foo:\n    global x\n    async def bar():\n        await baz()\n        yield 1";
        let violations = MontyLinter::check(code);
        // import, class, global, yield = 4 (async/await are now allowed)
        assert_eq!(violations.len(), 4);
    }

    #[test]
    fn test_line_numbers_correct() {
        let code = "x = 1\ny = 2\nimport os\nz = 3";
        let violations = MontyLinter::check(code);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].line, 3);
    }
}
