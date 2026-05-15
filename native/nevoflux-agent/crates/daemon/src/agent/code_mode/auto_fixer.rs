//! MontyAutoFixer - Mechanical code transforms for Monty compatibility.
//! Strips imports, decorators, typing annotations, and type:ignore comments.
//!
//! This is Layer 2 of the four-layer constraint pipeline. It applies
//! deterministic text transforms to fix common violations automatically,
//! with zero LLM cost and sub-millisecond execution.

/// Applies mechanical text transforms to Python code before it reaches
/// the Monty interpreter. All transforms are deterministic and preserve
/// the semantic meaning of code that Monty supports.
pub struct MontyAutoFixer;

impl MontyAutoFixer {
    /// Apply all mechanical transforms to the given Python code.
    ///
    /// Transforms applied (Phase 1-3):
    /// 1. Strip markdown artifacts (backticks, language tags, indented code fences)
    /// 2. Rewrite unsupported patterns:
    ///    - `sorted(key=...)` → `_keysort()` helper
    ///    - `map()`/`filter()` → `_map()`/`_filter()` helpers
    ///    - `math.*` → inline expressions or `_math_*` helpers
    ///    - `os.path.*` → `_path_*` helpers
    ///    - `json.dumps()`/`json.loads()` → `_json_dumps()`/`_json_loads()` helpers
    ///    - `functools.reduce()` → `_reduce()` helper
    ///    - `collections.Counter()` → `_counter()` helper
    ///    - `re.*` → `_re_*` helpers (via `run_command` + python3)
    ///    - `datetime.*` → `_datetime_*` helpers (via `run_command` + python3)
    ///    - `random.*` → `_random_*` helpers (via `run_command` + python3)
    /// 3. Per-line: strip imports, decorators, type annotations, type:ignore comments
    /// 4. Collapse excessive leading blank lines from stripped content
    pub fn fix(code: &str) -> String {
        // Phase 1: Strip markdown artifacts that LLMs sometimes include
        let code = Self::strip_markdown_artifacts(code);

        // Phase 1b: Strip async/await — Monty external functions return
        // synchronously, so `await func(...)` would cause
        // `TypeError: 'str' object can't be awaited` at runtime.
        let code = Self::strip_await(&code);

        // Phase 2: Rewrite unsupported builtins patterns
        let code = Self::rewrite_sorted_with_key(&code);
        let code = Self::rewrite_map_filter(&code);
        let code = Self::rewrite_math(&code);
        let code = Self::rewrite_os_path(&code);
        let code = Self::rewrite_json(&code);
        let code = Self::rewrite_reduce(&code);
        let code = Self::rewrite_counter(&code);
        // Phase 2b: Bash-bridged stdlib rewrites (requires run_command tool)
        let code = Self::rewrite_re(&code);
        let code = Self::rewrite_datetime(&code);
        let code = Self::rewrite_random(&code);
        let code = Self::rewrite_time(&code);

        // Phase 3: Per-line transforms
        let mut result_lines: Vec<String> = Vec::new();

        for line in code.lines() {
            let trimmed = line.trim();

            // Strip import statements: `import X` and `from X import Y`
            if trimmed.starts_with("import ") {
                continue;
            }
            if trimmed.starts_with("from ") && trimmed.contains(" import ") {
                continue;
            }

            // Strip decorator lines: lines starting with `@`
            if trimmed.starts_with('@') {
                continue;
            }

            // Strip `# type: ignore` suffixes (with optional bracket annotations)
            if let Some(pos) = line.find("# type: ignore") {
                let before = line[..pos].trim_end();
                if before.is_empty() {
                    // The entire line is just a type: ignore comment, skip it
                    continue;
                }
                result_lines.push(before.to_string());
                continue;
            }

            // Strip type annotations from variable assignments: `x: int = 1` → `x = 1`
            let line = Self::strip_variable_annotation(line);

            result_lines.push(line);
        }

        // Remove leading blank lines that result from stripping
        let start = result_lines
            .iter()
            .position(|l| !l.trim().is_empty())
            .unwrap_or(result_lines.len());
        let trimmed_lines = &result_lines[start..];

        trimmed_lines.join("\n")
    }

    /// Strip markdown artifacts that LLMs commonly embed in generated code.
    ///
    /// Handles:
    /// - Leading/trailing backtick fences (```python, ```, ````python-exec`, etc.)
    /// - Lines that are just backticks
    /// - Inline backtick wrapping on individual lines
    fn strip_markdown_artifacts(code: &str) -> String {
        let mut lines: Vec<&str> = code.lines().collect();

        // Remove leading fence line if present (```python, ```python-exec, ```, etc.)
        if let Some(first) = lines.first() {
            let t = first.trim();
            if t.starts_with("```") {
                lines.remove(0);
            }
        }

        // Remove trailing fence line if present
        if let Some(last) = lines.last() {
            let t = last.trim();
            if t.starts_with("```") && !t.contains('=') && !t.contains('(') {
                lines.pop();
            }
        }

        // Remove any remaining lines that are only backticks (e.g., inner fences)
        lines
            .into_iter()
            .filter(|line| {
                let t = line.trim();
                // Keep the line unless it's ONLY backticks (3+)
                !(t.len() >= 3 && t.chars().all(|c| c == '`'))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Strip `await` / `async def` / `async for` / `async with` keywords.
    ///
    /// Monty allows async/await syntax but external tool functions return
    /// synchronously.  `await tool(...)` causes `TypeError: 'str' object
    /// can't be awaited` at runtime.  Stripping before execution is the
    /// cheapest fix (no LLM rewrite needed).
    fn strip_await(code: &str) -> String {
        if !code.contains("await ") && !code.contains("async ") {
            return code.to_string();
        }
        code.replace("await ", "")
            .replace("async def ", "def ")
            .replace("async for ", "for ")
            .replace("async with ", "with ")
    }

    /// Strip type annotations from simple variable assignments.
    ///
    /// `x: int = 1` → `x = 1`
    /// `result: list[str] = []` → `result = []`
    ///
    /// Does NOT touch function parameters (handled differently by Monty)
    /// or lines without `=` (bare annotations like `x: int`).
    fn strip_variable_annotation(line: &str) -> String {
        // Only process lines with both `:` and `=`
        let trimmed = line.trim();
        if !trimmed.contains(':') || !trimmed.contains('=') {
            return line.to_string();
        }

        // Skip function definitions
        if trimmed.starts_with("def ") || trimmed.starts_with("async def ") {
            return line.to_string();
        }

        // Skip dict literals and slices: `{"key": val}`, `x = a[1:2]`
        // Only match annotations at the top level (before any `=`)
        let eq_pos = match trimmed.find('=') {
            Some(p)
                if p > 0
                    && trimmed.as_bytes().get(p - 1) != Some(&b'!')
                    && trimmed.as_bytes().get(p + 1) != Some(&b'=') =>
            {
                p
            }
            _ => return line.to_string(),
        };

        let before_eq = &trimmed[..eq_pos];
        let after_eq = &trimmed[eq_pos..]; // includes `= value`

        // Check if `before_eq` has a `:` that looks like a type annotation
        // Pattern: `name: type` where name is an identifier
        if let Some(colon_pos) = before_eq.find(':') {
            let var_name = before_eq[..colon_pos].trim();
            // Validate it looks like a variable name (simple check)
            if !var_name.is_empty()
                && var_name.chars().all(|c| c.is_alphanumeric() || c == '_')
                && !var_name.starts_with(|c: char| c.is_ascii_digit())
            {
                // Preserve leading whitespace
                let indent = line.len() - line.trim_start().len();
                let spaces = &line[..indent];
                return format!("{}{} {}", spaces, var_name, after_eq.trim_start());
            }
        }

        line.to_string()
    }

    /// Rewrite `sorted(iterable, key=func)` → manual sort pattern.
    ///
    /// Monty doesn't support keyword arguments for `sorted()`.
    /// Also rewrites `sorted(iterable, reverse=True)`.
    ///
    /// Transforms:
    /// - `sorted(items, key=lambda x: x['name'])` → `_sorted_list = list(items); _sorted_list.sort(); _sorted_list`
    ///   (but with a decorated-sort pattern for key functions)
    /// - `sorted(items, key=func, reverse=True)` → same with .reverse()
    ///
    /// This is a best-effort transform for common patterns.
    fn rewrite_sorted_with_key(code: &str) -> String {
        // Strategy: find `sorted(` with `key=` and rewrite to a helper function
        // that uses manual comparison-based sorting.
        //
        // We inject a helper function at the top, then replace
        // `sorted(X, key=Y)` with `_keysort(X, Y)`
        // `sorted(X, key=Y, reverse=True)` with `_keysort(X, Y, True)`
        // `sorted(X, reverse=True)` with `_keysort(X, None, True)`

        // Check if any sorted() or .sort() call uses keyword arguments
        let has_sorted_key = code.contains("sorted(") && code.contains("key=");
        let has_sorted_reverse = code.contains("sorted(") && code.contains("reverse=");
        let has_sort_method =
            code.contains(".sort(") && (code.contains("key=") || code.contains("reverse="));

        if !has_sorted_key && !has_sorted_reverse && !has_sort_method {
            return code.to_string();
        }

        // Inject helper function at the top.
        // Uses insertion sort with slice concatenation to avoid subscript
        // assignment (result[i] = x), which Monty does not support.
        let helper = r#"def _keysort(items, key_fn=None, reverse=False):
    result = []
    for item in items:
        k = key_fn(item) if key_fn else item
        inserted = False
        for i in range(len(result)):
            rk = key_fn(result[i]) if key_fn else result[i]
            do_insert = False
            if reverse:
                if k > rk:
                    do_insert = True
            else:
                if k < rk:
                    do_insert = True
            if do_insert:
                result = result[:i] + [item] + result[i:]
                inserted = True
                break
        if not inserted:
            result.append(item)
    return result
"#;

        let mut result = code.to_string();

        // Replace patterns: sorted(X, key=Y, reverse=Z) and sorted(X, key=Y)
        // and sorted(X, reverse=Z)
        // This is a simplified regex-free approach that handles common patterns.
        result = Self::replace_sorted_calls(&result);

        // Replace .sort(key=...) and .sort(reverse=...) method calls
        // e.g. `data.sort(key=lambda x: x[0], reverse=True)` → `data = _keysort(data, lambda x: x[0], True)`
        result = Self::replace_sort_method_calls(&result);

        format!("{}\n{}", helper.trim(), result)
    }

    /// Replace sorted() calls with keyword args to use _keysort().
    fn replace_sorted_calls(code: &str) -> String {
        let mut result = String::new();
        let mut chars = code.chars().peekable();
        let mut i = 0;

        while i < code.len() {
            // Look for "sorted(" — use starts_with to avoid slicing at non-char-boundary
            if code[i..].starts_with("sorted(") {
                // Find the matching closing parenthesis
                if let Some((args_str, end_pos)) = Self::extract_balanced_parens(code, i + 6) {
                    // Check if args contain key= or reverse=
                    if args_str.contains("key=") || args_str.contains("reverse=") {
                        let rewritten = Self::rewrite_single_sorted(&args_str);
                        result.push_str(&rewritten);
                        i = end_pos + 1;
                        // Advance the chars iterator to match
                        chars = code[i..].chars().peekable();
                        continue;
                    }
                }
            }

            if let Some(c) = chars.next() {
                result.push(c);
                i += c.len_utf8();
            } else {
                break;
            }
        }

        result
    }

    /// Replace `.sort(key=..., reverse=...)` method calls with `_keysort()`.
    ///
    /// Rewrites `var.sort(key=fn)` → `var = _keysort(var, fn)`
    /// and `var.sort(key=fn, reverse=True)` → `var = _keysort(var, fn, True)`
    /// and `var.sort(reverse=True)` → `var = _keysort(var, None, True)`
    ///
    /// Works line-by-line since `.sort()` is always a statement (no return value).
    fn replace_sort_method_calls(code: &str) -> String {
        let mut result_lines: Vec<String> = Vec::new();

        for line in code.lines() {
            let trimmed = line.trim();
            // Find `.sort(` in the line
            if let Some(sort_dot_pos) = trimmed.find(".sort(") {
                let before_dot = trimmed[..sort_dot_pos].trim();
                // The variable name is the identifier before `.sort(`
                // Must be a simple identifier (no operators, no complex expressions)
                if !before_dot.is_empty() && Self::is_simple_identifier(before_dot) {
                    let paren_start_in_trimmed = sort_dot_pos + ".sort".len();
                    if let Some((args_str, end_pos)) =
                        Self::extract_balanced_parens(trimmed, paren_start_in_trimmed)
                    {
                        if args_str.contains("key=") || args_str.contains("reverse=") {
                            // Check nothing follows the closing paren (it's a statement)
                            let after = trimmed[end_pos + 1..].trim();
                            if after.is_empty() || after.starts_with('#') {
                                let rewritten =
                                    Self::rewrite_single_sorted_for_method(before_dot, &args_str);
                                // Preserve leading whitespace
                                let indent = &line[..line.len() - line.trim_start().len()];
                                result_lines.push(format!("{}{}", indent, rewritten));
                                continue;
                            }
                        }
                    }
                }
            }
            result_lines.push(line.to_string());
        }

        result_lines.join("\n")
    }

    /// Check if a string is a simple Python identifier (variable name, possibly with
    /// subscript or attribute access like `data`, `self.items`, `results[0]`).
    fn is_simple_identifier(s: &str) -> bool {
        if s.is_empty() {
            return false;
        }
        let bytes = s.as_bytes();
        // Must start with letter or underscore
        if !bytes[0].is_ascii_alphabetic() && bytes[0] != b'_' {
            return false;
        }
        // Allow alphanumeric, underscore, dot, brackets
        let mut bracket_depth = 0i32;
        for &b in bytes {
            match b {
                b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'.' | b'\'' | b'"' => {}
                b'[' => bracket_depth += 1,
                b']' => bracket_depth -= 1,
                _ if bracket_depth > 0 => {} // allow anything inside brackets
                _ => return false,
            }
        }
        bracket_depth == 0
    }

    /// Rewrite args for a .sort() method call to a _keysort() assignment.
    /// `var.sort(key=fn, reverse=True)` → `var = _keysort(var, fn, True)`
    fn rewrite_single_sorted_for_method(var_name: &str, args: &str) -> String {
        let parts = Self::split_top_level_commas(args);

        let mut key_fn = String::new();
        let mut reverse = String::new();

        for part in &parts {
            let trimmed = part.trim();
            if let Some(rest) = trimmed.strip_prefix("key=") {
                key_fn = rest.trim().to_string();
            } else if let Some(rest) = trimmed.strip_prefix("reverse=") {
                reverse = rest.trim().to_string();
            }
        }

        if !key_fn.is_empty() && !reverse.is_empty() {
            format!(
                "{} = _keysort({}, {}, {})",
                var_name, var_name, key_fn, reverse
            )
        } else if !key_fn.is_empty() {
            format!("{} = _keysort({}, {})", var_name, var_name, key_fn)
        } else if !reverse.is_empty() {
            format!("{} = _keysort({}, None, {})", var_name, var_name, reverse)
        } else {
            // No keyword args, keep original
            format!("{}.sort({})", var_name, args)
        }
    }

    /// Extract content inside balanced parentheses starting at `open_pos`.
    /// Returns (inner_content, close_paren_position).
    fn extract_balanced_parens(code: &str, open_pos: usize) -> Option<(String, usize)> {
        if code.as_bytes().get(open_pos) != Some(&b'(') {
            return None;
        }
        let mut depth = 1;
        let mut i = open_pos + 1;
        let bytes = code.as_bytes();

        while i < bytes.len() && depth > 0 {
            match bytes[i] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        let inner = &code[open_pos + 1..i];
                        return Some((inner.to_string(), i));
                    }
                }
                b'"' | b'\'' => {
                    // Skip string literals
                    let quote = bytes[i];
                    i += 1;
                    while i < bytes.len() && bytes[i] != quote {
                        if bytes[i] == b'\\' {
                            i += 1; // skip escaped char
                        }
                        i += 1;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        None
    }

    /// Rewrite args of a single sorted() call to _keysort() call.
    /// Input: the content inside sorted(...), e.g. "items, key=lambda x: x['name']"
    /// Output: "_keysort(items, lambda x: x['name'])" etc.
    fn rewrite_single_sorted(args: &str) -> String {
        // Split by top-level commas (not inside parens/brackets)
        let parts = Self::split_top_level_commas(args);

        let mut iterable = String::new();
        let mut key_fn = String::new();
        let mut reverse = String::new();

        for (idx, part) in parts.iter().enumerate() {
            let trimmed = part.trim();
            if let Some(rest) = trimmed.strip_prefix("key=") {
                key_fn = rest.trim().to_string();
            } else if let Some(rest) = trimmed.strip_prefix("reverse=") {
                reverse = rest.trim().to_string();
            } else if idx == 0 {
                iterable = trimmed.to_string();
            }
        }

        if !key_fn.is_empty() && !reverse.is_empty() {
            format!("_keysort({}, {}, {})", iterable, key_fn, reverse)
        } else if !key_fn.is_empty() {
            format!("_keysort({}, {})", iterable, key_fn)
        } else if !reverse.is_empty() {
            format!("_keysort({}, None, {})", iterable, reverse)
        } else {
            // No keyword args found, keep original
            format!("sorted({})", args)
        }
    }

    /// Split a string by commas, but only at the top level
    /// (not inside parentheses, brackets, or braces).
    fn split_top_level_commas(s: &str) -> Vec<String> {
        let mut parts = Vec::new();
        let mut current = String::new();
        let mut depth = 0;
        let mut in_string = false;
        let mut string_char = '"';

        for ch in s.chars() {
            if in_string {
                current.push(ch);
                if ch == string_char {
                    in_string = false;
                }
                continue;
            }
            match ch {
                '"' | '\'' => {
                    in_string = true;
                    string_char = ch;
                    current.push(ch);
                }
                '(' | '[' | '{' => {
                    depth += 1;
                    current.push(ch);
                }
                ')' | ']' | '}' => {
                    depth -= 1;
                    current.push(ch);
                }
                ',' if depth == 0 => {
                    parts.push(current.clone());
                    current.clear();
                }
                _ => {
                    current.push(ch);
                }
            }
        }
        if !current.is_empty() {
            parts.push(current);
        }
        parts
    }

    /// Rewrite `math.*` calls and constants to pure Python equivalents.
    ///
    /// Handles:
    /// - `math.pi` → `3.141592653589793`
    /// - `math.e` → `2.718281828459045`
    /// - `math.inf` → `float("inf")`
    /// - `math.sqrt(x)` → `(x) ** 0.5`
    /// - `math.pow(x, y)` → `(x) ** (y)` (simplified; doesn't handle 3-arg pow)
    /// - `math.fabs(x)` → `abs(x)`
    /// - `math.floor(x)` → `_math_floor(x)` (injected helper)
    /// - `math.ceil(x)` → `_math_ceil(x)` (injected helper)
    /// - `math.log(x)` / `math.log(x, base)` → `_math_log(x)` / `_math_log(x, base)` (injected helper using Newton's method)
    /// - `math.log2(x)` → `_math_log(x, 2)`
    /// - `math.log10(x)` → `_math_log(x, 10)`
    fn rewrite_math(code: &str) -> String {
        if !code.contains("math.") {
            return code.to_string();
        }

        let mut result = code.to_string();
        let mut need_floor = false;
        let mut need_ceil = false;
        let mut need_log = false;

        // Simple constant replacements
        result = result.replace("math.pi", "3.141592653589793");
        result = result.replace("math.e", "2.718281828459045");
        result = result.replace("math.inf", "float(\"inf\")");

        // math.fabs(x) → abs(x)
        result = result.replace("math.fabs(", "abs(");

        // math.sqrt(x) → (x) ** 0.5
        // We need to find math.sqrt(...) and extract the argument
        while result.contains("math.sqrt(") {
            let pos = match result.find("math.sqrt(") {
                Some(p) => p,
                None => break,
            };
            let paren_start = pos + "math.sqrt".len();
            if let Some((inner, end)) = Self::extract_balanced_parens(&result, paren_start) {
                let replacement = format!("({}) ** 0.5", inner.trim());
                result = format!("{}{}{}", &result[..pos], replacement, &result[end + 1..]);
            } else {
                break;
            }
        }

        // math.pow(x, y) → (x) ** (y)
        while result.contains("math.pow(") {
            let pos = match result.find("math.pow(") {
                Some(p) => p,
                None => break,
            };
            let paren_start = pos + "math.pow".len();
            if let Some((inner, end)) = Self::extract_balanced_parens(&result, paren_start) {
                let parts = Self::split_top_level_commas(&inner);
                if parts.len() == 2 {
                    let replacement = format!("({}) ** ({})", parts[0].trim(), parts[1].trim());
                    result = format!("{}{}{}", &result[..pos], replacement, &result[end + 1..]);
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // math.floor(x) → _math_floor(x)
        if result.contains("math.floor(") {
            need_floor = true;
            result = result.replace("math.floor(", "_math_floor(");
        }

        // math.ceil(x) → _math_ceil(x)
        if result.contains("math.ceil(") {
            need_ceil = true;
            result = result.replace("math.ceil(", "_math_ceil(");
        }

        // math.log2(x) → _math_log(x, 2)
        while result.contains("math.log2(") {
            let pos = match result.find("math.log2(") {
                Some(p) => p,
                None => break,
            };
            let paren_start = pos + "math.log2".len();
            if let Some((inner, end)) = Self::extract_balanced_parens(&result, paren_start) {
                need_log = true;
                let replacement = format!("_math_log({}, 2)", inner.trim());
                result = format!("{}{}{}", &result[..pos], replacement, &result[end + 1..]);
            } else {
                break;
            }
        }

        // math.log10(x) → _math_log(x, 10)
        while result.contains("math.log10(") {
            let pos = match result.find("math.log10(") {
                Some(p) => p,
                None => break,
            };
            let paren_start = pos + "math.log10".len();
            if let Some((inner, end)) = Self::extract_balanced_parens(&result, paren_start) {
                need_log = true;
                let replacement = format!("_math_log({}, 10)", inner.trim());
                result = format!("{}{}{}", &result[..pos], replacement, &result[end + 1..]);
            } else {
                break;
            }
        }

        // math.log(x) or math.log(x, base) → _math_log(x) or _math_log(x, base)
        if result.contains("math.log(") {
            need_log = true;
            result = result.replace("math.log(", "_math_log(");
        }

        // Inject helpers
        let mut helpers = String::new();
        if need_floor {
            helpers.push_str("def _math_floor(x):\n    n = int(x)\n    if x < 0 and x != n:\n        return n - 1\n    return n\n");
        }
        if need_ceil {
            helpers.push_str("def _math_ceil(x):\n    n = int(x)\n    if x > n:\n        return n + 1\n    return n\n");
        }
        if need_log {
            // Natural log via Newton's method (ln), then log(x, base) = ln(x) / ln(base)
            helpers.push_str(concat!(
                "def _math_log(x, base=None):\n",
                "    if x <= 0:\n",
                "        return float(\"inf\")\n",
                "    ln = 0.0\n",
                "    if x < 1:\n",
                "        x = 1.0 / x\n",
                "        ln = -1.0\n",
                "    else:\n",
                "        ln = 0.0\n",
                "    y = (x - 1) / (x + 1)\n",
                "    y2 = y * y\n",
                "    term = y\n",
                "    s = term\n",
                "    for i in range(1, 100):\n",
                "        term = term * y2\n",
                "        s = s + term / (2 * i + 1)\n",
                "    result = 2.0 * s\n",
                "    if ln < 0:\n",
                "        result = -result\n",
                "    if base is not None:\n",
                "        lb = _math_log(base)\n",
                "        if lb != 0:\n",
                "            return result / lb\n",
                "    return result\n",
            ));
        }

        if helpers.is_empty() {
            result
        } else {
            format!("{}\n{}", helpers.trim(), result)
        }
    }

    /// Rewrite `os.path.join(...)` to a pure Python helper.
    ///
    /// Handles `os.path.join(a, b)`, `os.path.join(a, b, c)` etc.
    /// Also handles `os.path.basename(p)`, `os.path.dirname(p)`,
    /// `os.path.splitext(p)`, `os.path.exists(p)` (always returns True as best-effort).
    fn rewrite_os_path(code: &str) -> String {
        if !code.contains("os.path.") && !code.contains("os.sep") {
            return code.to_string();
        }

        let mut result = code.to_string();
        let mut need_join = false;
        let mut need_basename = false;
        let mut need_dirname = false;
        let mut need_splitext = false;

        // os.path.join(a, b, ...) → _path_join([a, b, ...])
        // We need to extract args and wrap them in a list
        while result.contains("os.path.join(") {
            let pos = match result.find("os.path.join(") {
                Some(p) => p,
                None => break,
            };
            let paren_start = pos + "os.path.join".len();
            if let Some((inner, end)) = Self::extract_balanced_parens(&result, paren_start) {
                need_join = true;
                let replacement = format!("_path_join([{}])", inner.trim());
                result = format!("{}{}{}", &result[..pos], replacement, &result[end + 1..]);
            } else {
                break;
            }
        }
        if result.contains("os.path.basename(") {
            need_basename = true;
            result = result.replace("os.path.basename(", "_path_basename(");
        }
        if result.contains("os.path.dirname(") {
            need_dirname = true;
            result = result.replace("os.path.dirname(", "_path_dirname(");
        }
        if result.contains("os.path.splitext(") {
            need_splitext = true;
            result = result.replace("os.path.splitext(", "_path_splitext(");
        }
        // os.path.exists → always True (best-effort, no filesystem access)
        if result.contains("os.path.exists(") {
            result = result.replace("os.path.exists(", "_path_exists(");
        }
        // os.path.sep → "/"
        result = result.replace("os.path.sep", "\"/\"");
        // os.sep → "/"
        result = result.replace("os.sep", "\"/\"");

        let mut helpers = String::new();
        if need_join {
            helpers.push_str(concat!(
                "def _path_join(parts):\n",
                "    result = \"\"\n",
                "    for p in parts:\n",
                "        if not result or p[0:1] == \"/\":\n",
                "            result = p\n",
                "        else:\n",
                "            if result[-1:] == \"/\":\n",
                "                result = result + p\n",
                "            else:\n",
                "                result = result + \"/\" + p\n",
                "    return result\n",
            ));
        }
        if need_basename {
            helpers.push_str(concat!(
                "def _path_basename(p):\n",
                "    idx = p.rfind(\"/\")\n",
                "    if idx < 0:\n",
                "        return p\n",
                "    return p[idx + 1:]\n",
            ));
        }
        if need_dirname {
            helpers.push_str(concat!(
                "def _path_dirname(p):\n",
                "    idx = p.rfind(\"/\")\n",
                "    if idx < 0:\n",
                "        return \"\"\n",
                "    return p[:idx]\n",
            ));
        }
        if need_splitext {
            helpers.push_str(concat!(
                "def _path_splitext(p):\n",
                "    idx = p.rfind(\".\")\n",
                "    slash = p.rfind(\"/\")\n",
                "    if idx < 0 or idx < slash:\n",
                "        return [p, \"\"]\n",
                "    return [p[:idx], p[idx:]]\n",
            ));
        }
        // _path_exists is a no-op stub
        if result.contains("_path_exists(") && !helpers.contains("_path_exists") {
            helpers.push_str("def _path_exists(p):\n    return True\n");
        }

        if helpers.is_empty() {
            result
        } else {
            format!("{}\n{}", helpers.trim(), result)
        }
    }

    /// Rewrite `json.dumps(x)` and `json.loads(s)` to pure Python helpers.
    ///
    /// `_json_dumps` recursively serializes dict/list/str/int/float/bool/None.
    /// `_json_loads` is a recursive descent parser for JSON strings.
    fn rewrite_json(code: &str) -> String {
        let has_dumps = code.contains("json.dumps(");
        let has_loads = code.contains("json.loads(");
        // Also handle standalone calls after `from json import dumps, loads`
        let has_json_import = code.contains("import json") || code.contains("from json import");
        let has_standalone_dumps = has_json_import && Self::has_standalone_call(code, "dumps");
        let has_standalone_loads = has_json_import && Self::has_standalone_call(code, "loads");

        let need_dumps = has_dumps || has_standalone_dumps;
        let need_loads = has_loads || has_standalone_loads;

        if !need_dumps && !need_loads {
            return code.to_string();
        }

        let mut helpers = String::new();
        let mut result = code.to_string();

        if need_dumps {
            helpers.push_str(concat!(
                "def _json_dumps(obj, indent=None):\n",
                "    if obj is None:\n",
                "        return \"null\"\n",
                "    if obj is True:\n",
                "        return \"true\"\n",
                "    if obj is False:\n",
                "        return \"false\"\n",
                "    if type(obj) == int or type(obj) == float:\n",
                "        return str(obj)\n",
                "    if type(obj) == str:\n",
                "        s = obj.replace(\"\\\\\", \"\\\\\\\\\").replace('\"', '\\\\\"')\n",
                "        s = s.replace(\"\\n\", \"\\\\n\").replace(\"\\t\", \"\\\\t\")\n",
                "        return '\"' + s + '\"'\n",
                "    if type(obj) == list:\n",
                "        parts = []\n",
                "        for item in obj:\n",
                "            parts.append(_json_dumps(item))\n",
                "        return \"[\" + \", \".join(parts) + \"]\"\n",
                "    if type(obj) == dict:\n",
                "        parts = []\n",
                "        for k in obj:\n",
                "            parts.append(_json_dumps(str(k)) + \": \" + _json_dumps(obj[k]))\n",
                "        return \"{\" + \", \".join(parts) + \"}\"\n",
                "    return str(obj)\n",
            ));
            result = result.replace("json.dumps(", "_json_dumps(");
            if has_standalone_dumps {
                result = Self::replace_standalone_call(&result, "dumps", "_json_dumps");
            }
        }

        if need_loads {
            // Order matters: Monty requires functions to be defined before
            // they are referenced.  Leaf helpers first, then the recursive
            // dispatcher, then the public entry-point last.
            helpers.push_str(concat!(
                // --- leaf helpers (no inter-function deps) ---
                "def _json_skip_ws(s, i):\n",
                "    while i < len(s) and s[i] in \" \\t\\n\\r\":\n",
                "        i = i + 1\n",
                "    return i\n",
                "\n",
                "def _json_parse_str(s, i):\n",
                "    i = i + 1\n",
                "    result = \"\"\n",
                "    while i < len(s) and s[i] != '\"':\n",
                "        if s[i] == '\\\\':\n",
                "            i = i + 1\n",
                "            if i < len(s):\n",
                "                esc = s[i]\n",
                "                if esc == 'n':\n",
                "                    result = result + \"\\n\"\n",
                "                elif esc == 't':\n",
                "                    result = result + \"\\t\"\n",
                "                elif esc == '\"':\n",
                "                    result = result + '\"'\n",
                "                elif esc == '\\\\':\n",
                "                    result = result + '\\\\'\n",
                "                elif esc == '/':\n",
                "                    result = result + '/'\n",
                "                else:\n",
                "                    result = result + esc\n",
                "        else:\n",
                "            result = result + s[i]\n",
                "        i = i + 1\n",
                "    return [result, i + 1]\n",
                "\n",
                "def _json_parse_num(s, i):\n",
                "    start = i\n",
                "    if i < len(s) and s[i] == '-':\n",
                "        i = i + 1\n",
                "    while i < len(s) and s[i] in \"0123456789\":\n",
                "        i = i + 1\n",
                "    is_float = False\n",
                "    if i < len(s) and s[i] == '.':\n",
                "        is_float = True\n",
                "        i = i + 1\n",
                "        while i < len(s) and s[i] in \"0123456789\":\n",
                "            i = i + 1\n",
                "    if i < len(s) and s[i] in \"eE\":\n",
                "        is_float = True\n",
                "        i = i + 1\n",
                "        if i < len(s) and s[i] in \"+-\":\n",
                "            i = i + 1\n",
                "        while i < len(s) and s[i] in \"0123456789\":\n",
                "            i = i + 1\n",
                "    raw = s[start:i]\n",
                "    if is_float:\n",
                "        return [float(raw), i]\n",
                "    return [int(raw), i]\n",
                "\n",
                // --- recursive dispatcher (inlines obj/arr to avoid cycles) ---
                "def _json_parse(s, i):\n",
                "    i = _json_skip_ws(s, i)\n",
                "    if i >= len(s):\n",
                "        return [None, i]\n",
                "    c = s[i]\n",
                "    if c == '\"':\n",
                "        return _json_parse_str(s, i)\n",
                "    if c == '{':\n",
                "        i = i + 1\n",
                "        obj = {}\n",
                "        i = _json_skip_ws(s, i)\n",
                "        if i < len(s) and s[i] == '}':\n",
                "            return [obj, i + 1]\n",
                "        while i < len(s):\n",
                "            i = _json_skip_ws(s, i)\n",
                "            key, i = _json_parse_str(s, i)\n",
                "            i = _json_skip_ws(s, i)\n",
                "            i = i + 1\n",
                "            val, i = _json_parse(s, i)\n",
                "            obj[key] = val\n",
                "            i = _json_skip_ws(s, i)\n",
                "            if i < len(s) and s[i] == ',':\n",
                "                i = i + 1\n",
                "            else:\n",
                "                break\n",
                "        return [obj, i + 1]\n",
                "    if c == '[':\n",
                "        i = i + 1\n",
                "        arr = []\n",
                "        i = _json_skip_ws(s, i)\n",
                "        if i < len(s) and s[i] == ']':\n",
                "            return [arr, i + 1]\n",
                "        while i < len(s):\n",
                "            val, i = _json_parse(s, i)\n",
                "            arr.append(val)\n",
                "            i = _json_skip_ws(s, i)\n",
                "            if i < len(s) and s[i] == ',':\n",
                "                i = i + 1\n",
                "            else:\n",
                "                break\n",
                "        return [arr, i + 1]\n",
                "    if s[i:i+4] == \"true\":\n",
                "        return [True, i + 4]\n",
                "    if s[i:i+5] == \"false\":\n",
                "        return [False, i + 5]\n",
                "    if s[i:i+4] == \"null\":\n",
                "        return [None, i + 4]\n",
                "    return _json_parse_num(s, i)\n",
                "\n",
                // --- public entry-point (all deps defined above) ---
                "def _json_loads(s):\n",
                "    s = s.strip()\n",
                "    val, _ = _json_parse(s, 0)\n",
                "    return val\n",
            ));
            result = result.replace("json.loads(", "_json_loads(");
            if has_standalone_loads {
                result = Self::replace_standalone_call(&result, "loads", "_json_loads");
            }
        }

        format!("{}\n{}", helpers.trim(), result)
    }

    /// Rewrite `reduce(func, iterable)` and `functools.reduce(func, iterable)`.
    ///
    /// Injects a `_reduce` helper and replaces calls.
    fn rewrite_reduce(code: &str) -> String {
        let has_reduce = Self::has_standalone_call(code, "reduce");
        let has_functools_reduce = code.contains("functools.reduce(");

        if !has_reduce && !has_functools_reduce {
            return code.to_string();
        }

        let helper = concat!(
            "def _reduce(fn, items, initial=None):\n",
            "    items = list(items)\n",
            "    if initial is not None:\n",
            "        acc = initial\n",
            "        start = 0\n",
            "    else:\n",
            "        acc = items[0]\n",
            "        start = 1\n",
            "    for i in range(start, len(items)):\n",
            "        acc = fn(acc, items[i])\n",
            "    return acc\n",
        );

        let mut result = code.to_string();

        // functools.reduce(...) → _reduce(...)
        if has_functools_reduce {
            result = result.replace("functools.reduce(", "_reduce(");
        }

        // standalone reduce(...) → _reduce(...)
        if has_reduce {
            result = Self::replace_standalone_call(&result, "reduce", "_reduce");
        }

        format!("{}\n{}", helper.trim(), result)
    }

    /// Rewrite `Counter(iterable)` and `collections.Counter(iterable)`.
    ///
    /// Injects a `_counter` helper and replaces calls.
    fn rewrite_counter(code: &str) -> String {
        let has_counter = Self::has_standalone_call(code, "Counter");
        let has_collections_counter = code.contains("collections.Counter(");

        if !has_counter && !has_collections_counter {
            return code.to_string();
        }

        let helper = concat!(
            "def _counter(items):\n",
            "    counts = {}\n",
            "    for item in items:\n",
            "        if item in counts:\n",
            "            counts[item] = counts[item] + 1\n",
            "        else:\n",
            "            counts[item] = 1\n",
            "    return counts\n",
        );

        let mut result = code.to_string();

        // collections.Counter(...) → _counter(...)
        if has_collections_counter {
            result = result.replace("collections.Counter(", "_counter(");
        }

        // standalone Counter(...) → _counter(...)
        if has_counter {
            result = Self::replace_standalone_call(&result, "Counter", "_counter");
        }

        format!("{}\n{}", helper.trim(), result)
    }

    /// Rewrite `map(func, iterable)` and `filter(func, iterable)` to helper functions.
    ///
    /// Monty doesn't have `map` or `filter` as builtins.
    /// Injects `_map` and `_filter` helper functions and replaces calls.
    ///
    /// Only replaces standalone calls — not `obj.map(...)` or `"map"` in strings.
    fn rewrite_map_filter(code: &str) -> String {
        let needs_map = Self::has_standalone_call(code, "map");
        let needs_filter = Self::has_standalone_call(code, "filter");

        if !needs_map && !needs_filter {
            return code.to_string();
        }

        let mut helpers = String::new();
        let mut result = code.to_string();

        if needs_map {
            helpers.push_str(
                "def _map(fn, items):\n    result = []\n    for _x in items:\n        result.append(fn(_x))\n    return result\n",
            );
            result = Self::replace_standalone_call(&result, "map", "_map");
        }

        if needs_filter {
            helpers.push_str(
                "def _filter(fn, items):\n    result = []\n    for _x in items:\n        if fn(_x):\n            result.append(_x)\n    return result\n",
            );
            result = Self::replace_standalone_call(&result, "filter", "_filter");
        }

        format!("{}\n{}", helpers.trim(), result)
    }

    /// Rewrite `re.*` calls to use `run_command("python3 -c ...")`.
    ///
    /// Injects helpers: `_re_findall`, `_re_search`, `_re_sub`, `_re_split`, `_re_match`.
    /// Each helper builds a python3 one-liner that uses the real `re` module
    /// and parses the output back.
    fn rewrite_re(code: &str) -> String {
        if !code.contains("re.") {
            return code.to_string();
        }

        let needs_findall = code.contains("re.findall(");
        let needs_search = code.contains("re.search(");
        let needs_sub = code.contains("re.sub(");
        let needs_split = code.contains("re.split(");
        let needs_match = code.contains("re.match(");

        if !needs_findall && !needs_search && !needs_sub && !needs_split && !needs_match {
            return code.to_string();
        }

        let mut helpers = String::new();
        let mut result = code.to_string();

        // Common JSON parse helper (reuse _json_loads if present, else add inline)
        let needs_json_parse = needs_findall || needs_search || needs_split || needs_match;
        if needs_json_parse && !result.contains("def _json_loads(") {
            helpers.push_str(concat!(
                "def _json_parse_simple(s):\n",
                "    if isinstance(s, list) or isinstance(s, dict):\n",
                "        return s\n",
                "    s = str(s).strip()\n",
                "    if s == \"null\" or s == \"None\":\n",
                "        return None\n",
                "    if s[0:1] == \"[\":\n",
                "        inner = s[1:-1].strip()\n",
                "        if inner == \"\":\n",
                "            return []\n",
                "        parts = []\n",
                "        current = \"\"\n",
                "        depth = 0\n",
                "        in_str = False\n",
                "        for c in inner:\n",
                "            if in_str:\n",
                "                current = current + c\n",
                "                if c == '\"':\n",
                "                    in_str = False\n",
                "            elif c == '\"':\n",
                "                in_str = True\n",
                "                current = current + c\n",
                "            elif c in \"([{\":\n",
                "                depth = depth + 1\n",
                "                current = current + c\n",
                "            elif c in \")]}\":\n",
                "                depth = depth - 1\n",
                "                current = current + c\n",
                "            elif c == \",\" and depth == 0:\n",
                "                parts.append(current.strip())\n",
                "                current = \"\"\n",
                "            else:\n",
                "                current = current + c\n",
                "        if current.strip():\n",
                "            parts.append(current.strip())\n",
                "        result = []\n",
                "        for p in parts:\n",
                "            if p[0:1] == \"'\" or p[0:1] == '\"':\n",
                "                result.append(p[1:-1])\n",
                "            else:\n",
                "                result.append(p)\n",
                "        return result\n",
                "    if s[0:1] == \"'\" or s[0:1] == '\"':\n",
                "        return s[1:-1]\n",
                "    return s\n",
            ));
        }

        // NOTE: python3 commands use aliased imports (`import re as _R, json as _J`)
        // to avoid `rewrite_json`'s global `str::replace("json.dumps(", ...)` from
        // corrupting the command strings during LLM rewrite cycles.

        if needs_findall {
            helpers.push_str(concat!(
                "def _re_findall(pattern, text):\n",
                "    cmd = 'python3 -c \"import re as _R,json as _J,sys; print(_J.dumps(_R.findall(sys.argv[1], sys.argv[2])))\" '\n",
                "    cmd = cmd + _shell_quote(pattern) + ' ' + _shell_quote(text)\n",
                "    out = run_command(cmd)\n",
                "    return _json_parse_simple(out)\n",
            ));
            result = result.replace("re.findall(", "_re_findall(");
        }

        if needs_search {
            helpers.push_str(concat!(
                "def _re_search(pattern, text):\n",
                "    cmd = 'python3 -c \"import re as _R,json as _J,sys; m=_R.search(sys.argv[1],sys.argv[2]); print(_J.dumps(m.group(0) if m else None))\" '\n",
                "    cmd = cmd + _shell_quote(pattern) + ' ' + _shell_quote(text)\n",
                "    out = run_command(cmd)\n",
                "    return _json_parse_simple(out)\n",
            ));
            result = result.replace("re.search(", "_re_search(");
        }

        if needs_match {
            helpers.push_str(concat!(
                "def _re_match(pattern, text):\n",
                "    cmd = 'python3 -c \"import re as _R,json as _J,sys; m=_R.match(sys.argv[1],sys.argv[2]); print(_J.dumps(m.group(0) if m else None))\" '\n",
                "    cmd = cmd + _shell_quote(pattern) + ' ' + _shell_quote(text)\n",
                "    out = run_command(cmd)\n",
                "    return _json_parse_simple(out)\n",
            ));
            result = result.replace("re.match(", "_re_match(");
        }

        if needs_sub {
            helpers.push_str(concat!(
                "def _re_sub(pattern, repl, text):\n",
                "    cmd = 'python3 -c \"import re as _R,sys; print(_R.sub(sys.argv[1],sys.argv[2],sys.argv[3]))\" '\n",
                "    cmd = cmd + _shell_quote(pattern) + ' ' + _shell_quote(repl) + ' ' + _shell_quote(text)\n",
                "    out = run_command(cmd)\n",
                "    return out.strip()\n",
            ));
            result = result.replace("re.sub(", "_re_sub(");
        }

        if needs_split {
            helpers.push_str(concat!(
                "def _re_split(pattern, text):\n",
                "    cmd = 'python3 -c \"import re as _R,json as _J,sys; print(_J.dumps(_R.split(sys.argv[1],sys.argv[2])))\" '\n",
                "    cmd = cmd + _shell_quote(pattern) + ' ' + _shell_quote(text)\n",
                "    out = run_command(cmd)\n",
                "    return _json_parse_simple(out)\n",
            ));
            result = result.replace("re.split(", "_re_split(");
        }

        // Shell quoting helper
        if !helpers.is_empty() && !result.contains("def _shell_quote(") {
            helpers = format!(
                "def _shell_quote(s):\n    return \"'\" + str(s).replace(\"'\", \"'\\\\''\" ) + \"'\"\n{}",
                helpers
            );
        }

        format!("{}\n{}", helpers.trim(), result)
    }

    /// Rewrite `datetime.*` calls to use `run_command`.
    ///
    /// Handles:
    /// - `datetime.datetime.now()` → current datetime string
    /// - `datetime.date.today()` → current date string
    /// - `datetime.datetime.strptime(s, fmt)` → parsed datetime dict
    /// - `datetime.datetime.fromisoformat(s)` → parsed datetime dict
    fn rewrite_datetime(code: &str) -> String {
        if !code.contains("datetime.") {
            return code.to_string();
        }

        let needs_now = code.contains("datetime.datetime.now(") || code.contains("datetime.now(");
        let needs_today = code.contains("datetime.date.today(");
        let needs_strptime =
            code.contains("datetime.datetime.strptime(") || code.contains("datetime.strptime(");
        let needs_fromisoformat = code.contains("datetime.datetime.fromisoformat(")
            || code.contains("datetime.fromisoformat(");

        if !needs_now && !needs_today && !needs_strptime && !needs_fromisoformat {
            return code.to_string();
        }

        let mut helpers = String::new();
        let mut result = code.to_string();

        // Shell quoting helper
        if !result.contains("def _shell_quote(") {
            helpers.push_str("def _shell_quote(s):\n    return \"'\" + str(s).replace(\"'\", \"'\\\\''\" ) + \"'\"\n");
        }

        if needs_now {
            helpers.push_str(concat!(
                "def _datetime_now():\n",
                "    out = run_command('python3 -c \"import datetime; print(datetime.datetime.now().isoformat())\"')\n",
                "    return out.strip()\n",
            ));
            result = result.replace("datetime.datetime.now()", "_datetime_now()");
            result = result.replace("datetime.now()", "_datetime_now()");
        }

        if needs_today {
            helpers.push_str(concat!(
                "def _date_today():\n",
                "    out = run_command('python3 -c \"import datetime; print(datetime.date.today().isoformat())\"')\n",
                "    return out.strip()\n",
            ));
            result = result.replace("datetime.date.today()", "_date_today()");
        }

        if needs_strptime {
            helpers.push_str(concat!(
                "def _datetime_strptime(date_str, fmt):\n",
                "    cmd = 'python3 -c \"import datetime,sys; d=datetime.datetime.strptime(sys.argv[1],sys.argv[2]); print(d.isoformat())\" '\n",
                "    cmd = cmd + _shell_quote(date_str) + ' ' + _shell_quote(fmt)\n",
                "    out = run_command(cmd)\n",
                "    return out.strip()\n",
            ));
            result = result.replace("datetime.datetime.strptime(", "_datetime_strptime(");
            result = result.replace("datetime.strptime(", "_datetime_strptime(");
        }

        if needs_fromisoformat {
            helpers.push_str(concat!(
                "def _datetime_fromisoformat(s):\n",
                "    cmd = 'python3 -c \"import datetime,sys; print(datetime.datetime.fromisoformat(sys.argv[1]).isoformat())\" '\n",
                "    cmd = cmd + _shell_quote(s)\n",
                "    out = run_command(cmd)\n",
                "    return out.strip()\n",
            ));
            result = result.replace(
                "datetime.datetime.fromisoformat(",
                "_datetime_fromisoformat(",
            );
            result = result.replace("datetime.fromisoformat(", "_datetime_fromisoformat(");
        }

        format!("{}\n{}", helpers.trim(), result)
    }

    /// Rewrite `random.*` calls to use `run_command`.
    ///
    /// Handles:
    /// - `random.randint(a, b)` → random integer
    /// - `random.choice(seq)` → random element (uses len)
    /// - `random.random()` → float 0..1
    /// - `random.shuffle(lst)` → shuffled in place
    /// - `random.sample(pop, k)` → k random elements
    fn rewrite_random(code: &str) -> String {
        if !code.contains("random.") {
            return code.to_string();
        }

        let needs_randint = code.contains("random.randint(");
        let needs_choice = code.contains("random.choice(");
        let needs_random = code.contains("random.random(");
        let needs_shuffle = code.contains("random.shuffle(");
        let needs_sample = code.contains("random.sample(");

        if !needs_randint && !needs_choice && !needs_random && !needs_shuffle && !needs_sample {
            return code.to_string();
        }

        let mut helpers = String::new();
        let mut result = code.to_string();

        if needs_randint {
            helpers.push_str(concat!(
                "def _random_randint(a, b):\n",
                "    cmd = 'python3 -c \"import random,sys; print(random.randint(int(sys.argv[1]),int(sys.argv[2])))\" '\n",
                "    cmd = cmd + str(a) + ' ' + str(b)\n",
                "    out = run_command(cmd)\n",
                "    return int(out.strip())\n",
            ));
            result = result.replace("random.randint(", "_random_randint(");
        }

        if needs_random {
            helpers.push_str(concat!(
                "def _random_random():\n",
                "    out = run_command('python3 -c \"import random; print(random.random())\"')\n",
                "    return float(out.strip())\n",
            ));
            result = result.replace("random.random()", "_random_random()");
        }

        if needs_choice {
            // For random.choice, we use the length to pick a random index
            helpers.push_str(concat!(
                "def _random_choice(seq):\n",
                "    items = list(seq)\n",
                "    idx = _random_randint(0, len(items) - 1)\n",
                "    return items[idx]\n",
            ));
            // Ensure randint helper is also available
            if !needs_randint {
                helpers.push_str(concat!(
                    "def _random_randint(a, b):\n",
                    "    cmd = 'python3 -c \"import random,sys; print(random.randint(int(sys.argv[1]),int(sys.argv[2])))\" '\n",
                    "    cmd = cmd + str(a) + ' ' + str(b)\n",
                    "    out = run_command(cmd)\n",
                    "    return int(out.strip())\n",
                ));
            }
            result = result.replace("random.choice(", "_random_choice(");
        }

        if needs_shuffle {
            // Shuffle in-place using Fisher-Yates with random.randint
            helpers.push_str(concat!(
                "def _random_shuffle(lst):\n",
                "    n = len(lst)\n",
                "    for i in range(n - 1, 0, -1):\n",
                "        j = _random_randint(0, i)\n",
                "        tmp = lst[i]\n",
                "        lst[i] = lst[j]\n",
                "        lst[j] = tmp\n",
            ));
            if !needs_randint && !needs_choice {
                helpers.push_str(concat!(
                    "def _random_randint(a, b):\n",
                    "    cmd = 'python3 -c \"import random,sys; print(random.randint(int(sys.argv[1]),int(sys.argv[2])))\" '\n",
                    "    cmd = cmd + str(a) + ' ' + str(b)\n",
                    "    out = run_command(cmd)\n",
                    "    return int(out.strip())\n",
                ));
            }
            result = result.replace("random.shuffle(", "_random_shuffle(");
        }

        if needs_sample {
            helpers.push_str(concat!(
                "def _random_sample(population, k):\n",
                "    items = list(population)\n",
                "    result = []\n",
                "    for _ in range(k):\n",
                "        idx = _random_randint(0, len(items) - 1)\n",
                "        result.append(items[idx])\n",
                "        items = items[:idx] + items[idx + 1:]\n",
                "    return result\n",
            ));
            if !needs_randint && !needs_choice && !needs_shuffle {
                helpers.push_str(concat!(
                    "def _random_randint(a, b):\n",
                    "    cmd = 'python3 -c \"import random,sys; print(random.randint(int(sys.argv[1]),int(sys.argv[2])))\" '\n",
                    "    cmd = cmd + str(a) + ' ' + str(b)\n",
                    "    out = run_command(cmd)\n",
                    "    return int(out.strip())\n",
                ));
            }
            result = result.replace("random.sample(", "_random_sample(");
        }

        format!("{}\n{}", helpers.trim(), result)
    }

    /// Rewrite `time.sleep(N)` → `run_command("sleep N")`.
    /// Also handles `time.time()` for elapsed-time patterns.
    fn rewrite_time(code: &str) -> String {
        if !code.contains("time.") {
            return code.to_string();
        }

        let mut helpers = String::new();
        let mut result = code.to_string();

        if code.contains("time.sleep(") {
            helpers.push_str(concat!(
                "def _time_sleep(seconds):\n",
                "    run_command(\"sleep \" + str(seconds))\n",
            ));
            result = result.replace("time.sleep(", "_time_sleep(");
        }

        if code.contains("time.time(") {
            helpers.push_str(concat!(
                "def _time_time():\n",
                "    out = run_command('python3 -c \"import time; print(time.time())\"')\n",
                "    return float(out.strip())\n",
            ));
            result = result.replace("time.time()", "_time_time()");
        }

        if helpers.is_empty() {
            return result;
        }

        format!("{}\n{}", helpers.trim(), result)
    }

    /// Check if `code` contains a standalone function call `name(` that is NOT
    /// a method call (`.name(`) or part of another identifier (`other_name(`).
    fn has_standalone_call(code: &str, name: &str) -> bool {
        let pattern = format!("{}(", name);
        let bytes = code.as_bytes();
        let pat_bytes = pattern.as_bytes();
        let mut i = 0;
        while i + pat_bytes.len() <= bytes.len() {
            if &bytes[i..i + pat_bytes.len()] == pat_bytes {
                // Check preceding char: must not be alphanumeric, `_`, or `.`
                if i == 0 || {
                    let prev = bytes[i - 1];
                    !prev.is_ascii_alphanumeric() && prev != b'_' && prev != b'.'
                } {
                    // Check we're not inside a string (simple heuristic: count quotes before)
                    let before = &code[..i];
                    let single_quotes = before.chars().filter(|&c| c == '\'').count();
                    let double_quotes = before.chars().filter(|&c| c == '"').count();
                    if single_quotes % 2 == 0 && double_quotes % 2 == 0 {
                        return true;
                    }
                }
            }
            i += 1;
        }
        false
    }

    /// Replace standalone calls of `name(` with `replacement(` in code.
    /// Does NOT replace method calls (`.name(`) or identifiers containing name.
    fn replace_standalone_call(code: &str, name: &str, replacement: &str) -> String {
        let pat = format!("{}(", name);
        let rep = format!("{}(", replacement);
        let bytes = code.as_bytes();
        let pat_bytes = pat.as_bytes();
        let mut result = String::new();
        let mut i = 0;

        while i < bytes.len() {
            if i + pat_bytes.len() <= bytes.len() && &bytes[i..i + pat_bytes.len()] == pat_bytes {
                let is_standalone = i == 0 || {
                    let prev = bytes[i - 1];
                    !prev.is_ascii_alphanumeric() && prev != b'_' && prev != b'.'
                };
                if is_standalone {
                    result.push_str(&rep);
                    i += pat_bytes.len();
                    continue;
                }
            }
            // Use char iteration to preserve multi-byte UTF-8 characters
            let ch = code[i..].chars().next().unwrap();
            result.push(ch);
            i += ch.len_utf8();
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_await() {
        let code = "result = await browser_eval_js(\"document.title\")\nprint(result)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(!fixed.contains("await "));
        assert!(fixed.contains("result = browser_eval_js(\"document.title\")"));
    }

    #[test]
    fn test_strip_async_def() {
        let code = "async def fetch():\n    data = await get_data()\n    return data";
        let fixed = MontyAutoFixer::fix(code);
        assert!(!fixed.contains("async "));
        assert!(!fixed.contains("await "));
        assert!(fixed.contains("def fetch():"));
        assert!(fixed.contains("data = get_data()"));
    }

    #[test]
    fn test_no_strip_await_when_absent() {
        let code = "x = browser_eval_js(\"test\")\nprint(x)";
        let fixed = MontyAutoFixer::fix(code);
        assert_eq!(fixed, code);
    }

    #[test]
    fn test_strip_imports() {
        let code = "import os\nimport sys\nx = 1\nfrom pathlib import Path\ny = 2";
        let fixed = MontyAutoFixer::fix(code);
        assert_eq!(fixed, "x = 1\ny = 2");
    }

    #[test]
    fn test_strip_decorators() {
        let code = "@dataclass\ndef make_item(name):\n    return {\"name\": name}";
        let fixed = MontyAutoFixer::fix(code);
        assert_eq!(fixed, "def make_item(name):\n    return {\"name\": name}");
    }

    #[test]
    fn test_strip_typing_imports() {
        let code = "from typing import List, Dict\nx: List[int] = [1, 2]";
        let fixed = MontyAutoFixer::fix(code);
        // Both the import AND the type annotation are stripped
        assert_eq!(fixed, "x = [1, 2]");
    }

    #[test]
    fn test_strip_type_ignore() {
        let code = "x = foo()  # type: ignore\ny = bar()  # type: ignore[no-return]";
        let fixed = MontyAutoFixer::fix(code);
        assert_eq!(fixed, "x = foo()\ny = bar()");
    }

    #[test]
    fn test_preserves_normal_code() {
        let code = "def greet(name):\n    return f\"Hello, {name}!\"";
        let fixed = MontyAutoFixer::fix(code);
        assert_eq!(fixed, code);
    }

    #[test]
    fn test_strip_multiple_decorators() {
        let code = "@property\n@staticmethod\ndef foo():\n    pass";
        let fixed = MontyAutoFixer::fix(code);
        assert_eq!(fixed, "def foo():\n    pass");
    }

    #[test]
    fn test_decorator_with_arguments() {
        let code = "@app.route(\"/\")\ndef index():\n    return \"hello\"";
        let fixed = MontyAutoFixer::fix(code);
        assert_eq!(fixed, "def index():\n    return \"hello\"");
    }

    #[test]
    fn test_mixed_code() {
        let code = "import os\nfrom typing import List\n\n@dataclass\ndef process(items):\n    x = compute()  # type: ignore\n    return x";
        let fixed = MontyAutoFixer::fix(code);
        assert_eq!(
            fixed,
            "def process(items):\n    x = compute()\n    return x"
        );
    }

    #[test]
    fn test_empty_input() {
        let fixed = MontyAutoFixer::fix("");
        assert_eq!(fixed, "");
    }

    #[test]
    fn test_only_imports() {
        let code = "import os\nimport sys\nfrom pathlib import Path";
        let fixed = MontyAutoFixer::fix(code);
        assert_eq!(fixed, "");
    }

    #[test]
    fn test_preserves_indentation() {
        let code = "def foo():\n    if True:\n        x = 1\n        y = 2";
        let fixed = MontyAutoFixer::fix(code);
        assert_eq!(fixed, code);
    }

    #[test]
    fn test_from_variable_not_stripped() {
        // `from_email = ...` should NOT be stripped (no ` import ` present)
        let code = "from_email = \"test@example.com\"\nresult = send(from_email)";
        let fixed = MontyAutoFixer::fix(code);
        assert_eq!(fixed, code);
    }

    // === Markdown artifact stripping tests ===

    #[test]
    fn test_strip_leading_code_fence() {
        let code = "```python\nx = 1\nprint(x)\n```";
        let fixed = MontyAutoFixer::fix(code);
        assert_eq!(fixed, "x = 1\nprint(x)");
    }

    #[test]
    fn test_strip_python_exec_fence() {
        let code = "```python-exec\nx = 1\nprint(x)\n```";
        let fixed = MontyAutoFixer::fix(code);
        assert_eq!(fixed, "x = 1\nprint(x)");
    }

    #[test]
    fn test_strip_backtick_only_lines() {
        let code = "````\nx = 1\n````";
        let fixed = MontyAutoFixer::fix(code);
        assert_eq!(fixed, "x = 1");
    }

    #[test]
    fn test_no_strip_backticks_in_code() {
        // Backticks inside f-strings or dict keys should be preserved
        let code = "msg = f\"use `print` to debug\"\nprint(msg)";
        let fixed = MontyAutoFixer::fix(code);
        assert_eq!(fixed, code);
    }

    #[test]
    fn test_clean_code_unchanged() {
        let code = "x = 1\nfor i in range(10):\n    print(i)";
        let fixed = MontyAutoFixer::fix(code);
        assert_eq!(fixed, code);
    }

    // === Type annotation stripping tests ===

    #[test]
    fn test_strip_simple_type_annotation() {
        let code = "x: int = 1";
        let fixed = MontyAutoFixer::fix(code);
        assert_eq!(fixed, "x = 1");
    }

    #[test]
    fn test_strip_complex_type_annotation() {
        let code = "results: list[str] = []";
        let fixed = MontyAutoFixer::fix(code);
        assert_eq!(fixed, "results = []");
    }

    #[test]
    fn test_strip_annotation_preserves_indent() {
        let code = "    total: int = 0";
        let fixed = MontyAutoFixer::fix(code);
        assert_eq!(fixed, "    total = 0");
    }

    #[test]
    fn test_no_strip_dict_literal() {
        let code = "d = {\"key\": \"value\"}";
        let fixed = MontyAutoFixer::fix(code);
        assert_eq!(fixed, code);
    }

    #[test]
    fn test_no_strip_slice() {
        let code = "x = items[1:3]";
        let fixed = MontyAutoFixer::fix(code);
        assert_eq!(fixed, code);
    }

    #[test]
    fn test_no_strip_def_annotation() {
        let code = "def foo(x: int) -> str:\n    return str(x)";
        let fixed = MontyAutoFixer::fix(code);
        assert_eq!(fixed, code);
    }

    // === sorted() keyword argument rewriting tests ===

    #[test]
    fn test_sorted_with_key_lambda() {
        let code = "result = sorted(items, key=lambda x: x['name'])";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_keysort(items, lambda x: x['name'])"));
        assert!(fixed.contains("def _keysort("));
        assert!(!fixed.contains("sorted(items, key="));
    }

    #[test]
    fn test_sorted_with_key_and_reverse() {
        let code = "result = sorted(data, key=lambda x: x[1], reverse=True)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_keysort(data, lambda x: x[1], True)"));
    }

    #[test]
    fn test_sorted_with_reverse_only() {
        let code = "result = sorted(nums, reverse=True)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_keysort(nums, None, True)"));
    }

    #[test]
    fn test_sorted_without_key_unchanged() {
        let code = "result = sorted(items)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("sorted(items)"));
        assert!(!fixed.contains("_keysort"));
    }

    #[test]
    fn test_sorted_multiple_calls() {
        let code = "a = sorted(x, key=lambda i: i)\nb = sorted(y, key=lambda j: j)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_keysort(x, lambda i: i)"));
        assert!(fixed.contains("_keysort(y, lambda j: j)"));
        // Helper should only appear once
        let count = fixed.matches("def _keysort(").count();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_sorted_nested_in_expression() {
        let code = "for item in sorted(data, key=lambda x: x['count']):\n    print(item)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_keysort(data, lambda x: x['count'])"));
    }

    // === .sort() method rewriting tests ===

    #[test]
    fn test_sort_method_with_key() {
        let code = "items.sort(key=lambda x: x['name'])";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("items = _keysort(items, lambda x: x['name'])"));
        assert!(fixed.contains("def _keysort("));
        assert!(!fixed.contains(".sort("));
    }

    #[test]
    fn test_sort_method_with_key_and_reverse() {
        let code = "jobs_data.sort(key=lambda x: x['match_score'], reverse=True)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("jobs_data = _keysort(jobs_data, lambda x: x['match_score'], True)"));
    }

    #[test]
    fn test_sort_method_with_reverse_only() {
        let code = "nums.sort(reverse=True)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("nums = _keysort(nums, None, True)"));
    }

    #[test]
    fn test_sort_method_without_key_unchanged() {
        let code = "items.sort()";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("items.sort()"));
        assert!(!fixed.contains("_keysort"));
    }

    #[test]
    fn test_sort_method_preserves_indent() {
        let code = "    data.sort(key=lambda x: x[0])";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("    data = _keysort(data, lambda x: x[0])"));
    }

    #[test]
    fn test_sort_method_with_attribute_access() {
        let code = "self.items.sort(key=lambda x: x.name)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("self.items = _keysort(self.items, lambda x: x.name)"));
    }

    // === map() rewriting tests ===

    #[test]
    fn test_map_lambda() {
        let code = "result = list(map(lambda x: x * 2, items))";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_map(lambda x: x * 2, items)"));
        assert!(fixed.contains("def _map("));
        assert!(!fixed.contains(" map(lambda"));
    }

    #[test]
    fn test_map_function_ref() {
        let code = "result = list(map(str, numbers))";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_map(str, numbers)"));
    }

    #[test]
    fn test_map_in_for_loop() {
        let code = "for x in map(int, values):\n    print(x)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_map(int, values)"));
    }

    #[test]
    fn test_no_replace_method_map() {
        // obj.map() should NOT be rewritten
        let code = "result = df.map(func)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("df.map(func)"));
        assert!(!fixed.contains("_map"));
    }

    #[test]
    fn test_no_replace_map_variable() {
        // map_data should NOT be rewritten
        let code = "map_data = {}\nhash_map(x)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("map_data = {}"));
        assert!(fixed.contains("hash_map(x)"));
        assert!(!fixed.contains("def _map("));
    }

    // === filter() rewriting tests ===

    #[test]
    fn test_filter_lambda() {
        let code = "result = list(filter(lambda x: x > 0, items))";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_filter(lambda x: x > 0, items)"));
        assert!(fixed.contains("def _filter("));
        assert!(!fixed.contains(" filter(lambda"));
    }

    #[test]
    fn test_filter_function_ref() {
        let code = "result = list(filter(None, items))";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_filter(None, items)"));
    }

    #[test]
    fn test_no_replace_method_filter() {
        let code = "qs = queryset.filter(active=True)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("queryset.filter(active=True)"));
        assert!(!fixed.contains("_filter"));
    }

    // === Combined tests ===

    #[test]
    fn test_map_and_filter_together() {
        let code = "result = list(map(str, filter(lambda x: x > 0, nums)))";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_map(str, _filter(lambda x: x > 0, nums))"));
        assert!(fixed.contains("def _map("));
        assert!(fixed.contains("def _filter("));
    }

    #[test]
    fn test_no_map_filter_when_absent() {
        let code = "x = sorted([3, 1, 2])\nprint(x)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(!fixed.contains("def _map("));
        assert!(!fixed.contains("def _filter("));
    }

    // === math.* rewriting tests ===

    #[test]
    fn test_math_pi_constant() {
        let code = "import math\narea = math.pi * r * r";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("3.141592653589793"));
        assert!(!fixed.contains("math.pi"));
        assert!(!fixed.contains("import math"));
    }

    #[test]
    fn test_math_e_constant() {
        let code = "import math\nval = math.e";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("2.718281828459045"));
    }

    #[test]
    fn test_math_sqrt() {
        let code = "import math\nx = math.sqrt(16)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("(16) ** 0.5"));
        assert!(!fixed.contains("math.sqrt"));
    }

    #[test]
    fn test_math_sqrt_expression() {
        let code = "y = math.sqrt(a + b)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("(a + b) ** 0.5"));
    }

    #[test]
    fn test_math_pow() {
        let code = "import math\nresult = math.pow(2, 10)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("(2) ** (10)"));
        assert!(!fixed.contains("math.pow"));
    }

    #[test]
    fn test_math_fabs() {
        let code = "import math\nx = math.fabs(-5)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("abs(-5)"));
        assert!(!fixed.contains("math.fabs"));
    }

    #[test]
    fn test_math_floor() {
        let code = "import math\nn = math.floor(3.7)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_math_floor(3.7)"));
        assert!(fixed.contains("def _math_floor("));
        assert!(!fixed.contains("math.floor"));
    }

    #[test]
    fn test_math_ceil() {
        let code = "import math\nn = math.ceil(3.2)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_math_ceil(3.2)"));
        assert!(fixed.contains("def _math_ceil("));
    }

    #[test]
    fn test_math_log() {
        let code = "import math\nx = math.log(100)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_math_log(100)"));
        assert!(fixed.contains("def _math_log("));
    }

    #[test]
    fn test_math_log2() {
        let code = "import math\nx = math.log2(8)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_math_log(8, 2)"));
    }

    #[test]
    fn test_math_log10() {
        let code = "import math\nx = math.log10(1000)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_math_log(1000, 10)"));
    }

    #[test]
    fn test_math_inf() {
        let code = "import math\nif x == math.inf:\n    pass";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("float(\"inf\")"));
    }

    #[test]
    fn test_math_no_rewrite_without_math_prefix() {
        let code = "x = sqrt(16)";
        let fixed = MontyAutoFixer::fix(code);
        assert_eq!(fixed, code);
    }

    #[test]
    fn test_math_multiple_calls() {
        let code = "import math\na = math.sqrt(4)\nb = math.floor(3.7)\nc = math.pi";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("(4) ** 0.5"));
        assert!(fixed.contains("_math_floor(3.7)"));
        assert!(fixed.contains("3.141592653589793"));
        assert!(fixed.contains("def _math_floor("));
        // No ceil helper since it's not used
        assert!(!fixed.contains("def _math_ceil("));
    }

    // === os.path.* rewriting tests ===

    #[test]
    fn test_os_path_join_two_args() {
        let code = "import os\nresult = os.path.join(base, name)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_path_join([base, name])"));
        assert!(fixed.contains("def _path_join("));
        assert!(!fixed.contains("os.path.join"));
    }

    #[test]
    fn test_os_path_join_three_args() {
        let code = "import os\np = os.path.join(root, sub, file)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_path_join([root, sub, file])"));
    }

    #[test]
    fn test_os_path_basename() {
        let code = "import os\nname = os.path.basename(\"/foo/bar.txt\")";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_path_basename(\"/foo/bar.txt\")"));
        assert!(fixed.contains("def _path_basename("));
    }

    #[test]
    fn test_os_path_dirname() {
        let code = "import os\ndir = os.path.dirname(\"/foo/bar.txt\")";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_path_dirname(\"/foo/bar.txt\")"));
        assert!(fixed.contains("def _path_dirname("));
    }

    #[test]
    fn test_os_path_splitext() {
        let code = "import os\nparts = os.path.splitext(\"file.txt\")";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_path_splitext(\"file.txt\")"));
        assert!(fixed.contains("def _path_splitext("));
    }

    #[test]
    fn test_os_sep() {
        let code = "import os\nsep = os.sep";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("\"/\""));
    }

    #[test]
    fn test_os_path_no_rewrite_without_prefix() {
        let code = "result = path.join(a, b)";
        let fixed = MontyAutoFixer::fix(code);
        assert_eq!(fixed, code);
    }

    // === functools.reduce rewriting tests ===

    #[test]
    fn test_reduce_simple() {
        let code = "from functools import reduce\ntotal = reduce(lambda a, b: a + b, nums)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_reduce(lambda a, b: a + b, nums)"));
        assert!(fixed.contains("def _reduce("));
        assert!(!fixed.contains("import"));
    }

    #[test]
    fn test_reduce_with_initial() {
        let code = "from functools import reduce\ntotal = reduce(lambda a, b: a + b, nums, 0)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_reduce(lambda a, b: a + b, nums, 0)"));
    }

    #[test]
    fn test_functools_reduce_dotted() {
        let code = "import functools\ntotal = functools.reduce(lambda a, b: a + b, items)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_reduce(lambda a, b: a + b, items)"));
    }

    #[test]
    fn test_no_reduce_without_call() {
        let code = "x = 1\ny = x + 1";
        let fixed = MontyAutoFixer::fix(code);
        assert!(!fixed.contains("_reduce"));
    }

    // === collections.Counter rewriting tests ===

    #[test]
    fn test_counter_simple() {
        let code = "from collections import Counter\ncounts = Counter(words)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_counter(words)"));
        assert!(fixed.contains("def _counter("));
        assert!(!fixed.contains("import"));
    }

    #[test]
    fn test_collections_counter_dotted() {
        let code = "import collections\ncounts = collections.Counter(items)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_counter(items)"));
    }

    #[test]
    fn test_counter_no_rewrite_method() {
        let code = "obj.Counter(x)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(!fixed.contains("_counter"));
    }

    // === json.dumps/json.loads rewriting tests ===

    #[test]
    fn test_json_dumps_simple() {
        let code = "import json\nresult = json.dumps({\"key\": \"value\"})";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_json_dumps({\"key\": \"value\"})"));
        assert!(fixed.contains("def _json_dumps("));
        assert!(!fixed.contains("json.dumps"));
    }

    #[test]
    fn test_json_loads_simple() {
        let code = "import json\ndata = json.loads(text)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_json_loads(text)"));
        assert!(fixed.contains("def _json_loads("));
        assert!(fixed.contains("def _json_parse("));
        assert!(!fixed.contains("json.loads"));
    }

    #[test]
    fn test_json_both() {
        let code = "import json\ndata = json.loads(raw)\nout = json.dumps(data)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_json_loads(raw)"));
        assert!(fixed.contains("_json_dumps(data)"));
    }

    #[test]
    fn test_json_no_rewrite_without_prefix() {
        let code = "data = loads(text)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(!fixed.contains("_json_loads"));
    }

    #[test]
    fn test_json_from_import() {
        let code = "from json import loads, dumps\ndata = loads(text)\nout = dumps(data)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_json_loads(text)"));
        assert!(fixed.contains("_json_dumps(data)"));
    }

    // === re.* rewriting tests (bash-bridged) ===

    #[test]
    fn test_re_findall() {
        let code = "import re\nmatches = re.findall(r'\\d+', text)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_re_findall(r'\\d+', text)"));
        assert!(fixed.contains("def _re_findall("));
        assert!(fixed.contains("run_command("));
        assert!(fixed.contains("def _shell_quote("));
        // The user code "re.findall" should be replaced, but the helper contains "re.findall" in the python3 command
        assert!(fixed.contains("matches = _re_findall("));
    }

    #[test]
    fn test_re_sub() {
        let code = "import re\nresult = re.sub(r'\\s+', ' ', text)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_re_sub(r'\\s+', ' ', text)"));
        assert!(fixed.contains("def _re_sub("));
    }

    #[test]
    fn test_re_search() {
        let code = "import re\nm = re.search(r'(\\w+)', text)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_re_search(r'(\\w+)', text)"));
        assert!(fixed.contains("def _re_search("));
    }

    #[test]
    fn test_re_split() {
        let code = "import re\nparts = re.split(r'[,;]', text)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_re_split(r'[,;]', text)"));
        assert!(fixed.contains("def _re_split("));
    }

    #[test]
    fn test_re_match() {
        let code = "import re\nm = re.match(r'^\\d+', text)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_re_match(r'^\\d+', text)"));
        assert!(fixed.contains("def _re_match("));
    }

    #[test]
    fn test_re_no_rewrite_without_prefix() {
        let code = "x = findall(pattern, text)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(!fixed.contains("_re_findall"));
    }

    #[test]
    fn test_re_multiple_calls() {
        let code = "import re\na = re.findall(r'\\d+', t)\nb = re.sub(r'x', 'y', t)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_re_findall("));
        assert!(fixed.contains("_re_sub("));
        // Shell quote helper should appear only once
        assert_eq!(fixed.matches("def _shell_quote(").count(), 1);
    }

    #[test]
    fn test_re_findall_with_json_dumps_no_cross_contamination() {
        // Regression: rewrite_json must NOT corrupt json.dumps inside _re_findall's python3 command.
        // When LLM rewrite copies helpers and adds json.dumps, the global replace would turn
        // 'json.dumps(' into '_json_dumps(' inside the python3 -c string, causing NameError.
        let code = "import re\nimport json\nm = re.findall(r'\\d+', t)\nout = json.dumps(m)";
        let fixed = MontyAutoFixer::fix(code);
        // User code should use internal helpers
        assert!(fixed.contains("_re_findall(r'\\d+', t)"));
        assert!(fixed.contains("_json_dumps(m)"));
        // python3 command inside _re_findall must NOT contain _json_dumps or _re_findall
        // (it uses aliased imports _J.dumps and _R.findall)
        assert!(!fixed.contains("print(_json_dumps("));
        assert!(!fixed.contains("print(_re_findall("));
    }

    // === datetime.* rewriting tests (bash-bridged) ===

    #[test]
    fn test_datetime_now() {
        let code = "import datetime\nnow = datetime.datetime.now()";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_datetime_now()"));
        assert!(fixed.contains("def _datetime_now("));
        assert!(fixed.contains("run_command("));
    }

    #[test]
    fn test_date_today() {
        let code = "import datetime\ntoday = datetime.date.today()";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_date_today()"));
        assert!(fixed.contains("def _date_today("));
    }

    #[test]
    fn test_datetime_strptime() {
        let code = "import datetime\nd = datetime.datetime.strptime(s, '%Y-%m-%d')";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_datetime_strptime(s, '%Y-%m-%d')"));
        assert!(fixed.contains("def _datetime_strptime("));
    }

    #[test]
    fn test_datetime_no_rewrite_without_prefix() {
        let code = "now = get_current_time()";
        let fixed = MontyAutoFixer::fix(code);
        assert!(!fixed.contains("_datetime"));
    }

    // === random.* rewriting tests (bash-bridged) ===

    #[test]
    fn test_random_randint() {
        let code = "import random\nn = random.randint(1, 10)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_random_randint(1, 10)"));
        assert!(fixed.contains("def _random_randint("));
        assert!(fixed.contains("run_command("));
    }

    #[test]
    fn test_random_random() {
        let code = "import random\nf = random.random()";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_random_random()"));
        assert!(fixed.contains("def _random_random("));
    }

    #[test]
    fn test_random_choice() {
        let code = "import random\nitem = random.choice(items)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_random_choice(items)"));
        assert!(fixed.contains("def _random_choice("));
        // choice depends on randint
        assert!(fixed.contains("def _random_randint("));
    }

    #[test]
    fn test_random_shuffle() {
        let code = "import random\nrandom.shuffle(items)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_random_shuffle(items)"));
        assert!(fixed.contains("def _random_shuffle("));
    }

    #[test]
    fn test_random_sample() {
        let code = "import random\nresult = random.sample(population, 3)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_random_sample(population, 3)"));
        assert!(fixed.contains("def _random_sample("));
    }

    #[test]
    fn test_random_no_rewrite_without_prefix() {
        let code = "x = randint(1, 10)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(!fixed.contains("_random"));
    }

    #[test]
    fn test_fix_with_chinese_comments() {
        // This previously panicked: "byte index 7 is not a char boundary"
        let code = "# 基于当前页面收集的职位数据进行分析\njobs = []\nprint(len(jobs))";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("# 基于当前页面"));
        assert!(fixed.contains("print(len(jobs))"));
    }

    #[test]
    fn test_fix_with_multibyte_in_sorted() {
        // sorted() with Chinese comments should not panic
        let code = "# 排序数据\nresult = sorted(items, key=lambda x: x['名前'])";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("_keysort"));
        assert!(fixed.contains("名前"));
    }

    #[test]
    fn test_replace_standalone_call_with_multibyte() {
        // Ensure multi-byte chars are preserved, not corrupted via bytes[i] as char
        let code = "# 使用map处理\nresult = map(func, items)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(fixed.contains("# 使用map处理"));
    }
}
