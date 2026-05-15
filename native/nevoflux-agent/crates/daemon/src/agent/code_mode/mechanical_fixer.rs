//! MechanicalFixer - Error-driven code fixes without LLM.
//!
//! When a runtime error matches a known pattern, applies a targeted code
//! transform instead of invoking an expensive LLM rewrite call.
//! Complements the auto_fixer (which runs pre-execution on source patterns)
//! by operating post-execution on error patterns.

/// Attempts to fix code based on a runtime error pattern.
///
/// Returns `Some(fixed_code)` if a mechanical fix was applied,
/// `None` if the error doesn't match any known pattern.
pub fn try_fix(
    code: &str,
    error_type: &str,
    error_msg: &str,
    _line: Option<usize>,
) -> Option<String> {
    // Try fixers in priority order; return first that produces different code.
    let result = fix_await_syntax(code, error_type, error_msg)
        .or_else(|| fix_name_error_map(code, error_type, error_msg))
        .or_else(|| fix_name_error_filter(code, error_type, error_msg))
        .or_else(|| fix_sorted_kwargs(code, error_type, error_msg))
        .or_else(|| fix_sort_method_kwargs(code, error_type, error_msg))
        .or_else(|| fix_max_min_kwargs(code, error_type, error_msg))
        .or_else(|| fix_name_error_reduce(code, error_type, error_msg))
        .or_else(|| fix_name_error_counter(code, error_type, error_msg))
        .or_else(|| fix_name_error_itertools(code, error_type, error_msg))
        // Bare names from `from X import Y` after import stripping:
        .or_else(|| fix_name_error_chain(code, error_type, error_msg))
        .or_else(|| fix_name_error_zip_longest(code, error_type, error_msg))
        .or_else(|| fix_name_error_ordered_dict(code, error_type, error_msg))
        .or_else(|| fix_name_error_deque(code, error_type, error_msg))
        .or_else(|| fix_name_error_islice(code, error_type, error_msg))
        // Helper functions lost during LLM rewrite cycles:
        .or_else(|| fix_name_error_shell_quote(code, error_type, error_msg))
        // File I/O: open() is not available, replace with write_file/read_file tools
        .or_else(|| fix_name_error_open(code, error_type, error_msg))
        // Tool name aliases: write→write_file, read→read_file
        .or_else(|| fix_name_error_tool_alias(code, error_type, error_msg))
        // General fallback: strip known module prefixes so the next retry
        // can match a specific name fixer (e.g. functools.reduce → reduce).
        .or_else(|| fix_module_prefix(code, error_type, error_msg));

    // Only return if the fix actually changed the code.
    result.filter(|fixed| fixed != code)
}

// ---------------------------------------------------------------------------
// Pattern 1: SyntaxError from leftover async/await
// ---------------------------------------------------------------------------

fn fix_await_syntax(code: &str, error_type: &str, error_msg: &str) -> Option<String> {
    // Match SyntaxError, NameError, and TypeError (e.g. "'str' object can't be awaited")
    if !error_type.contains("SyntaxError")
        && !error_type.contains("NameError")
        && !error_type.contains("TypeError")
    {
        return None;
    }
    let is_async_related = error_msg.contains("await")
        || error_msg.contains("async")
        || error_msg.contains("can't be awaited")
        || (error_type.contains("SyntaxError")
            && (code.contains("await ") || code.contains("async ")));
    if !is_async_related {
        return None;
    }
    let fixed = code
        .replace("await ", "")
        .replace("async def ", "def ")
        .replace("async for ", "for ")
        .replace("async with ", "with ");
    Some(fixed)
}

// ---------------------------------------------------------------------------
// Pattern 2: NameError: name 'map' is not defined
// ---------------------------------------------------------------------------

const MAP_HELPER: &str = "\
def _map_fn(fn, iterable):
    result = []
    for _x in iterable:
        result.append(fn(_x))
    return result
";

fn fix_name_error_map(code: &str, error_type: &str, error_msg: &str) -> Option<String> {
    if !error_type.contains("NameError") {
        return None;
    }
    if !extract_undefined_name(error_msg).is_some_and(|n| n == "map") {
        return None;
    }
    if !code.contains("map(") {
        return None;
    }
    // Inject helper and rename map( → _map_fn(
    let replaced = replace_function_calls(code, "map", "_map_fn");
    if replaced == code {
        return None; // No standalone map( calls found
    }
    Some(format!("{}\n{}", MAP_HELPER.trim(), replaced))
}

// ---------------------------------------------------------------------------
// Pattern 3: NameError: name 'filter' is not defined
// ---------------------------------------------------------------------------

const FILTER_HELPER: &str = "\
def _filter_fn(fn, iterable):
    result = []
    for _x in iterable:
        if fn(_x):
            result.append(_x)
    return result
";

fn fix_name_error_filter(code: &str, error_type: &str, error_msg: &str) -> Option<String> {
    if !error_type.contains("NameError") {
        return None;
    }
    if !extract_undefined_name(error_msg).is_some_and(|n| n == "filter") {
        return None;
    }
    if !code.contains("filter(") {
        return None;
    }
    let replaced = replace_function_calls(code, "filter", "_filter_fn");
    if replaced == code {
        return None;
    }
    Some(format!("{}\n{}", FILTER_HELPER.trim(), replaced))
}

// ---------------------------------------------------------------------------
// Pattern 4: TypeError: sorted() got unexpected keyword argument
// ---------------------------------------------------------------------------

fn fix_sorted_kwargs(code: &str, error_type: &str, error_msg: &str) -> Option<String> {
    if !error_type.contains("TypeError") {
        return None;
    }
    if !error_msg.contains("sorted") {
        return None;
    }
    if !error_msg.contains("keyword argument") && !error_msg.contains("unexpected") {
        return None;
    }
    // Fallback: strip key= and reverse= from sorted() calls.
    // This degrades to unsorted order but prevents the crash.
    // The auto_fixer should have handled this, so reaching here means
    // an edge case the regex missed.
    strip_sorted_keyword_args(code)
}

// ---------------------------------------------------------------------------
// Pattern 4b: TypeError: sort() got unexpected keyword argument (method call)
// ---------------------------------------------------------------------------

fn fix_sort_method_kwargs(code: &str, error_type: &str, error_msg: &str) -> Option<String> {
    if !error_type.contains("TypeError") {
        return None;
    }
    // Match .sort() TypeError — not sorted() (already handled by fix_sorted_kwargs)
    if !error_msg.contains("sort") {
        return None;
    }
    if error_msg.contains("sorted") {
        return None; // Handled by fix_sorted_kwargs
    }
    if !error_msg.contains("keyword argument") && !error_msg.contains("unexpected") {
        return None;
    }
    strip_sort_method_keyword_args(code)
}

// ---------------------------------------------------------------------------
// Pattern 4c: TypeError: max()/min() got unexpected keyword argument
// ---------------------------------------------------------------------------

fn fix_max_min_kwargs(code: &str, error_type: &str, error_msg: &str) -> Option<String> {
    if !error_type.contains("TypeError") {
        return None;
    }
    if !error_msg.contains("keyword argument") && !error_msg.contains("unexpected") {
        return None;
    }
    if error_msg.contains("max") {
        return strip_func_keyword_args(code, "max");
    }
    if error_msg.contains("min") {
        return strip_func_keyword_args(code, "min");
    }
    None
}

// ---------------------------------------------------------------------------
// Pattern 5: NameError: name 'reduce' is not defined
// ---------------------------------------------------------------------------

const REDUCE_HELPER: &str = "\
def _reduce_fn(fn, iterable, initial=None):
    it = list(iterable)
    if initial is not None:
        acc = initial
        start = 0
    else:
        acc = it[0]
        start = 1
    for i in range(start, len(it)):
        acc = fn(acc, it[i])
    return acc
";

fn fix_name_error_reduce(code: &str, error_type: &str, error_msg: &str) -> Option<String> {
    if !error_type.contains("NameError") {
        return None;
    }
    if !extract_undefined_name(error_msg).is_some_and(|n| n == "reduce") {
        return None;
    }
    if !code.contains("reduce(") {
        return None;
    }
    let replaced = replace_function_calls(code, "reduce", "_reduce_fn");
    if replaced == code {
        return None;
    }
    Some(format!("{}\n{}", REDUCE_HELPER.trim(), replaced))
}

// ---------------------------------------------------------------------------
// Pattern 6: NameError: name 'Counter' is not defined
// ---------------------------------------------------------------------------

const COUNTER_HELPER: &str = "\
def _counter_fn(items):
    counts = {}
    for item in items:
        if item in counts:
            counts[item] = counts[item] + 1
        else:
            counts[item] = 1
    return counts
";

fn fix_name_error_counter(code: &str, error_type: &str, error_msg: &str) -> Option<String> {
    if !error_type.contains("NameError") {
        return None;
    }
    if !extract_undefined_name(error_msg).is_some_and(|n| n == "Counter") {
        return None;
    }
    if !code.contains("Counter(") {
        return None;
    }
    let replaced = replace_function_calls(code, "Counter", "_counter_fn");
    if replaced == code {
        return None;
    }
    Some(format!("{}\n{}", COUNTER_HELPER.trim(), replaced))
}

// ---------------------------------------------------------------------------
// Pattern 7: NameError: name 'itertools' is not defined
// ---------------------------------------------------------------------------

fn fix_name_error_itertools(code: &str, error_type: &str, error_msg: &str) -> Option<String> {
    if !error_type.contains("NameError") {
        return None;
    }
    if !extract_undefined_name(error_msg).is_some_and(|n| n == "itertools") {
        return None;
    }
    let mut fixed = code.to_string();
    let mut changed = false;

    // itertools.chain(a, b) → list(a) + list(b)
    while fixed.contains("itertools.chain(") {
        if let Some(start) = fixed.find("itertools.chain(") {
            let open = start + "itertools.chain(".len();
            if let Some(close) = find_matching_paren(&fixed, open - 1) {
                let args = &fixed[open..close];
                // Split on top-level comma
                let parts = split_top_level_args(args);
                let replacement = parts
                    .iter()
                    .map(|p| format!("list({})", p.trim()))
                    .collect::<Vec<_>>()
                    .join(" + ");
                fixed = format!("{}{}{}", &fixed[..start], replacement, &fixed[close + 1..]);
                changed = true;
            } else {
                break;
            }
        }
    }

    // itertools.islice(it, n) → list(it)[:n]
    while fixed.contains("itertools.islice(") {
        if let Some(start) = fixed.find("itertools.islice(") {
            let open = start + "itertools.islice(".len();
            if let Some(close) = find_matching_paren(&fixed, open - 1) {
                let args = &fixed[open..close];
                let parts = split_top_level_args(args);
                let replacement = if parts.len() == 2 {
                    format!("list({})[:{}]", parts[0].trim(), parts[1].trim())
                } else if parts.len() == 3 {
                    format!(
                        "list({})[{}:{}]",
                        parts[0].trim(),
                        parts[1].trim(),
                        parts[2].trim()
                    )
                } else {
                    break; // Can't handle this form
                };
                fixed = format!("{}{}{}", &fixed[..start], replacement, &fixed[close + 1..]);
                changed = true;
            } else {
                break;
            }
        }
    }

    // Other itertools.X( → strip prefix, leave as-is (will likely fail but
    // gives a more descriptive error than "itertools not defined")
    if !changed && fixed.contains("itertools.") {
        fixed = fixed.replace("itertools.", "");
        changed = true;
    }

    if changed {
        Some(fixed)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Pattern 8: NameError: name 'chain' is not defined (bare import)
// ---------------------------------------------------------------------------

fn fix_name_error_chain(code: &str, error_type: &str, error_msg: &str) -> Option<String> {
    if !error_type.contains("NameError") {
        return None;
    }
    if !extract_undefined_name(error_msg).is_some_and(|n| n == "chain") {
        return None;
    }
    if !code.contains("chain(") {
        return None;
    }
    // Rename to temp name to avoid false positives in the while loop
    let renamed = replace_function_calls(code, "chain", "__chain_tmp");
    if renamed == code {
        return None;
    }
    // Replace __chain_tmp(a, b, ...) → list(a) + list(b) + ...
    let mut fixed = renamed;
    let mut changed = false;

    while fixed.contains("__chain_tmp(") {
        if let Some(start) = fixed.find("__chain_tmp(") {
            let open = start + "__chain_tmp(".len();
            if let Some(close) = find_matching_paren(&fixed, open - 1) {
                let args = &fixed[open..close];
                let parts = split_top_level_args(args);
                let replacement = parts
                    .iter()
                    .map(|p| format!("list({})", p.trim()))
                    .collect::<Vec<_>>()
                    .join(" + ");
                fixed = format!("{}{}{}", &fixed[..start], replacement, &fixed[close + 1..]);
                changed = true;
            } else {
                break;
            }
        }
    }

    if changed {
        Some(fixed)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Pattern 9: NameError: name 'zip_longest' is not defined (bare import)
// ---------------------------------------------------------------------------

const ZIP_LONGEST_HELPER: &str = "\
def _zip_longest_fn(a, b, fillvalue=None):
    la = list(a)
    lb = list(b)
    ml = len(la)
    if len(lb) > ml:
        ml = len(lb)
    result = []
    for i in range(ml):
        va = la[i] if i < len(la) else fillvalue
        vb = lb[i] if i < len(lb) else fillvalue
        result.append([va, vb])
    return result
";

fn fix_name_error_zip_longest(code: &str, error_type: &str, error_msg: &str) -> Option<String> {
    if !error_type.contains("NameError") {
        return None;
    }
    if !extract_undefined_name(error_msg).is_some_and(|n| n == "zip_longest") {
        return None;
    }
    if !code.contains("zip_longest(") {
        return None;
    }
    let replaced = replace_function_calls(code, "zip_longest", "_zip_longest_fn");
    if replaced == code {
        return None;
    }
    Some(format!("{}\n{}", ZIP_LONGEST_HELPER.trim(), replaced))
}

// ---------------------------------------------------------------------------
// Pattern 10: NameError: name 'OrderedDict' is not defined (bare import)
// ---------------------------------------------------------------------------

fn fix_name_error_ordered_dict(code: &str, error_type: &str, error_msg: &str) -> Option<String> {
    if !error_type.contains("NameError") {
        return None;
    }
    if !extract_undefined_name(error_msg).is_some_and(|n| n == "OrderedDict") {
        return None;
    }
    if !code.contains("OrderedDict(") {
        return None;
    }
    // OrderedDict → dict (Python 3.7+ dicts preserve insertion order)
    let replaced = replace_function_calls(code, "OrderedDict", "dict");
    if replaced == code {
        return None;
    }
    Some(replaced)
}

// ---------------------------------------------------------------------------
// Pattern 11: NameError: name 'deque' is not defined (bare import)
// ---------------------------------------------------------------------------

fn fix_name_error_deque(code: &str, error_type: &str, error_msg: &str) -> Option<String> {
    if !error_type.contains("NameError") {
        return None;
    }
    if !extract_undefined_name(error_msg).is_some_and(|n| n == "deque") {
        return None;
    }
    if !code.contains("deque(") {
        return None;
    }
    // deque → list (loses appendleft/popleft but covers basic creation)
    let replaced = replace_function_calls(code, "deque", "list");
    if replaced == code {
        return None;
    }
    Some(replaced)
}

// ---------------------------------------------------------------------------
// Pattern 12: NameError: name 'islice' is not defined (bare import)
// ---------------------------------------------------------------------------

fn fix_name_error_islice(code: &str, error_type: &str, error_msg: &str) -> Option<String> {
    if !error_type.contains("NameError") {
        return None;
    }
    if !extract_undefined_name(error_msg).is_some_and(|n| n == "islice") {
        return None;
    }
    if !code.contains("islice(") {
        return None;
    }
    // Rename to temp to avoid false positives
    let renamed = replace_function_calls(code, "islice", "__islice_tmp");
    if renamed == code {
        return None;
    }
    // Replace __islice_tmp(it, n) → list(it)[:n]
    // Replace __islice_tmp(it, start, stop) → list(it)[start:stop]
    let mut fixed = renamed;
    let mut changed = false;

    while fixed.contains("__islice_tmp(") {
        if let Some(start) = fixed.find("__islice_tmp(") {
            let open = start + "__islice_tmp(".len();
            if let Some(close) = find_matching_paren(&fixed, open - 1) {
                let args = &fixed[open..close];
                let parts = split_top_level_args(args);
                let replacement = if parts.len() == 2 {
                    format!("list({})[:{}]", parts[0].trim(), parts[1].trim())
                } else if parts.len() == 3 {
                    format!(
                        "list({})[{}:{}]",
                        parts[0].trim(),
                        parts[1].trim(),
                        parts[2].trim()
                    )
                } else {
                    break; // Can't handle this form
                };
                fixed = format!("{}{}{}", &fixed[..start], replacement, &fixed[close + 1..]);
                changed = true;
            } else {
                break;
            }
        }
    }

    if changed {
        Some(fixed)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Pattern 13: NameError: name '_shell_quote' is not defined
// ---------------------------------------------------------------------------

const SHELL_QUOTE_HELPER: &str = "\
def _shell_quote(s):
    return \"'\" + str(s).replace(\"'\", \"'\\\\''\" ) + \"'\"
";

fn fix_name_error_shell_quote(code: &str, error_type: &str, error_msg: &str) -> Option<String> {
    if !error_type.contains("NameError") {
        return None;
    }
    if !extract_undefined_name(error_msg).is_some_and(|n| n == "_shell_quote") {
        return None;
    }
    // Don't check for _shell_quote( in code — it's called indirectly by
    // auto_fixer-generated helpers like _re_findall, _datetime_strptime, etc.
    // Just inject the definition at the top.
    if code.contains("def _shell_quote(") {
        return None; // Already defined
    }
    Some(format!("{}\n{}", SHELL_QUOTE_HELPER.trim(), code))
}

// ---------------------------------------------------------------------------
// Pattern 15: NameError: name 'open' is not defined
// Monty has no `open()` builtin; replace with write_file/read_file tools.
// ---------------------------------------------------------------------------

fn fix_name_error_open(code: &str, error_type: &str, error_msg: &str) -> Option<String> {
    if !error_type.contains("NameError") {
        return None;
    }
    if !extract_undefined_name(error_msg).is_some_and(|n| n == "open") {
        return None;
    }
    if !code.contains("open(") {
        return None;
    }

    let mut result_lines: Vec<String> = Vec::new();
    let lines: Vec<&str> = code.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let stripped = line.trim();

        // Pattern A: `with open(PATH, "w"...) as VAR:`
        if let Some(block) = try_parse_with_open(stripped) {
            let block_indent = get_indent(line);
            let body_indent = format!("{}    ", block_indent);

            // Collect the indented body
            let mut body_lines: Vec<&str> = Vec::new();
            let mut j = i + 1;
            while j < lines.len() {
                let next = lines[j];
                if next.trim().is_empty() {
                    body_lines.push(next);
                    j += 1;
                    continue;
                }
                if get_indent(next).len() >= body_indent.len() {
                    body_lines.push(next);
                    j += 1;
                } else {
                    break;
                }
            }

            if block.is_write {
                // Collect all VAR.write(EXPR) calls and concatenate
                let write_prefix = format!("{}.write(", block.var_name);
                let mut content_parts: Vec<String> = Vec::new();
                let mut other_lines: Vec<String> = Vec::new();

                for bline in &body_lines {
                    let bt = bline.trim();
                    if bt.starts_with(&write_prefix) && bt.ends_with(')') {
                        let inner = &bt[write_prefix.len()..bt.len() - 1];
                        content_parts.push(inner.to_string());
                    } else if !bt.is_empty() {
                        // Non-write lines in the block — keep them
                        other_lines.push(format!("{}{}", block_indent, bt));
                    }
                }

                if !content_parts.is_empty() {
                    let content_expr = if content_parts.len() == 1 {
                        content_parts[0].clone()
                    } else {
                        content_parts.join(" + ")
                    };
                    result_lines.push(format!(
                        "{}write_file({}, {})",
                        block_indent, block.path_expr, content_expr
                    ));
                    for ol in other_lines {
                        result_lines.push(ol);
                    }
                } else {
                    // No write calls found — keep original block as-is
                    result_lines.push(line.to_string());
                    for bl in &body_lines {
                        result_lines.push(bl.to_string());
                    }
                }
            } else {
                // Read mode: find VAR.read() assignments
                let read_call = format!("{}.read()", block.var_name);
                let mut found_read = false;

                for bline in &body_lines {
                    let bt = bline.trim();
                    if bt.contains(&read_call) {
                        // e.g. "data = f.read()" → "data = read_file(PATH)"
                        let replaced =
                            bt.replace(&read_call, &format!("read_file({})", block.path_expr));
                        result_lines.push(format!("{}{}", block_indent, replaced));
                        found_read = true;
                    } else if !bt.is_empty() {
                        result_lines.push(format!("{}{}", block_indent, bt));
                    }
                }

                if !found_read {
                    // Couldn't transform — keep original
                    result_lines.push(line.to_string());
                    for bl in &body_lines {
                        result_lines.push(bl.to_string());
                    }
                }
            }

            i = j;
            continue;
        }

        // Pattern B: standalone `VAR = open(PATH, "w")` ... `VAR.write(...)` ... `VAR.close()`
        // Just replace open( calls inline — simpler fallback
        result_lines.push(line.to_string());
        i += 1;
    }

    let fixed = result_lines.join("\n");
    // Preserve trailing newline if original had one
    let fixed = if code.ends_with('\n') && !fixed.ends_with('\n') {
        format!("{}\n", fixed)
    } else {
        fixed
    };

    if fixed == code {
        None
    } else {
        Some(fixed)
    }
}

struct WithOpenBlock {
    path_expr: String,
    var_name: String,
    is_write: bool,
}

/// Parse `with open(PATH, MODE...) as VAR:` returning components.
fn try_parse_with_open(line: &str) -> Option<WithOpenBlock> {
    let rest = line.strip_prefix("with open(")?;
    // Find the matching closing paren for open(...)
    let mut depth = 1;
    let mut open_end = None;
    for (i, ch) in rest.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    open_end = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let open_end = open_end?;
    let args_str = &rest[..open_end];
    let after = rest[open_end + 1..].trim();

    // Parse "as VAR:"
    let after = after.strip_prefix("as ")?;
    let var_name = after.strip_suffix(':')?.trim().to_string();
    if var_name.is_empty() {
        return None;
    }

    // Parse open() arguments: first arg is path, second is mode
    let args = split_top_level_args(args_str);
    if args.is_empty() {
        return None;
    }
    let path_expr = args[0].trim().to_string();
    let is_write = if args.len() > 1 {
        let mode = args[1].trim();
        mode.contains('w') || mode.contains('a')
    } else {
        false // default is read
    };

    Some(WithOpenBlock {
        path_expr,
        var_name,
        is_write,
    })
}

fn get_indent(line: &str) -> String {
    let trimmed = line.trim_start();
    line[..line.len() - trimmed.len()].to_string()
}

// ---------------------------------------------------------------------------
// Pattern 16: NameError for tool name aliases (write→write_file, read→read_file)
// LLMs use `write(path, content)` / `read(path)` because the system prompt
// says "Use `write` for new files" — but the actual Monty tool functions
// are `write_file` and `read_file`.
// ---------------------------------------------------------------------------

fn fix_name_error_tool_alias(code: &str, error_type: &str, error_msg: &str) -> Option<String> {
    if !error_type.contains("NameError") {
        return None;
    }
    let name = extract_undefined_name(error_msg)?;
    match name.as_str() {
        "write" if code.contains("write(") => {
            Some(replace_function_calls(code, "write", "write_file"))
        }
        "read" if code.contains("read(") => Some(replace_function_calls(code, "read", "read_file")),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Pattern 14: NameError for known module names (functools, collections)
// ---------------------------------------------------------------------------

fn fix_module_prefix(code: &str, error_type: &str, error_msg: &str) -> Option<String> {
    if !error_type.contains("NameError") {
        return None;
    }
    let name = extract_undefined_name(error_msg)?;
    // Strip module prefixes where the auto_fixer handles the underlying
    // patterns. After stripping, the next retry runs auto_fixer again which
    // rewrites the bare calls (e.g. functools.reduce → reduce → _reduce helper,
    // re.findall → _re_findall helper via run_command).
    //
    // Also handles LLM-rewritten code that re-introduces module prefixes
    // (the rewrite output does NOT pass through auto_fixer's rewrite_* phase).
    match name.as_str() {
        "functools" | "collections" | "re" | "json" | "math" | "os" | "datetime" | "random"
        | "time" => {
            let prefix = format!("{}.", name);
            if code.contains(&*prefix) {
                Some(code.replace(&*prefix, ""))
            } else {
                None
            }
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract the undefined name from a NameError message.
/// "name 'map' is not defined" → Some("map")
fn extract_undefined_name(msg: &str) -> Option<String> {
    let start = msg.find('\'')?;
    let rest = &msg[start + 1..];
    let end = rest.find('\'')?;
    Some(rest[..end].to_string())
}

/// Replace standalone function calls `old_name(` with `new_name(`.
/// Avoids replacing inside strings, method calls (`.old_name(`), or
/// names that are substrings of longer identifiers.
fn replace_function_calls(code: &str, old_name: &str, new_name: &str) -> String {
    let search = format!("{}(", old_name);
    let replace = format!("{}(", new_name);
    let mut result = String::with_capacity(code.len());
    let mut i = 0;
    let bytes = code.as_bytes();

    while i < code.len() {
        if code[i..].starts_with(&search) {
            // Check the character before: must not be alphanumeric, underscore, or dot
            let prev_ok = if i == 0 {
                true
            } else {
                let prev = bytes[i - 1];
                !prev.is_ascii_alphanumeric() && prev != b'_' && prev != b'.'
            };
            if prev_ok {
                result.push_str(&replace);
                i += search.len();
                continue;
            }
        }
        // Safe to index: we only advance by one byte for ASCII, which covers
        // all identifier chars and operators. Multi-byte UTF-8 chars are pushed
        // via the char.
        let ch = code[i..].chars().next().unwrap();
        result.push(ch);
        i += ch.len_utf8();
    }
    result
}

/// Split a comma-separated argument string at top-level commas only.
/// Respects nested parens, brackets, braces, and string literals.
fn split_top_level_args(args: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut in_string = false;
    let mut string_char = '"';
    let mut prev_char = '\0';
    let mut start = 0;

    for (i, ch) in args.char_indices() {
        if in_string {
            if ch == string_char && prev_char != '\\' {
                in_string = false;
            }
            prev_char = ch;
            continue;
        }
        match ch {
            '\'' | '"' => {
                in_string = true;
                string_char = ch;
            }
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&args[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        prev_char = ch;
    }
    parts.push(&args[start..]);
    parts
}

/// Strip `key=` and `reverse=` keyword arguments from sorted() calls.
/// Fallback when auto_fixer's sophisticated rewrite missed an edge case.
/// Converts `sorted(items, key=..., reverse=...)` → `sorted(items)`.
fn strip_sorted_keyword_args(code: &str) -> Option<String> {
    strip_func_keyword_args(code, "sorted")
}

/// Strip keyword arguments from `.sort()` method calls.
/// Converts `expr.sort(key=..., reverse=...)` → `expr.sort()`.
fn strip_sort_method_keyword_args(code: &str) -> Option<String> {
    if !code.contains(".sort(") {
        return None;
    }

    let mut result = String::new();
    let mut changed = false;
    let mut i = 0;

    while i < code.len() {
        if code[i..].starts_with(".sort(") {
            let open = i + ".sort(".len();
            if let Some(args_end) = find_matching_paren(code, open - 1) {
                let args_str = &code[open..args_end];
                if args_str.contains("key=") || args_str.contains("reverse=") {
                    result.push_str(".sort()");
                    i = args_end + 1;
                    changed = true;
                    continue;
                }
            }
        }
        let ch = code[i..].chars().next().unwrap();
        result.push(ch);
        i += ch.len_utf8();
    }

    if changed {
        Some(result)
    } else {
        None
    }
}

/// General-purpose keyword argument stripper for function calls.
/// Finds `func_name(` calls and removes keyword arguments (key=, reverse=, default=),
/// keeping only positional arguments.
fn strip_func_keyword_args(code: &str, func_name: &str) -> Option<String> {
    let search_pattern = format!("{}(", func_name);
    if !code.contains(&*search_pattern) {
        return None;
    }

    let mut result = String::new();
    let mut changed = false;
    let mut i = 0;

    while i < code.len() {
        if code[i..].starts_with(&search_pattern) {
            let open = i + search_pattern.len();
            if let Some(args_end) = find_matching_paren(code, open - 1) {
                let args_str = &code[open..args_end];
                if args_str.contains("key=")
                    || args_str.contains("reverse=")
                    || args_str.contains("default=")
                {
                    let positional = extract_positional_args(args_str);
                    result.push_str(&search_pattern);
                    result.push_str(positional.trim());
                    result.push(')');
                    i = args_end + 1;
                    changed = true;
                    continue;
                }
            }
        }
        let ch = code[i..].chars().next().unwrap();
        result.push(ch);
        i += ch.len_utf8();
    }

    if changed {
        Some(result)
    } else {
        None
    }
}

/// Find the matching closing paren for the open paren at `open_pos`.
fn find_matching_paren(code: &str, open_pos: usize) -> Option<usize> {
    let mut depth = 0;
    let mut in_string = false;
    let mut string_char = '"';
    let mut prev_char = '\0';

    for (offset, ch) in code[open_pos..].char_indices() {
        if in_string {
            // Only close string if quote is not escaped
            if ch == string_char && prev_char != '\\' {
                in_string = false;
            }
            prev_char = ch;
            continue;
        }
        match ch {
            '\'' | '"' => {
                in_string = true;
                string_char = ch;
            }
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open_pos + offset);
                }
            }
            _ => {}
        }
        prev_char = ch;
    }
    None
}

/// Extract all positional arguments from a function args string.
/// Stops at the first `, key=`, `, reverse=`, or `, default=` boundary.
fn extract_positional_args(args: &str) -> &str {
    let mut depth = 0;
    let mut in_string = false;
    let mut string_char = '"';

    for (i, ch) in args.char_indices() {
        if in_string {
            if ch == string_char {
                in_string = false;
            }
            continue;
        }
        match ch {
            '\'' | '"' => {
                in_string = true;
                string_char = ch;
            }
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                let rest = args[i + 1..].trim_start();
                if rest.starts_with("key=")
                    || rest.starts_with("reverse=")
                    || rest.starts_with("default=")
                {
                    return &args[..i];
                }
            }
            _ => {}
        }
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- extract_undefined_name ---

    #[test]
    fn test_extract_name_from_error() {
        assert_eq!(
            extract_undefined_name("name 'map' is not defined"),
            Some("map".to_string())
        );
        assert_eq!(
            extract_undefined_name("name 'filter' is not defined"),
            Some("filter".to_string())
        );
        assert_eq!(extract_undefined_name("no quotes here"), None);
    }

    // --- fix_await_syntax ---

    #[test]
    fn test_fix_await() {
        let code = "result = await web_search(\"test\")\nprint(result)";
        let fixed = try_fix(code, "SyntaxError", "invalid syntax: await", None);
        assert!(fixed.is_some());
        let fixed = fixed.unwrap();
        assert!(!fixed.contains("await "));
        assert!(fixed.contains("result = web_search(\"test\")"));
    }

    #[test]
    fn test_fix_async_def() {
        let code = "async def fetch():\n    return await get()";
        let fixed = try_fix(code, "SyntaxError", "invalid syntax: async", None);
        assert!(fixed.is_some());
        let fixed = fixed.unwrap();
        assert!(fixed.contains("def fetch():"));
        assert!(!fixed.contains("async"));
        assert!(!fixed.contains("await"));
    }

    #[test]
    fn test_fix_await_type_error() {
        // TypeError: 'str' object can't be awaited — runtime error from await on sync return
        let code = "result = await browser_eval_js(\"document.title\")\nprint(result)";
        let fixed = try_fix(code, "TypeError", "'str' object can't be awaited", None);
        assert!(fixed.is_some());
        let fixed = fixed.unwrap();
        assert!(!fixed.contains("await "));
        assert!(fixed.contains("result = browser_eval_js(\"document.title\")"));
    }

    #[test]
    fn test_no_fix_for_unrelated_syntax_error() {
        let code = "x = 1 +";
        let fixed = try_fix(code, "SyntaxError", "unexpected EOF", None);
        assert!(fixed.is_none());
    }

    // --- fix_name_error_map ---

    #[test]
    fn test_fix_map() {
        let code = "result = map(lambda x: x * 2, items)";
        let fixed = try_fix(code, "NameError", "name 'map' is not defined", None);
        assert!(fixed.is_some());
        let fixed = fixed.unwrap();
        assert!(fixed.contains("def _map_fn("));
        assert!(fixed.contains("_map_fn(lambda x: x * 2, items)"));
        assert!(!fixed.contains("\nresult = map("));
    }

    #[test]
    fn test_fix_map_no_false_positive() {
        // bitmap( should NOT be replaced
        let code = "result = bitmap(data)";
        let fixed = try_fix(code, "NameError", "name 'map' is not defined", None);
        // The code doesn't contain standalone `map(`, so no fix
        assert!(fixed.is_none());
    }

    #[test]
    fn test_fix_map_in_list() {
        let code = "result = list(map(str, items))";
        let fixed = try_fix(code, "NameError", "name 'map' is not defined", None);
        assert!(fixed.is_some());
        let fixed = fixed.unwrap();
        assert!(fixed.contains("list(_map_fn(str, items))"));
    }

    // --- fix_name_error_filter ---

    #[test]
    fn test_fix_filter() {
        let code = "result = filter(lambda x: x > 0, nums)";
        let fixed = try_fix(code, "NameError", "name 'filter' is not defined", None);
        assert!(fixed.is_some());
        let fixed = fixed.unwrap();
        assert!(fixed.contains("def _filter_fn("));
        assert!(fixed.contains("_filter_fn(lambda x: x > 0, nums)"));
    }

    // --- fix_sorted_kwargs ---

    #[test]
    fn test_fix_sorted_key() {
        let code = "result = sorted(items, key=lambda x: x['name'])";
        let fixed = try_fix(
            code,
            "TypeError",
            "sorted() got an unexpected keyword argument 'key'",
            None,
        );
        assert!(fixed.is_some());
        let fixed = fixed.unwrap();
        assert!(fixed.contains("sorted(items)"));
        assert!(!fixed.contains("key="));
    }

    #[test]
    fn test_fix_sorted_key_and_reverse() {
        let code = "result = sorted(data, key=lambda x: x[1], reverse=True)";
        let fixed = try_fix(
            code,
            "TypeError",
            "sorted() got an unexpected keyword argument 'key'",
            None,
        );
        assert!(fixed.is_some());
        let fixed = fixed.unwrap();
        assert!(fixed.contains("sorted(data)"));
        assert!(!fixed.contains("key="));
        assert!(!fixed.contains("reverse="));
    }

    #[test]
    fn test_fix_sorted_nested() {
        let code = "for item in sorted(data, key=lambda x: x['count']):\n    print(item)";
        let fixed = try_fix(
            code,
            "TypeError",
            "sorted() got an unexpected keyword argument",
            None,
        );
        assert!(fixed.is_some());
        let fixed = fixed.unwrap();
        assert!(fixed.contains("sorted(data)"));
    }

    #[test]
    fn test_fix_sorted_without_kwargs_unchanged() {
        let code = "result = sorted(items)";
        let fixed = try_fix(
            code,
            "TypeError",
            "sorted() got an unexpected keyword argument",
            None,
        );
        // No kwargs to strip, so no fix
        assert!(fixed.is_none());
    }

    // --- fix_name_error_reduce ---

    #[test]
    fn test_fix_reduce() {
        let code = "total = reduce(lambda a, b: a + b, numbers)";
        let fixed = try_fix(code, "NameError", "name 'reduce' is not defined", None);
        assert!(fixed.is_some());
        let fixed = fixed.unwrap();
        assert!(fixed.contains("def _reduce_fn("));
        assert!(fixed.contains("_reduce_fn(lambda a, b: a + b, numbers)"));
    }

    // --- replace_function_calls ---

    #[test]
    fn test_replace_skips_method_calls() {
        let result = replace_function_calls("obj.map(x)", "map", "_map_fn");
        assert_eq!(result, "obj.map(x)"); // dot prefix → skip
    }

    #[test]
    fn test_replace_skips_longer_names() {
        let result = replace_function_calls("bitmap(x)", "map", "_map_fn");
        assert_eq!(result, "bitmap(x)"); // part of longer name → skip
    }

    #[test]
    fn test_replace_at_line_start() {
        let result = replace_function_calls("map(fn, items)", "map", "_map_fn");
        assert_eq!(result, "_map_fn(fn, items)");
    }

    #[test]
    fn test_replace_after_equals() {
        let result = replace_function_calls("x = map(fn, items)", "map", "_map_fn");
        assert_eq!(result, "x = _map_fn(fn, items)");
    }

    #[test]
    fn test_replace_in_list() {
        let result = replace_function_calls("list(map(fn, items))", "map", "_map_fn");
        assert_eq!(result, "list(_map_fn(fn, items))");
    }

    // --- try_fix returns None for unknown errors ---

    #[test]
    fn test_no_fix_for_unknown_error() {
        let code = "x = 1 / 0";
        let fixed = try_fix(code, "ZeroDivisionError", "division by zero", None);
        assert!(fixed.is_none());
    }

    #[test]
    fn test_no_fix_for_attribute_error() {
        let code = "x.foo()";
        let fixed = try_fix(
            code,
            "AttributeError",
            "'int' object has no attribute 'foo'",
            None,
        );
        assert!(fixed.is_none());
    }

    // --- strip_sorted_keyword_args ---

    #[test]
    fn test_strip_sorted_preserves_complex_first_arg() {
        let code = "sorted([x for x in data], key=lambda x: x)";
        let fixed = strip_sorted_keyword_args(code);
        assert!(fixed.is_some());
        assert_eq!(fixed.unwrap(), "sorted([x for x in data])");
    }

    // --- find_matching_paren ---

    #[test]
    fn test_find_matching_paren_simple() {
        assert_eq!(find_matching_paren("(abc)", 0), Some(4));
    }

    #[test]
    fn test_find_matching_paren_nested() {
        assert_eq!(find_matching_paren("(a(b)c)", 0), Some(6));
    }

    #[test]
    fn test_find_matching_paren_with_string() {
        assert_eq!(find_matching_paren("(a, ')')", 0), Some(7));
    }

    #[test]
    fn test_find_matching_paren_with_escaped_quote() {
        // The \' inside the string should not close it
        assert_eq!(find_matching_paren(r"(a, 'it\'s')", 0), Some(11));
    }

    // --- fix_await: async for / async with ---

    #[test]
    fn test_fix_async_for() {
        let code = "async for item in gen():\n    print(item)";
        let fixed = try_fix(code, "SyntaxError", "invalid syntax: async", None);
        assert!(fixed.is_some());
        assert!(fixed.unwrap().contains("for item in gen():"));
    }

    #[test]
    fn test_fix_async_with() {
        let code = "async with open(f) as h:\n    pass";
        let fixed = try_fix(code, "SyntaxError", "invalid syntax: async", None);
        assert!(fixed.is_some());
        assert!(fixed.unwrap().contains("with open(f) as h:"));
    }

    #[test]
    fn test_fix_await_on_name_error() {
        // Monty might treat `await` as an identifier → NameError
        let code = "result = await fetch()";
        let fixed = try_fix(code, "NameError", "name 'await' is not defined", None);
        assert!(fixed.is_some());
        assert!(fixed.unwrap().contains("result = fetch()"));
    }

    // --- fix_name_error_counter ---

    #[test]
    fn test_fix_counter() {
        let code = "counts = Counter(words)";
        let fixed = try_fix(code, "NameError", "name 'Counter' is not defined", None);
        assert!(fixed.is_some());
        let fixed = fixed.unwrap();
        assert!(fixed.contains("def _counter_fn("));
        assert!(fixed.contains("_counter_fn(words)"));
    }

    #[test]
    fn test_fix_counter_no_false_positive() {
        let code = "obj.Counter(x)";
        let fixed = try_fix(code, "NameError", "name 'Counter' is not defined", None);
        // Method call — should not be replaced
        assert!(fixed.is_none());
    }

    // --- fix_name_error_itertools ---

    #[test]
    fn test_fix_itertools_chain() {
        let code = "result = itertools.chain(a, b)";
        let fixed = try_fix(code, "NameError", "name 'itertools' is not defined", None);
        assert!(fixed.is_some());
        let fixed = fixed.unwrap();
        assert!(fixed.contains("list(a) + list(b)"));
        assert!(!fixed.contains("itertools"));
    }

    #[test]
    fn test_fix_itertools_islice() {
        let code = "result = itertools.islice(gen, 5)";
        let fixed = try_fix(code, "NameError", "name 'itertools' is not defined", None);
        assert!(fixed.is_some());
        let fixed = fixed.unwrap();
        assert!(fixed.contains("list(gen)[:5]"));
    }

    #[test]
    fn test_fix_itertools_other() {
        // Unknown itertools function — strip prefix as fallback
        let code = "result = itertools.product(a, b)";
        let fixed = try_fix(code, "NameError", "name 'itertools' is not defined", None);
        assert!(fixed.is_some());
        let fixed = fixed.unwrap();
        assert!(fixed.contains("product(a, b)"));
        assert!(!fixed.contains("itertools."));
    }

    // --- split_top_level_args ---

    #[test]
    fn test_split_args_simple() {
        let parts = split_top_level_args("a, b, c");
        assert_eq!(parts, vec!["a", " b", " c"]);
    }

    #[test]
    fn test_split_args_nested() {
        let parts = split_top_level_args("f(a, b), c");
        assert_eq!(parts, vec!["f(a, b)", " c"]);
    }

    // --- fix_sort_method_kwargs ---

    #[test]
    fn test_fix_sort_method_key() {
        let code = "data.sort(key=lambda x: x[1])";
        let fixed = try_fix(
            code,
            "TypeError",
            "sort() got an unexpected keyword argument 'key'",
            None,
        );
        assert!(fixed.is_some());
        assert_eq!(fixed.unwrap(), "data.sort()");
    }

    #[test]
    fn test_fix_sort_method_key_and_reverse() {
        let code = "items.sort(key=lambda x: x['name'], reverse=True)";
        let fixed = try_fix(
            code,
            "TypeError",
            "sort() got an unexpected keyword argument 'key'",
            None,
        );
        assert!(fixed.is_some());
        assert_eq!(fixed.unwrap(), "items.sort()");
    }

    #[test]
    fn test_fix_sort_method_no_kwargs_unchanged() {
        let code = "items.sort()";
        let fixed = try_fix(
            code,
            "TypeError",
            "sort() got an unexpected keyword argument",
            None,
        );
        assert!(fixed.is_none());
    }

    // --- fix_max_min_kwargs ---

    #[test]
    fn test_fix_max_with_key() {
        let code = "result = max(items, key=lambda x: x[1])";
        let fixed = try_fix(
            code,
            "TypeError",
            "max() got an unexpected keyword argument 'key'",
            None,
        );
        assert!(fixed.is_some());
        assert_eq!(fixed.unwrap(), "result = max(items)");
    }

    #[test]
    fn test_fix_min_with_key() {
        let code = "result = min(scores, key=lambda x: x['value'])";
        let fixed = try_fix(
            code,
            "TypeError",
            "min() got an unexpected keyword argument 'key'",
            None,
        );
        assert!(fixed.is_some());
        assert_eq!(fixed.unwrap(), "result = min(scores)");
    }

    #[test]
    fn test_fix_max_with_default() {
        let code = "result = max(items, default=0)";
        let fixed = try_fix(
            code,
            "TypeError",
            "max() got an unexpected keyword argument 'default'",
            None,
        );
        assert!(fixed.is_some());
        assert_eq!(fixed.unwrap(), "result = max(items)");
    }

    #[test]
    fn test_fix_max_multiple_positional_with_key() {
        // max(a, b, key=fn) → max(a, b)
        let code = "result = max(a, b, key=lambda x: abs(x))";
        let fixed = try_fix(
            code,
            "TypeError",
            "max() got an unexpected keyword argument 'key'",
            None,
        );
        assert!(fixed.is_some());
        assert_eq!(fixed.unwrap(), "result = max(a, b)");
    }

    #[test]
    fn test_fix_min_without_kwargs_unchanged() {
        let code = "result = min(items)";
        let fixed = try_fix(
            code,
            "TypeError",
            "min() got an unexpected keyword argument",
            None,
        );
        assert!(fixed.is_none());
    }

    // --- fix_name_error_chain ---

    #[test]
    fn test_fix_bare_chain() {
        let code = "result = list(chain(a, b))";
        let fixed = try_fix(code, "NameError", "name 'chain' is not defined", None);
        assert!(fixed.is_some());
        let fixed = fixed.unwrap();
        assert!(fixed.contains("list(a) + list(b)"));
        assert!(!fixed.contains("chain"));
    }

    #[test]
    fn test_fix_bare_chain_three_args() {
        let code = "all_items = chain(x, y, z)";
        let fixed = try_fix(code, "NameError", "name 'chain' is not defined", None);
        assert!(fixed.is_some());
        let fixed = fixed.unwrap();
        assert!(fixed.contains("list(x) + list(y) + list(z)"));
    }

    #[test]
    fn test_fix_bare_chain_no_false_positive() {
        // blockchain( should NOT be replaced
        let code = "result = blockchain(data)";
        let fixed = try_fix(code, "NameError", "name 'chain' is not defined", None);
        assert!(fixed.is_none());
    }

    // --- fix_name_error_zip_longest ---

    #[test]
    fn test_fix_bare_zip_longest() {
        let code = "result = zip_longest(a, b)";
        let fixed = try_fix(code, "NameError", "name 'zip_longest' is not defined", None);
        assert!(fixed.is_some());
        let fixed = fixed.unwrap();
        assert!(fixed.contains("def _zip_longest_fn("));
        assert!(fixed.contains("_zip_longest_fn(a, b)"));
    }

    #[test]
    fn test_fix_bare_zip_longest_no_false_positive() {
        let code = "my_zip_longest(a, b)";
        let fixed = try_fix(code, "NameError", "name 'zip_longest' is not defined", None);
        assert!(fixed.is_none());
    }

    // --- fix_name_error_ordered_dict ---

    #[test]
    fn test_fix_bare_ordered_dict() {
        let code = "d = OrderedDict()";
        let fixed = try_fix(code, "NameError", "name 'OrderedDict' is not defined", None);
        assert!(fixed.is_some());
        assert_eq!(fixed.unwrap(), "d = dict()");
    }

    #[test]
    fn test_fix_ordered_dict_with_args() {
        let code = "d = OrderedDict([(\"a\", 1), (\"b\", 2)])";
        let fixed = try_fix(code, "NameError", "name 'OrderedDict' is not defined", None);
        assert!(fixed.is_some());
        assert!(fixed.unwrap().contains("dict([(\"a\", 1), (\"b\", 2)])"));
    }

    // --- fix_name_error_deque ---

    #[test]
    fn test_fix_bare_deque() {
        let code = "q = deque([1, 2, 3])";
        let fixed = try_fix(code, "NameError", "name 'deque' is not defined", None);
        assert!(fixed.is_some());
        assert_eq!(fixed.unwrap(), "q = list([1, 2, 3])");
    }

    #[test]
    fn test_fix_deque_empty() {
        let code = "q = deque()";
        let fixed = try_fix(code, "NameError", "name 'deque' is not defined", None);
        assert!(fixed.is_some());
        assert_eq!(fixed.unwrap(), "q = list()");
    }

    // --- fix_name_error_islice ---

    #[test]
    fn test_fix_bare_islice_two_args() {
        let code = "result = islice(gen, 5)";
        let fixed = try_fix(code, "NameError", "name 'islice' is not defined", None);
        assert!(fixed.is_some());
        assert_eq!(fixed.unwrap(), "result = list(gen)[:5]");
    }

    #[test]
    fn test_fix_bare_islice_three_args() {
        let code = "result = islice(gen, 2, 10)";
        let fixed = try_fix(code, "NameError", "name 'islice' is not defined", None);
        assert!(fixed.is_some());
        assert_eq!(fixed.unwrap(), "result = list(gen)[2:10]");
    }

    #[test]
    fn test_fix_bare_islice_no_false_positive() {
        let code = "result = my_islice(data)";
        let fixed = try_fix(code, "NameError", "name 'islice' is not defined", None);
        assert!(fixed.is_none());
    }

    // --- fix_name_error_shell_quote ---

    #[test]
    fn test_fix_shell_quote_missing() {
        let code = "cmd = 'python3 -c ...' + _shell_quote(pattern)\nout = run_command(cmd)";
        let fixed = try_fix(
            code,
            "NameError",
            "name '_shell_quote' is not defined",
            None,
        );
        assert!(fixed.is_some());
        let fixed = fixed.unwrap();
        assert!(fixed.contains("def _shell_quote("));
        assert!(fixed.contains("_shell_quote(pattern)"));
    }

    #[test]
    fn test_fix_shell_quote_already_defined() {
        let code = "def _shell_quote(s):\n    return s\ncmd = _shell_quote(x)";
        let fixed = try_fix(
            code,
            "NameError",
            "name '_shell_quote' is not defined",
            None,
        );
        // Already defined — should not inject again
        assert!(fixed.is_none());
    }

    // --- fix_module_prefix ---

    #[test]
    fn test_fix_functools_prefix() {
        let code = "total = functools.reduce(lambda a, b: a + b, nums)";
        let fixed = try_fix(code, "NameError", "name 'functools' is not defined", None);
        assert!(fixed.is_some());
        let fixed = fixed.unwrap();
        assert!(fixed.contains("reduce(lambda a, b: a + b, nums)"));
        assert!(!fixed.contains("functools."));
    }

    #[test]
    fn test_fix_collections_prefix() {
        let code = "counts = collections.Counter(words)";
        let fixed = try_fix(code, "NameError", "name 'collections' is not defined", None);
        assert!(fixed.is_some());
        let fixed = fixed.unwrap();
        assert!(fixed.contains("Counter(words)"));
        assert!(!fixed.contains("collections."));
    }

    #[test]
    fn test_fix_re_prefix() {
        let code = "matches = re.findall(r'\\d+', text)";
        let fixed = try_fix(code, "NameError", "name 're' is not defined", None);
        assert!(fixed.is_some());
        let fixed = fixed.unwrap();
        assert!(fixed.contains("findall(r'\\d+', text)"));
        assert!(!fixed.contains("re."));
    }

    #[test]
    fn test_fix_json_prefix() {
        let code = "data = json.loads(text)";
        let fixed = try_fix(code, "NameError", "name 'json' is not defined", None);
        assert!(fixed.is_some());
        assert!(fixed.unwrap().contains("loads(text)"));
    }

    #[test]
    fn test_fix_math_prefix() {
        let code = "x = math.sqrt(16)";
        let fixed = try_fix(code, "NameError", "name 'math' is not defined", None);
        assert!(fixed.is_some());
        assert!(fixed.unwrap().contains("sqrt(16)"));
    }

    #[test]
    fn test_fix_datetime_prefix() {
        let code = "now = datetime.datetime.now()";
        let fixed = try_fix(code, "NameError", "name 'datetime' is not defined", None);
        assert!(fixed.is_some());
        // After stripping "datetime.", becomes "now = datetime.now()"
        // (second occurrence gets stripped too → "now = now()" — but auto_fixer
        // handles the rewrite on the next retry cycle)
        assert!(!fixed.unwrap().contains("datetime."));
    }

    #[test]
    fn test_fix_unknown_module_unchanged() {
        // Unknown modules should NOT be stripped (avoid breaking valid code)
        let code = "result = numpy.array([1, 2, 3])";
        let fixed = try_fix(code, "NameError", "name 'numpy' is not defined", None);
        assert!(fixed.is_none());
    }

    // --- fix_name_error_open ---

    #[test]
    fn test_fix_open_write_with_block() {
        let code = "\
content = \"hello world\"
with open(\"/tmp/test.txt\", \"w\", encoding=\"utf-8\") as f:
    f.write(content)
print(\"done\")";
        let fixed = try_fix(code, "NameError", "name 'open' is not defined", None);
        assert!(fixed.is_some());
        let fixed = fixed.unwrap();
        assert!(fixed.contains("write_file(\"/tmp/test.txt\", content)"));
        assert!(!fixed.contains("with open("));
        assert!(fixed.contains("print(\"done\")"));
    }

    #[test]
    fn test_fix_open_write_multiple_writes() {
        let code = "\
with open(\"/tmp/out.txt\", \"w\") as f:
    f.write(header)
    f.write(body)";
        let fixed = try_fix(code, "NameError", "name 'open' is not defined", None);
        assert!(fixed.is_some());
        let fixed = fixed.unwrap();
        assert!(fixed.contains("write_file(\"/tmp/out.txt\", header + body)"));
    }

    #[test]
    fn test_fix_open_read_with_block() {
        let code = "\
with open(\"/tmp/data.txt\", \"r\") as f:
    data = f.read()
print(data)";
        let fixed = try_fix(code, "NameError", "name 'open' is not defined", None);
        assert!(fixed.is_some());
        let fixed = fixed.unwrap();
        assert!(fixed.contains("data = read_file(\"/tmp/data.txt\")"));
        assert!(!fixed.contains("with open("));
        assert!(fixed.contains("print(data)"));
    }

    #[test]
    fn test_fix_open_read_default_mode() {
        // open(path) without mode defaults to read
        let code = "\
with open(\"/tmp/data.txt\") as f:
    text = f.read()";
        let fixed = try_fix(code, "NameError", "name 'open' is not defined", None);
        assert!(fixed.is_some());
        let fixed = fixed.unwrap();
        assert!(fixed.contains("text = read_file(\"/tmp/data.txt\")"));
    }

    #[test]
    fn test_fix_open_no_match_for_unrelated_error() {
        let code = "with open(\"/tmp/x.txt\", \"w\") as f:\n    f.write(\"hi\")";
        let fixed = try_fix(code, "NameError", "name 'foo' is not defined", None);
        assert!(fixed.is_none());
    }

    #[test]
    fn test_fix_open_indented_block() {
        let code = "\
if True:
    with open(\"/tmp/test.txt\", \"w\") as f:
        f.write(content)
    print(\"saved\")";
        let fixed = try_fix(code, "NameError", "name 'open' is not defined", None);
        assert!(fixed.is_some());
        let fixed = fixed.unwrap();
        assert!(fixed.contains("    write_file(\"/tmp/test.txt\", content)"));
        assert!(fixed.contains("print(\"saved\")"));
    }

    // --- fix_name_error_tool_alias ---

    #[test]
    fn test_fix_write_to_write_file() {
        let code = "content = \"hello\"\nresult = write(\"/tmp/test.txt\", content)";
        let fixed = try_fix(code, "NameError", "name 'write' is not defined", None);
        assert!(fixed.is_some());
        let fixed = fixed.unwrap();
        assert!(fixed.contains("write_file(\"/tmp/test.txt\", content)"));
        assert!(!fixed.contains("\nresult = write("));
    }

    #[test]
    fn test_fix_read_to_read_file() {
        let code = "data = read(\"/tmp/data.txt\")";
        let fixed = try_fix(code, "NameError", "name 'read' is not defined", None);
        assert!(fixed.is_some());
        assert!(fixed.unwrap().contains("read_file(\"/tmp/data.txt\")"));
    }

    #[test]
    fn test_fix_write_does_not_touch_write_file() {
        // Should not match if the code already uses write_file
        let code = "write_file(\"/tmp/test.txt\", content)";
        let fixed = try_fix(code, "NameError", "name 'write' is not defined", None);
        // write( is not present as a standalone call, only write_file(
        assert!(fixed.is_none());
    }

    #[test]
    fn test_fix_write_preserves_f_write() {
        // f.write() is a method call, should not be renamed
        let code = "f.write(content)\nresult = write(\"/tmp/x.txt\", data)";
        let fixed = try_fix(code, "NameError", "name 'write' is not defined", None);
        assert!(fixed.is_some());
        let fixed = fixed.unwrap();
        assert!(fixed.contains("f.write(content)")); // method call preserved
        assert!(fixed.contains("write_file(\"/tmp/x.txt\", data)")); // standalone renamed
    }

    // --- strip_func_keyword_args ---

    #[test]
    fn test_strip_func_kwargs_preserves_complex_arg() {
        let fixed = strip_func_keyword_args("max([x for x in data], key=lambda x: x)", "max");
        assert!(fixed.is_some());
        assert_eq!(fixed.unwrap(), "max([x for x in data])");
    }

    // --- strip_sort_method_keyword_args ---

    #[test]
    fn test_strip_sort_method_preserves_context() {
        let code = "for x in items:\n    x.sort(key=lambda a: a[0])\n    print(x)";
        let fixed = strip_sort_method_keyword_args(code);
        assert!(fixed.is_some());
        let fixed = fixed.unwrap();
        assert!(fixed.contains("x.sort()"));
        assert!(fixed.contains("print(x)"));
    }

    // --- extract_positional_args ---

    #[test]
    fn test_extract_positional_with_default() {
        assert_eq!(extract_positional_args("items, default=0"), "items");
    }

    #[test]
    fn test_extract_positional_multiple_args() {
        assert_eq!(extract_positional_args("a, b, key=lambda x: x"), "a, b");
    }

    #[test]
    fn test_extract_positional_no_kwargs() {
        assert_eq!(extract_positional_args("a, b, c"), "a, b, c");
    }
}
