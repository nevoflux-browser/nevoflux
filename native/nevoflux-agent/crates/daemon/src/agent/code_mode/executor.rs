//! CodeModeExecutor - Monty execution loop with external function routing.
//! Runs auto-fix -> lint -> execute -> retry pipeline.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use monty::{
    ExternalResult, LimitedTracker, MontyObject, MontyRun, PrintWriter, ResourceLimits, RunProgress,
};

use super::auto_fixer::MontyAutoFixer;
use super::linter::MontyLinter;
use super::mechanical_fixer;
use super::repair_prompt::RepairPrompt;

/// Maximum number of retries (rewrite attempts) before giving up.
const MAX_RETRIES: u32 = 2;

/// Result of a Code Mode execution.
#[derive(Debug)]
pub struct CodeModeResult {
    /// Final output from print() statements during execution.
    pub output: String,
    /// Final expression value as JSON (the last expression in the script).
    /// `None` when the script has no trailing expression or ends with a statement.
    pub result: Option<serde_json::Value>,
    /// Tool call results collected during execution.
    pub tool_results: Vec<ToolCallResult>,
    /// Whether execution completed successfully.
    pub success: bool,
    /// Error message if execution failed.
    pub error: Option<String>,
    /// Number of retries used (0 = first attempt succeeded).
    pub retries: u32,
}

impl CodeModeResult {
    /// Create a successful result.
    pub fn success(output: String) -> Self {
        Self {
            output,
            result: None,
            tool_results: Vec::new(),
            success: true,
            error: None,
            retries: 0,
        }
    }

    /// Create a failed result with an error message.
    pub fn fail(error: impl Into<String>) -> Self {
        Self {
            output: String::new(),
            result: None,
            tool_results: Vec::new(),
            success: false,
            error: Some(error.into()),
            retries: 0,
        }
    }

    /// Create a failed result that includes partial output.
    pub fn fail_with_output(output: String, error: impl Into<String>) -> Self {
        Self {
            output,
            result: None,
            tool_results: Vec::new(),
            success: false,
            error: Some(error.into()),
            retries: 0,
        }
    }

    /// Set the final expression result.
    pub fn with_result(mut self, value: serde_json::Value) -> Self {
        self.result = Some(value);
        self
    }

    /// Set the retry count.
    pub fn with_retries(mut self, retries: u32) -> Self {
        self.retries = retries;
        self
    }

    /// Set tool call results.
    pub fn with_tool_results(mut self, tool_results: Vec<ToolCallResult>) -> Self {
        self.tool_results = tool_results;
        self
    }

    /// Format the result as a JSON string matching the design spec:
    /// `{"output": "...", "result": ..., "success": true, "error": null}`
    pub fn to_json_string(&self) -> String {
        serde_json::json!({
            "output": self.output,
            "result": self.result,
            "success": self.success,
            "error": self.error,
        })
        .to_string()
    }
}

/// Default resource limits for Monty execution.
fn default_resource_limits() -> ResourceLimits {
    ResourceLimits {
        max_allocations: Some(100_000),
        max_duration: Some(Duration::from_secs(30)),
        max_memory: Some(64 * 1024 * 1024), // 64MB
        gc_interval: Some(10_000),
        max_recursion_depth: Some(100),
    }
}

/// A tool call made during Python execution.
#[derive(Debug, Clone)]
pub struct ToolCallResult {
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub result: serde_json::Value,
}

/// Convert a `MontyObject` to a `serde_json::Value`.
fn monty_object_to_json(obj: &MontyObject) -> serde_json::Value {
    match obj {
        MontyObject::None => serde_json::Value::Null,
        MontyObject::Bool(b) => serde_json::Value::Bool(*b),
        MontyObject::Int(i) => serde_json::json!(*i),
        MontyObject::BigInt(bi) => {
            // Try to fit into i64 first, otherwise use string representation
            if let Ok(i) = i64::try_from(bi) {
                serde_json::json!(i)
            } else {
                serde_json::Value::String(bi.to_string())
            }
        }
        MontyObject::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        MontyObject::String(s) => serde_json::Value::String(s.clone()),
        MontyObject::List(items) => {
            serde_json::Value::Array(items.iter().map(monty_object_to_json).collect())
        }
        MontyObject::Tuple(items) => {
            serde_json::Value::Array(items.iter().map(monty_object_to_json).collect())
        }
        MontyObject::Dict(pairs) => {
            let mut map = serde_json::Map::new();
            for (k, v) in pairs {
                let key = match k {
                    MontyObject::String(s) => s.clone(),
                    other => other.to_string(),
                };
                map.insert(key, monty_object_to_json(v));
            }
            serde_json::Value::Object(map)
        }
        MontyObject::Bytes(b) => {
            serde_json::Value::Array(b.iter().map(|byte| serde_json::json!(*byte)).collect())
        }
        MontyObject::Set(items) | MontyObject::FrozenSet(items) => {
            serde_json::Value::Array(items.iter().map(monty_object_to_json).collect())
        }
        MontyObject::NamedTuple { values, .. } => {
            serde_json::Value::Array(values.iter().map(monty_object_to_json).collect())
        }
        MontyObject::Path(p) => serde_json::Value::String(p.clone()),
        // For all other variants, use debug/repr formatting
        other => serde_json::Value::String(format!("{other}")),
    }
}

/// Convert a `serde_json::Value` to a `MontyObject`.
fn json_to_monty_object(val: &serde_json::Value) -> MontyObject {
    match val {
        serde_json::Value::Null => MontyObject::None,
        serde_json::Value::Bool(b) => MontyObject::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                MontyObject::Int(i)
            } else if let Some(f) = n.as_f64() {
                MontyObject::Float(f)
            } else {
                // u64 that doesn't fit in i64
                MontyObject::String(n.to_string())
            }
        }
        serde_json::Value::String(s) => MontyObject::String(s.clone()),
        serde_json::Value::Array(arr) => {
            MontyObject::List(arr.iter().map(json_to_monty_object).collect())
        }
        serde_json::Value::Object(map) => {
            let pairs: Vec<(MontyObject, MontyObject)> = map
                .iter()
                .map(|(k, v)| (MontyObject::String(k.clone()), json_to_monty_object(v)))
                .collect();
            MontyObject::dict(pairs)
        }
    }
}

/// Extract collected output from a `PrintWriter::Collect`, returning an empty string for other variants.
fn collect_output(print_writer: PrintWriter<'_>) -> String {
    match print_writer {
        PrintWriter::Collect(s) => s,
        _ => String::new(),
    }
}

/// Core execution engine for Agent Code Mode.
///
/// Orchestrates the four-layer constraint pipeline:
/// 1. Prompt constraint (handled externally by system prompt)
/// 2. `MontyAutoFixer` - mechanical text transforms
/// 3. `MontyLinter` - regex-based violation detection
/// 4. Monty interpreter execution with external function routing
///
/// On lint violations or runtime errors, the executor can request the LLM
/// to rewrite the code, up to `MAX_RETRIES` times.
#[derive(Default)]
pub struct CodeModeExecutor;

impl CodeModeExecutor {
    pub fn new() -> Self {
        Self
    }

    /// Execute Python code through the full pipeline.
    ///
    /// Pipeline: auto-fix -> lint -> monty execute -> retry on error
    ///
    /// # Arguments
    /// * `code` - Raw Python code from LLM
    /// * `external_function_names` - Names of tool functions available to the code
    /// * `tool_executor` - Async callback to execute tool calls via ToolRegistry
    /// * `llm_rewrite` - Async callback to ask LLM to rewrite code given a repair prompt
    pub async fn execute<F, R>(
        &self,
        code: &str,
        external_function_names: &[String],
        tool_executor: F,
        llm_rewrite: R,
    ) -> CodeModeResult
    where
        F: Fn(
                &str,
                serde_json::Value,
            )
                -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>>
            + Send
            + Sync,
        R: Fn(&str) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> + Send + Sync,
    {
        let mut current_code = code.to_string();
        let mut retries: u32 = 0;
        // E: Tool result cache — survives across retries so re-executed code
        // reuses results from previous tool calls with identical arguments.
        let mut tool_cache: std::collections::HashMap<String, serde_json::Value> =
            std::collections::HashMap::new();

        loop {
            // Layer 2: Auto-fix mechanical transforms
            let auto_fixed = MontyAutoFixer::fix(&current_code);
            if auto_fixed != current_code {
                tracing::debug!(
                    "Code Mode: auto_fixer modified code (retry {}), first 300 chars: {:?}",
                    retries,
                    &auto_fixed[..auto_fixed.floor_char_boundary(300)]
                );
            }

            // Layer 3: Lint for unsupported constructs
            let violations = MontyLinter::check(&auto_fixed);
            if !violations.is_empty() {
                if retries >= MAX_RETRIES {
                    let violation_msgs: Vec<String> = violations
                        .iter()
                        .map(|v| format!("Line {}: `{}` - {}", v.line, v.construct, v.suggestion))
                        .collect();
                    return CodeModeResult::fail(format!(
                        "Code has unsupported constructs after {} retries: {}",
                        retries,
                        violation_msgs.join("; ")
                    ))
                    .with_retries(retries);
                }

                let repair_prompt = RepairPrompt::from_violations(&auto_fixed, &violations);
                match llm_rewrite(&repair_prompt).await {
                    Ok(rewritten) => {
                        current_code = rewritten;
                        retries += 1;
                        continue;
                    }
                    Err(e) => {
                        return CodeModeResult::fail(format!(
                            "LLM rewrite failed for lint violations: {e}"
                        ))
                        .with_retries(retries);
                    }
                }
            }

            // Layer 4: Execute with Monty interpreter
            let runner = match MontyRun::new(
                auto_fixed.clone(),
                "code_mode.py",
                vec![],
                external_function_names.to_vec(),
            ) {
                Ok(runner) => runner,
                Err(exc) => {
                    let error_msg = exc.message().unwrap_or("parse error").to_string();
                    let error_type = format!("{}", exc.exc_type());
                    tracing::warn!(
                        "Code Mode: parse error (retry {}/{}): {}: {}, first 200 chars: {:?}",
                        retries,
                        MAX_RETRIES,
                        error_type,
                        error_msg,
                        &auto_fixed[..auto_fixed.floor_char_boundary(200)]
                    );

                    if retries < MAX_RETRIES {
                        let line = exc.traceback().last().map(|f| f.start.line as usize);

                        // B: Try mechanical fix before expensive LLM rewrite
                        if let Some(fixed) =
                            mechanical_fixer::try_fix(&auto_fixed, &error_type, &error_msg, line)
                        {
                            tracing::info!(
                                "Code Mode: mechanical fix applied for parse error: {}: {}",
                                error_type,
                                error_msg
                            );
                            current_code = fixed;
                            retries += 1;
                            continue;
                        }

                        tracing::debug!(
                            "Code Mode: mechanical_fixer returned None for parse error: {}: {} (line={:?})",
                            error_type,
                            error_msg,
                            line
                        );

                        let repair_prompt = RepairPrompt::from_runtime_error(
                            &auto_fixed,
                            &error_type,
                            &error_msg,
                            line,
                            external_function_names,
                        );
                        tracing::info!("Code Mode: requesting LLM rewrite for parse error");
                        match llm_rewrite(&repair_prompt).await {
                            Ok(rewritten) => {
                                tracing::info!(
                                    "Code Mode: LLM rewrite succeeded ({} bytes)",
                                    rewritten.len()
                                );
                                current_code = rewritten;
                                retries += 1;
                                continue;
                            }
                            Err(e) => {
                                tracing::error!("Code Mode: LLM rewrite failed: {}", e);
                                return CodeModeResult::fail(format!(
                                    "Parse error and LLM rewrite failed: {error_type}: {error_msg} (rewrite error: {e})"
                                ))
                                .with_retries(retries);
                            }
                        }
                    }

                    return CodeModeResult::fail(format!("{error_type}: {error_msg}"))
                        .with_retries(retries);
                }
            };

            let resource_tracker = LimitedTracker::new(default_resource_limits());
            let mut print_writer = PrintWriter::Collect(String::new());
            let mut tool_results: Vec<ToolCallResult> = Vec::new();
            // Track resolved external call results keyed by call_id for ResolveFutures.
            // When a FunctionCall is processed synchronously via state.run(), we also
            // store the result here so that if ResolveFutures fires (e.g. from
            // asyncio.gather edge cases), we can provide the already-computed values.
            let mut pending_results: Vec<(u32, MontyObject)> = Vec::new();

            let start_result = runner.start(vec![], resource_tracker, &mut print_writer);

            let mut progress = match start_result {
                Ok(p) => p,
                Err(exc) => {
                    let error_msg = exc.message().unwrap_or("runtime error").to_string();
                    let error_type = format!("{}", exc.exc_type());
                    tracing::warn!(
                        "Code Mode: start error (retry {}/{}): {}: {}, first 200 chars: {:?}",
                        retries,
                        MAX_RETRIES,
                        error_type,
                        error_msg,
                        &auto_fixed[..auto_fixed.floor_char_boundary(200)]
                    );

                    if retries < MAX_RETRIES {
                        let line = exc.traceback().last().map(|f| f.start.line as usize);

                        // B: Try mechanical fix first
                        if let Some(fixed) =
                            mechanical_fixer::try_fix(&auto_fixed, &error_type, &error_msg, line)
                        {
                            tracing::info!(
                                "Code Mode: mechanical fix applied for start error: {}: {}",
                                error_type,
                                error_msg
                            );
                            current_code = fixed;
                            retries += 1;
                            continue;
                        }

                        tracing::debug!(
                            "Code Mode: mechanical_fixer returned None for start error: {}: {} (line={:?})",
                            error_type,
                            error_msg,
                            line
                        );

                        let repair_prompt = RepairPrompt::from_runtime_error(
                            &auto_fixed,
                            &error_type,
                            &error_msg,
                            line,
                            external_function_names,
                        );
                        tracing::info!("Code Mode: requesting LLM rewrite for start error");
                        match llm_rewrite(&repair_prompt).await {
                            Ok(rewritten) => {
                                tracing::info!(
                                    "Code Mode: LLM rewrite succeeded for start error ({} bytes)",
                                    rewritten.len()
                                );
                                current_code = rewritten;
                                retries += 1;
                                continue;
                            }
                            Err(e) => {
                                tracing::error!(
                                    "Code Mode: LLM rewrite failed for start error: {}",
                                    e
                                );
                                return CodeModeResult::fail_with_output(
                                    collect_output(print_writer),
                                    format!("Runtime error and LLM rewrite failed: {error_type}: {error_msg} (rewrite error: {e})"),
                                )
                                .with_tool_results(tool_results)
                                .with_retries(retries);
                            }
                        }
                    }

                    return CodeModeResult::fail_with_output(
                        collect_output(print_writer),
                        format!("{error_type}: {error_msg}"),
                    )
                    .with_tool_results(tool_results)
                    .with_retries(retries);
                }
            };

            // Execution loop: handle function calls until completion
            loop {
                match progress {
                    RunProgress::FunctionCall {
                        function_name,
                        args,
                        kwargs,
                        call_id,
                        method_call: _,
                        state,
                    } => {
                        // Build arguments JSON, merging positional args and kwargs.
                        // When kwargs are present, use a special envelope so
                        // positional_to_named_auto can map positional args by
                        // index (using param names) and merge kwargs on top.
                        let arguments = if kwargs.is_empty() {
                            let json_args: Vec<serde_json::Value> =
                                args.iter().map(monty_object_to_json).collect();
                            serde_json::Value::Array(json_args)
                        } else {
                            let positional: Vec<serde_json::Value> =
                                args.iter().map(monty_object_to_json).collect();
                            let mut kw_obj = serde_json::Map::new();
                            for (key, val) in &kwargs {
                                let key_str = match monty_object_to_json(key) {
                                    serde_json::Value::String(s) => s,
                                    other => other.to_string(),
                                };
                                kw_obj.insert(key_str, monty_object_to_json(val));
                            }
                            serde_json::json!({
                                "__positional": positional,
                                "__kwargs": kw_obj
                            })
                        };

                        // E: Check tool cache before executing
                        let cache_key = format!(
                            "{}:{}",
                            function_name,
                            serde_json::to_string(&arguments).unwrap_or_default()
                        );

                        let (result_json, resume_value): (serde_json::Value, ExternalResult) =
                            if let Some(cached) = tool_cache.get(&cache_key) {
                                tracing::debug!(
                                    "Code Mode: tool cache hit for {} (key len={})",
                                    function_name,
                                    cache_key.len()
                                );
                                let return_obj = json_to_monty_object(cached);
                                (cached.clone(), ExternalResult::Return(return_obj))
                            } else {
                                // Execute the tool
                                let tool_result =
                                    tool_executor(&function_name, arguments.clone()).await;

                                let (rj, rv) = match tool_result {
                                    Ok(result_val) => {
                                        let return_obj = json_to_monty_object(&result_val);
                                        (result_val, ExternalResult::Return(return_obj))
                                    }
                                    Err(e) => {
                                        // Return error as a dict rather than
                                        // ExternalResult::Error, because Monty may not
                                        // properly raise Python exceptions from
                                        // ExternalResult::Error.
                                        let error_val = serde_json::json!({
                                            "__tool_error": true,
                                            "error": format!("{function_name}: {e}"),
                                        });
                                        let return_obj = json_to_monty_object(&error_val);
                                        (error_val, ExternalResult::Return(return_obj))
                                    }
                                };
                                // Cache successful results (skip error dicts)
                                if !rj
                                    .as_object()
                                    .is_some_and(|o| o.contains_key("__tool_error"))
                                {
                                    tool_cache.insert(cache_key, rj.clone());
                                }
                                (rj, rv)
                            };

                        tool_results.push(ToolCallResult {
                            tool_name: function_name,
                            arguments,
                            result: result_json,
                        });

                        let pending_obj = match &resume_value {
                            ExternalResult::Return(obj) => obj.clone(),
                            _ => MontyObject::None,
                        };
                        pending_results.push((call_id, pending_obj));

                        // Resume execution with the result
                        match state.run(resume_value, &mut print_writer) {
                            Ok(next) => {
                                progress = next;
                            }
                            Err(exc) => {
                                let error_msg =
                                    exc.message().unwrap_or("runtime error").to_string();
                                let error_type = format!("{}", exc.exc_type());
                                tracing::warn!(
                                    "Code Mode: post-tool-call error (retry {}/{}): {}: {}, first 200 chars: {:?}",
                                    retries,
                                    MAX_RETRIES,
                                    error_type,
                                    error_msg,
                                    &auto_fixed[..auto_fixed.floor_char_boundary(200)]
                                );

                                if retries < MAX_RETRIES {
                                    let line =
                                        exc.traceback().last().map(|f| f.start.line as usize);

                                    // B: Try mechanical fix first
                                    if let Some(fixed) = mechanical_fixer::try_fix(
                                        &auto_fixed,
                                        &error_type,
                                        &error_msg,
                                        line,
                                    ) {
                                        tracing::info!(
                                            "Code Mode: mechanical fix applied after tool call: {}: {}",
                                            error_type,
                                            error_msg
                                        );
                                        current_code = fixed;
                                        retries += 1;
                                        break;
                                    }

                                    tracing::debug!(
                                        "Code Mode: mechanical_fixer returned None for post-tool-call error: {}: {} (line={:?})",
                                        error_type,
                                        error_msg,
                                        line
                                    );

                                    let repair_prompt = RepairPrompt::from_runtime_error(
                                        &auto_fixed,
                                        &error_type,
                                        &error_msg,
                                        line,
                                        external_function_names,
                                    );
                                    tracing::info!("Code Mode: requesting LLM rewrite for post-tool-call error");
                                    match llm_rewrite(&repair_prompt).await {
                                        Ok(rewritten) => {
                                            tracing::info!(
                                                "Code Mode: LLM rewrite succeeded for post-tool-call error ({} bytes)",
                                                rewritten.len()
                                            );
                                            current_code = rewritten;
                                            retries += 1;
                                            break; // break inner loop to restart outer loop
                                        }
                                        Err(e) => {
                                            tracing::error!("Code Mode: LLM rewrite failed for post-tool-call error: {}", e);
                                            return CodeModeResult::fail_with_output(
                                                collect_output(print_writer),
                                                format!("Runtime error after tool call and LLM rewrite failed: {error_type}: {error_msg} (rewrite error: {e})"),
                                            )
                                            .with_tool_results(tool_results)
                                            .with_retries(retries);
                                        }
                                    }
                                }

                                return CodeModeResult::fail_with_output(
                                    collect_output(print_writer),
                                    format!("{error_type}: {error_msg}"),
                                )
                                .with_tool_results(tool_results)
                                .with_retries(retries);
                            }
                        }
                    }
                    RunProgress::Complete(value) => {
                        let final_value = monty_object_to_json(&value);
                        let mut result = CodeModeResult::success(collect_output(print_writer))
                            .with_tool_results(tool_results)
                            .with_retries(retries);
                        // Capture non-None final expressions as the result
                        if !final_value.is_null() {
                            result = result.with_result(final_value);
                        }
                        return result;
                    }
                    RunProgress::OsCall { .. } => {
                        return CodeModeResult::fail_with_output(
                            collect_output(print_writer),
                            "OS calls are not permitted in sandboxed execution",
                        )
                        .with_tool_results(tool_results)
                        .with_retries(retries);
                    }
                    RunProgress::ResolveFutures(future_state) => {
                        // Sequential dispatch: resolve all pending futures using
                        // results that were already computed during FunctionCall
                        // handling.  For any call_id without a stored result, we
                        // fall back to MontyObject::None.
                        let results: Vec<(u32, ExternalResult)> = future_state
                            .pending_call_ids()
                            .iter()
                            .map(|&cid| {
                                let value = pending_results
                                    .iter()
                                    .find(|(id, _)| *id == cid)
                                    .map(|(_, v)| v.clone())
                                    .unwrap_or_else(|| {
                                        tracing::warn!(
                                            "Code Mode: ResolveFutures has unknown call_id {cid}, \
                                             resolving with None"
                                        );
                                        MontyObject::None
                                    });
                                (cid, ExternalResult::Return(value))
                            })
                            .collect();

                        tracing::debug!(
                            "Code Mode: resolving {} pending futures sequentially",
                            results.len()
                        );

                        match future_state.resume(results, &mut print_writer) {
                            Ok(next) => {
                                progress = next;
                            }
                            Err(exc) => {
                                let error_msg =
                                    exc.message().unwrap_or("runtime error").to_string();
                                let error_type = format!("{}", exc.exc_type());
                                tracing::warn!(
                                    "Code Mode: post-futures error (retry {}/{}): {}: {}, first 200 chars: {:?}",
                                    retries,
                                    MAX_RETRIES,
                                    error_type,
                                    error_msg,
                                    &auto_fixed[..auto_fixed.floor_char_boundary(200)]
                                );

                                if retries < MAX_RETRIES {
                                    let line =
                                        exc.traceback().last().map(|f| f.start.line as usize);

                                    // B: Try mechanical fix first
                                    if let Some(fixed) = mechanical_fixer::try_fix(
                                        &auto_fixed,
                                        &error_type,
                                        &error_msg,
                                        line,
                                    ) {
                                        tracing::info!(
                                            "Code Mode: mechanical fix applied after futures: {}: {}",
                                            error_type,
                                            error_msg
                                        );
                                        current_code = fixed;
                                        retries += 1;
                                        break;
                                    }

                                    tracing::debug!(
                                        "Code Mode: mechanical_fixer returned None for post-futures error: {}: {} (line={:?})",
                                        error_type,
                                        error_msg,
                                        line
                                    );

                                    let repair_prompt = RepairPrompt::from_runtime_error(
                                        &auto_fixed,
                                        &error_type,
                                        &error_msg,
                                        line,
                                        external_function_names,
                                    );
                                    tracing::info!(
                                        "Code Mode: requesting LLM rewrite for post-futures error"
                                    );
                                    match llm_rewrite(&repair_prompt).await {
                                        Ok(rewritten) => {
                                            tracing::info!(
                                                "Code Mode: LLM rewrite succeeded for post-futures error ({} bytes)",
                                                rewritten.len()
                                            );
                                            current_code = rewritten;
                                            retries += 1;
                                            break; // break inner loop to restart outer
                                        }
                                        Err(e) => {
                                            tracing::error!("Code Mode: LLM rewrite failed for post-futures error: {}", e);
                                            return CodeModeResult::fail_with_output(
                                                collect_output(print_writer),
                                                format!(
                                                    "Runtime error resolving futures and LLM \
                                                     rewrite failed: {error_type}: {error_msg} \
                                                     (rewrite error: {e})"
                                                ),
                                            )
                                            .with_tool_results(tool_results)
                                            .with_retries(retries);
                                        }
                                    }
                                }

                                return CodeModeResult::fail_with_output(
                                    collect_output(print_writer),
                                    format!("{error_type}: {error_msg}"),
                                )
                                .with_tool_results(tool_results)
                                .with_retries(retries);
                            }
                        }
                    }
                }
            }
            // If we broke out of the inner loop (retry after tool call error),
            // continue the outer loop.
        }
    }
}

use crate::agent::tools::ToolRegistry;
use crate::wasm::services::BrowserContext;
use std::collections::HashMap;
use std::sync::Arc;

/// Create a shared ToolRegistry and tool executor callback for `execute_python_simple`.
///
/// `param_mappings` maps tool names to ordered parameter name lists, used to
/// convert positional args (JSON arrays from Monty) to named args (JSON objects
/// for ToolRegistry).  When a tool is not present in the map, extra positional
/// arguments are assigned generic names (`arg0`, `arg1`, ...).
///
/// `tools_config` optionally restricts which tools can be executed at runtime.
/// When set, the executor guard checks the allowlist before dispatching.
fn build_registry_and_executor(
    browser_ctx: Option<BrowserContext>,
    param_mappings: HashMap<String, Vec<String>>,
    tools_config: Option<nevoflux_protocol::subagent::ToolsConfig>,
) -> (
    Vec<String>,
    impl Fn(
        &str,
        serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>>,
) {
    let registry = match browser_ctx {
        Some(ctx) => ToolRegistry::with_browser(ctx),
        None => ToolRegistry::new(),
    };

    // Use caller-provided param_mappings, or auto-generate from registry hints.
    let effective_mappings = if param_mappings.is_empty() {
        registry.param_mappings()
    } else {
        param_mappings
    };

    let shared_registry = Arc::new(registry);
    let external_names: Vec<String> = shared_registry
        .tool_names()
        .iter()
        .map(|s| s.to_string())
        .collect();

    let param_cache = Arc::new(effective_mappings);
    let tools_config = Arc::new(tools_config);

    let tool_executor = move |name: &str, args: serde_json::Value| {
        let name = name.to_string();
        let args = args.clone();
        let registry = shared_registry.clone();
        let mappings = param_cache.clone();
        let tools_config = tools_config.clone();
        let fut: Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>> =
            Box::pin(async move {
                // Executor guard: check tool allowlist if configured
                match tools_config.as_ref() {
                    Some(nevoflux_protocol::subagent::ToolsConfig::None) => {
                        return Err(format!(
                            "Tool '{}' is not available: all tools are disabled",
                            name
                        ));
                    }
                    Some(nevoflux_protocol::subagent::ToolsConfig::Allow(ref allowlist)) => {
                        if !nevoflux_protocol::subagent::is_tool_allowed(allowlist, &name) {
                            return Err(format!(
                                "Tool '{}' is not available: not in the allowed tool list",
                                name
                            ));
                        }
                    }
                    None => {} // inherit: allow all
                }

                let param_names = mappings.get(&name).cloned().unwrap_or_default();
                let named_args = positional_to_named_auto(&param_names, &args);
                let call = crate::agent::abi::PendingToolCall {
                    id: format!("code-mode-{}", uuid_simple()),
                    name: name.clone(),
                    arguments: named_args,
                };
                let result = registry.execute(&call).await;

                // Auto-inject wait_for_stable after navigation actions so that
                // SPA pages have time to render before the next tool call reads
                // page content. This mirrors the Agent-mode behaviour in
                // auto_snapshot_after_action().
                if matches!(
                    name.as_str(),
                    "browser_navigate" | "browser_go_back" | "browser_go_forward"
                ) {
                    let wait_call = crate::agent::abi::PendingToolCall {
                        id: format!("code-mode-wait-{}", uuid_simple()),
                        name: "browser_wait_for_stable".to_string(),
                        arguments: serde_json::json!({
                            "strategy": "navigation",
                            "max_wait": 3000
                        }),
                    };
                    // Best-effort: ignore wait errors (page may already be stable)
                    let _ = registry.execute(&wait_call).await;
                }

                if let Some(error) = result.error {
                    Err(error)
                } else {
                    let content = result.content.unwrap_or_default();
                    match serde_json::from_str::<serde_json::Value>(&content) {
                        Ok(val) => Ok(val),
                        Err(_) => Ok(serde_json::Value::String(content)),
                    }
                }
            });
        fut
    };

    (external_names, tool_executor)
}

/// Execute Python code through Monty with optional tool support.
///
/// When `browser_ctx` is provided, browser and web tools are available.
/// This is the entry point for the `orchestrate` tool call.
///
/// Delegates to `CodeModeExecutor::execute()` with a no-op LLM rewrite callback.
pub fn execute_python_simple(code: &str, browser_ctx: Option<BrowserContext>) -> CodeModeResult {
    let runtime = tokio::runtime::Handle::current();
    // Build param mappings from registry tool definitions.
    // Empty mappings = positional args use generic names (arg0, arg1, ...).
    // When SignatureCache is wired in from the caller, real mappings will be provided.
    let (external_names, tool_executor) =
        build_registry_and_executor(browser_ctx, HashMap::new(), None);
    let executor = CodeModeExecutor::new();

    let llm_rewrite =
        |_prompt: &str| -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
            Box::pin(async { Err("No LLM retry in orchestrate tool mode".to_string()) })
        };

    tokio::task::block_in_place(|| {
        runtime.block_on(async {
            executor
                .execute(code, &external_names, tool_executor, llm_rewrite)
                .await
        })
    })
}

/// Execute Python code with LLM-powered error recovery.
///
/// Like [`execute_python_simple`], but uses a real LLM call to rewrite code
/// when the linter finds violations or the runtime encounters errors.
/// The LLM receives a repair prompt and returns corrected Python code.
///
/// # Arguments
/// * `code` - Raw Python code from the LLM
/// * `browser_ctx` - Optional browser context for browser/web tool access
/// * `provider` - LLM provider type (Anthropic, OpenAI, etc.)
/// * `api_key` - API key for the provider
/// * `model` - Model name to use for the rewrite call
pub fn execute_python_with_llm(
    code: &str,
    browser_ctx: Option<BrowserContext>,
    provider: nevoflux_llm::ProviderType,
    api_key: String,
    model: String,
    base_url: Option<String>,
) -> CodeModeResult {
    let runtime = tokio::runtime::Handle::current();
    let (external_names, tool_executor) =
        build_registry_and_executor(browser_ctx, HashMap::new(), None);
    let executor = CodeModeExecutor::new();

    let llm_rewrite =
        move |prompt: &str| -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
            let prompt = prompt.to_string();
            let api_key = api_key.clone();
            let model = model.clone();
            let base_url = base_url.clone();
            Box::pin(async move {
                let request = crate::wasm::llm::LlmChatRequest {
                    messages: vec![crate::wasm::llm::LlmMessage::user(&prompt)],
                    system: Some(
                        "You are a Python code repair assistant. \
                     Fix the code according to the error description. \
                     Return ONLY the corrected Python code inside a ```python fence. \
                     Do not include any explanation outside the fence."
                            .to_string(),
                    ),
                    temperature: Some(0.0),
                    max_tokens: Some(4096),
                    tools: None,
                };

                let response = crate::wasm::llm::execute_llm_chat(
                    provider,
                    &api_key,
                    &model,
                    request,
                    base_url.as_deref(),
                )
                .await
                .map_err(|e| format!("LLM rewrite call failed: {e}"))?;

                // Extract Python code from the response (handles ```python, ```py, ``` fences)
                let text = response.content;
                if let Some(code) = crate::agent::runner::extract_any_python_block(&text) {
                    Ok(code)
                } else {
                    // If no fence found, use the raw response as code
                    // (the LLM may have returned bare code without fences)
                    Ok(text)
                }
            })
        };

    tokio::task::block_in_place(|| {
        runtime.block_on(async {
            executor
                .execute(code, &external_names, tool_executor, llm_rewrite)
                .await
        })
    })
}

/// Convert positional args to named args using auto-generated parameter mapping.
/// If args is already a plain object, pass through unchanged.
///
/// Also handles the `{"__positional": [...], "__kwargs": {...}}` envelope
/// produced when Monty delivers both positional and keyword arguments:
/// positional args are mapped by index using `param_names`, then kwargs
/// are merged on top (kwargs override positional).
fn positional_to_named_auto(param_names: &[String], args: &serde_json::Value) -> serde_json::Value {
    // Special envelope: positional + kwargs from Monty FunctionCall
    if let Some(obj) = args.as_object() {
        if obj.contains_key("__positional") || obj.contains_key("__kwargs") {
            let mut result = serde_json::Map::new();
            // Map positional args by index
            if let Some(positional) = obj.get("__positional").and_then(|v| v.as_array()) {
                for (i, val) in positional.iter().enumerate() {
                    let key = if i < param_names.len() {
                        param_names[i].clone()
                    } else {
                        format!("arg{}", i)
                    };
                    result.insert(key, val.clone());
                }
            }
            // Merge kwargs (override positional)
            if let Some(kwargs) = obj.get("__kwargs").and_then(|v| v.as_object()) {
                for (k, v) in kwargs {
                    result.insert(k.clone(), v.clone());
                }
            }
            return serde_json::Value::Object(result);
        }
        // Plain object — pass through
        return args.clone();
    }
    // Array — map by index
    let arr = match args.as_array() {
        Some(a) => a,
        None => return serde_json::json!({}),
    };
    let mut obj = serde_json::Map::new();
    for (i, val) in arr.iter().enumerate() {
        let key = if i < param_names.len() {
            param_names[i].clone()
        } else {
            format!("arg{}", i)
        };
        obj.insert(key, val.clone());
    }
    serde_json::Value::Object(obj)
}

/// Generate a simple unique ID (timestamp + nanos).
fn uuid_simple() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}-{:x}", now.as_millis(), now.subsec_nanos())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monty_object_to_json() {
        assert_eq!(
            monty_object_to_json(&MontyObject::None),
            serde_json::Value::Null
        );
        assert_eq!(
            monty_object_to_json(&MontyObject::Int(42)),
            serde_json::json!(42)
        );
        assert_eq!(
            monty_object_to_json(&MontyObject::Bool(true)),
            serde_json::json!(true)
        );
        assert_eq!(
            monty_object_to_json(&MontyObject::String("hello".to_string())),
            serde_json::json!("hello")
        );
        assert_eq!(
            monty_object_to_json(&MontyObject::Float(3.14)),
            serde_json::json!(3.14)
        );
        assert_eq!(
            monty_object_to_json(&MontyObject::List(vec![
                MontyObject::Int(1),
                MontyObject::Int(2),
            ])),
            serde_json::json!([1, 2])
        );
    }

    #[test]
    fn test_json_to_monty_object() {
        assert!(matches!(
            json_to_monty_object(&serde_json::json!(null)),
            MontyObject::None
        ));
        assert!(matches!(
            json_to_monty_object(&serde_json::json!(42)),
            MontyObject::Int(42)
        ));
        assert!(matches!(
            json_to_monty_object(&serde_json::json!(true)),
            MontyObject::Bool(true)
        ));
        match json_to_monty_object(&serde_json::json!("hello")) {
            MontyObject::String(s) => assert_eq!(s, "hello"),
            other => panic!("Expected String, got {:?}", other),
        }
        match json_to_monty_object(&serde_json::json!(3.14)) {
            MontyObject::Float(f) => assert!((f - 3.14).abs() < f64::EPSILON),
            other => panic!("Expected Float, got {:?}", other),
        }
    }

    #[test]
    fn test_json_to_monty_object_dict() {
        let json = serde_json::json!({"name": "test", "value": 42});
        let obj = json_to_monty_object(&json);
        // Should convert to Dict, not String
        match &obj {
            MontyObject::Dict(_) => {}
            other => panic!("Expected Dict, got {:?}", other),
        }
        // Round-trip: Dict → JSON → verify keys
        let back = monty_object_to_json(&obj);
        assert_eq!(back.get("name").unwrap(), "test");
        assert_eq!(back.get("value").unwrap(), 42);
    }

    #[test]
    fn test_json_to_monty_object_nested_dict() {
        let json = serde_json::json!([{"id": "e1", "tag": "a"}, {"id": "e2", "tag": "div"}]);
        let obj = json_to_monty_object(&json);
        // Should be a list of dicts
        match &obj {
            MontyObject::List(items) => {
                assert_eq!(items.len(), 2);
                match &items[0] {
                    MontyObject::Dict(_) => {}
                    other => panic!("Expected Dict element, got {:?}", other),
                }
            }
            other => panic!("Expected List, got {:?}", other),
        }
    }

    #[test]
    fn test_monty_object_to_json_tuple() {
        let tuple = MontyObject::Tuple(vec![MontyObject::Int(1), MontyObject::String("a".into())]);
        assert_eq!(monty_object_to_json(&tuple), serde_json::json!([1, "a"]));
    }

    #[test]
    fn test_monty_object_to_json_nan() {
        // NaN cannot be represented in JSON, should map to null
        assert_eq!(
            monty_object_to_json(&MontyObject::Float(f64::NAN)),
            serde_json::Value::Null
        );
    }

    #[test]
    fn test_auto_fix_applied() {
        // Verify that auto-fixer runs: code with `import os` should have it stripped
        let code = "import os\nx = 1 + 2\nprint(x)";
        let fixed = MontyAutoFixer::fix(code);
        assert!(!fixed.contains("import os"));
        assert!(fixed.contains("x = 1 + 2"));
    }

    #[tokio::test]
    async fn test_simple_execution() {
        let executor = CodeModeExecutor::new();
        let result = executor
            .execute(
                "x = 1 + 2\nprint(x)",
                &[],
                |_name, _args| Box::pin(async { Ok(serde_json::json!("ok")) }),
                |_prompt| Box::pin(async { Err("no rewrite".to_string()) }),
            )
            .await;
        assert!(
            result.success,
            "Expected success, got error: {:?}",
            result.error
        );
        assert!(
            result.output.contains('3'),
            "Expected output to contain '3', got: {:?}",
            result.output
        );
        assert_eq!(result.retries, 0);
    }

    #[tokio::test]
    async fn test_lint_violation_triggers_rewrite() {
        let executor = CodeModeExecutor::new();
        let result = executor
            .execute(
                "class Foo:\n    pass",
                &[],
                |_name, _args| Box::pin(async { Ok(serde_json::json!("ok")) }),
                |_prompt| {
                    Box::pin(async {
                        // Return valid code without class
                        Ok("x = {\"type\": \"Foo\"}\nprint(x)".to_string())
                    })
                },
            )
            .await;
        assert!(
            result.success,
            "Expected success, got error: {:?}",
            result.error
        );
        assert!(result.retries >= 1);
    }

    #[tokio::test]
    async fn test_external_function_call() {
        let executor = CodeModeExecutor::new();
        let result = executor
            .execute(
                "result = web_search(\"test\")\nprint(result)",
                &["web_search".to_string()],
                |name, _args| {
                    let name = name.to_string();
                    Box::pin(async move {
                        assert_eq!(name, "web_search");
                        Ok(serde_json::json!("search results"))
                    })
                },
                |_prompt| Box::pin(async { Err("no rewrite".to_string()) }),
            )
            .await;
        assert!(
            result.success,
            "Expected success, got error: {:?}",
            result.error
        );
        assert_eq!(result.tool_results.len(), 1);
        assert_eq!(result.tool_results[0].tool_name, "web_search");
    }

    #[tokio::test]
    async fn test_max_retries_exceeded() {
        let executor = CodeModeExecutor::new();
        let result = executor
            .execute(
                "class Foo:\n    pass",
                &[],
                |_name, _args| Box::pin(async { Ok(serde_json::json!("ok")) }),
                |_prompt| {
                    Box::pin(async {
                        // Always return code with class (never fixes it)
                        Ok("class Bar:\n    pass".to_string())
                    })
                },
            )
            .await;
        assert!(!result.success);
        assert!(result.error.is_some());
        assert_eq!(result.retries, MAX_RETRIES);
    }

    #[tokio::test]
    async fn test_import_auto_stripped_before_execution() {
        let executor = CodeModeExecutor::new();
        let result = executor
            .execute(
                "import os\nimport sys\nx = 10\nprint(x)",
                &[],
                |_name, _args| Box::pin(async { Ok(serde_json::json!("ok")) }),
                |_prompt| Box::pin(async { Err("no rewrite".to_string()) }),
            )
            .await;
        assert!(
            result.success,
            "Expected success, got error: {:?}",
            result.error
        );
        assert!(result.output.contains("10"));
        assert_eq!(result.retries, 0);
    }

    #[tokio::test]
    async fn test_multiple_tool_calls() {
        let executor = CodeModeExecutor::new();
        let result = executor
            .execute(
                "a = tool_a(\"x\")\nb = tool_b(\"y\")\nprint(a, b)",
                &["tool_a".to_string(), "tool_b".to_string()],
                |name, _args| {
                    let name = name.to_string();
                    Box::pin(async move { Ok(serde_json::json!(format!("result_{}", name))) })
                },
                |_prompt| Box::pin(async { Err("no rewrite".to_string()) }),
            )
            .await;
        assert!(
            result.success,
            "Expected success, got error: {:?}",
            result.error
        );
        assert_eq!(result.tool_results.len(), 2);
        assert_eq!(result.tool_results[0].tool_name, "tool_a");
        assert_eq!(result.tool_results[1].tool_name, "tool_b");
    }

    #[tokio::test]
    async fn test_empty_code() {
        let executor = CodeModeExecutor::new();
        let result = executor
            .execute(
                "",
                &[],
                |_name, _args| Box::pin(async { Ok(serde_json::json!("ok")) }),
                |_prompt| Box::pin(async { Err("no rewrite".to_string()) }),
            )
            .await;
        // Empty code should either succeed with no output or fail gracefully
        // (depends on Monty behavior with empty input)
        // Just verify it doesn't panic
        assert!(result.output.is_empty() || result.success || result.error.is_some());
    }

    #[tokio::test]
    async fn test_async_external_call() {
        // Verifies that calling an external function in non-async context still works
        // (goes through FunctionCall path, ResolveFutures should not fire).
        let executor = CodeModeExecutor::new();
        let result = executor
            .execute(
                "result = fetch(\"https://example.com\")\nprint(result)",
                &["fetch".to_string()],
                |_name, _args| Box::pin(async { Ok(serde_json::json!("page content")) }),
                |_prompt| Box::pin(async { Err("no rewrite".to_string()) }),
            )
            .await;
        assert!(
            result.success,
            "Expected success, got error: {:?}",
            result.error
        );
        assert_eq!(result.tool_results.len(), 1);
        assert_eq!(result.tool_results[0].tool_name, "fetch");
        assert!(
            result.output.contains("page content"),
            "Expected output to contain 'page content', got: {:?}",
            result.output
        );
    }

    #[tokio::test]
    async fn test_async_gather_rewritten_to_sequential() {
        // Verify that async gather code triggers lint → rewrite → sequential execution.
        // The LLM rewrite callback converts async code into sequential calls.
        use std::sync::{Arc, Mutex};

        let call_order = Arc::new(Mutex::new(Vec::new()));
        let call_order_clone = call_order.clone();

        let executor = CodeModeExecutor::new();
        let result = executor
            .execute(
                // Initial code uses async (triggers lint violation)
                "import asyncio\n\
                 async def main():\n\
                     a, b, c = await asyncio.gather(api_a(), api_b(), api_c())\n\
                     return [a, b, c]\n\
                 results = await main()\n\
                 print(results)",
                &[
                    "api_a".to_string(),
                    "api_b".to_string(),
                    "api_c".to_string(),
                ],
                move |name, _args| {
                    let name = name.to_string();
                    let order = call_order_clone.clone();
                    Box::pin(async move {
                        order.lock().unwrap().push(name.clone());
                        Ok(serde_json::json!(format!("result_{}", name)))
                    })
                },
                |_prompt| {
                    // Rewrite async code as sequential calls
                    Box::pin(async {
                        Ok("a = api_a()\nb = api_b()\nc = api_c()\n\
                            results = [a, b, c]\nprint(results)"
                            .to_string())
                    })
                },
            )
            .await;
        assert!(
            result.success,
            "Expected success, got error: {:?}",
            result.error
        );
        assert!(result.retries >= 1, "Should have retried at least once");
        // All three tools should have been called sequentially
        assert_eq!(result.tool_results.len(), 3);

        let order = call_order.lock().unwrap();
        assert_eq!(order.len(), 3);
        assert_eq!(order[0], "api_a");
        assert_eq!(order[1], "api_b");
        assert_eq!(order[2], "api_c");
    }

    #[test]
    fn test_positional_to_named_auto() {
        let mapping = vec!["selector".to_string(), "button".to_string()];
        let args = serde_json::json!(["#submit", "right"]);
        let named = positional_to_named_auto(&mapping, &args);
        assert_eq!(
            named,
            serde_json::json!({"selector": "#submit", "button": "right"})
        );
    }

    #[test]
    fn test_positional_to_named_auto_object_passthrough() {
        let mapping = vec!["url".to_string()];
        let args = serde_json::json!({"url": "https://example.com"});
        let named = positional_to_named_auto(&mapping, &args);
        assert_eq!(named, serde_json::json!({"url": "https://example.com"}));
    }

    #[test]
    fn test_positional_to_named_auto_overflow() {
        let mapping = vec!["url".to_string()];
        let args = serde_json::json!(["https://example.com", "extra"]);
        let named = positional_to_named_auto(&mapping, &args);
        assert_eq!(
            named,
            serde_json::json!({"url": "https://example.com", "arg1": "extra"})
        );
    }

    #[test]
    fn test_positional_to_named_auto_empty() {
        let mapping: Vec<String> = vec![];
        let args = serde_json::json!(null);
        let named = positional_to_named_auto(&mapping, &args);
        assert_eq!(named, serde_json::json!({}));
    }

    #[test]
    fn test_positional_to_named_auto_kwargs_only() {
        // Pure kwargs: web_fetch(url="https://example.com")
        let mapping = vec!["url".to_string()];
        let args = serde_json::json!({
            "__positional": [],
            "__kwargs": {"url": "https://example.com"}
        });
        let named = positional_to_named_auto(&mapping, &args);
        assert_eq!(named, serde_json::json!({"url": "https://example.com"}));
    }

    #[test]
    fn test_positional_to_named_auto_mixed_positional_kwargs() {
        // Mixed: browser_click("#btn", button="right")
        let mapping = vec!["selector".to_string(), "button".to_string()];
        let args = serde_json::json!({
            "__positional": ["#btn"],
            "__kwargs": {"button": "right"}
        });
        let named = positional_to_named_auto(&mapping, &args);
        assert_eq!(
            named,
            serde_json::json!({"selector": "#btn", "button": "right"})
        );
    }

    #[test]
    fn test_positional_to_named_auto_kwargs_override_positional() {
        // kwargs should override positional when both specify the same param
        let mapping = vec!["url".to_string()];
        let args = serde_json::json!({
            "__positional": ["https://old.com"],
            "__kwargs": {"url": "https://new.com"}
        });
        let named = positional_to_named_auto(&mapping, &args);
        assert_eq!(named, serde_json::json!({"url": "https://new.com"}));
    }

    // ---- End-to-end integration tests ----

    #[tokio::test]
    async fn test_orchestrate_full_pipeline() {
        let executor = CodeModeExecutor::new();
        let result = executor
            .execute(
                r#"
a = fetch("https://a.com")
b = fetch("https://b.com")
combined = a + " | " + b
print(combined)
combined
"#,
                &["fetch".to_string()],
                |_name, args| {
                    Box::pin(async move {
                        let url = args
                            .as_array()
                            .and_then(|a| a.first())
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        Ok(serde_json::json!(format!("content from {}", url)))
                    })
                },
                |_prompt| Box::pin(async { Err("no rewrite".to_string()) }),
            )
            .await;

        assert!(result.success, "Expected success, got: {:?}", result.error);
        assert_eq!(result.tool_results.len(), 2);
        assert!(
            result.output.contains("content from"),
            "Expected output to contain 'content from', got: {:?}",
            result.output
        );
    }

    #[tokio::test]
    async fn test_orchestrate_auto_fix_import() {
        let executor = CodeModeExecutor::new();
        let result = executor
            .execute(
                "import json\nx = 42\nprint(x)",
                &[],
                |_name, _args| Box::pin(async { Ok(serde_json::json!("ok")) }),
                |_prompt| Box::pin(async { Err("no rewrite".to_string()) }),
            )
            .await;

        assert!(result.success, "Expected success, got: {:?}", result.error);
        assert!(
            result.output.contains("42"),
            "Expected output to contain '42', got: {:?}",
            result.output
        );
    }

    #[tokio::test]
    async fn test_orchestrate_tool_result_in_computation() {
        let executor = CodeModeExecutor::new();
        let result = executor
            .execute(
                "items = search(\"rust programming\")\ncount = len(items)\nprint(\"Found \" + str(count) + \" results\")",
                &["search".to_string()],
                |_name, _args| {
                    Box::pin(async {
                        Ok(serde_json::json!(["result1", "result2", "result3"]))
                    })
                },
                |_prompt| Box::pin(async { Err("no rewrite".to_string()) }),
            )
            .await;

        assert!(result.success, "Expected success, got: {:?}", result.error);
        assert!(
            result.output.contains("Found 3 results"),
            "Expected output to contain 'Found 3 results', got: {:?}",
            result.output
        );
    }

    #[tokio::test]
    async fn test_final_expression_captured_as_result() {
        // §3.7: Final expression value should be captured in `result` field
        let executor = CodeModeExecutor::new();
        let result = executor
            .execute(
                "x = 40 + 2\nx",
                &[],
                |_name, _args| Box::pin(async { Ok(serde_json::json!("ok")) }),
                |_prompt| Box::pin(async { Err("no rewrite".to_string()) }),
            )
            .await;
        assert!(result.success, "Expected success, got: {:?}", result.error);
        assert_eq!(
            result.result,
            Some(serde_json::json!(42)),
            "Final expression should be captured as result"
        );
    }

    #[tokio::test]
    async fn test_no_final_expression_result_is_none() {
        // When code ends with a statement (not an expression), result should be None
        let executor = CodeModeExecutor::new();
        let result = executor
            .execute(
                "x = 42\nprint(x)",
                &[],
                |_name, _args| Box::pin(async { Ok(serde_json::json!("ok")) }),
                |_prompt| Box::pin(async { Err("no rewrite".to_string()) }),
            )
            .await;
        assert!(result.success, "Expected success, got: {:?}", result.error);
        // print() returns None, which is filtered out
        assert!(
            result.result.is_none(),
            "Result should be None when code has no trailing expression, got: {:?}",
            result.result
        );
    }

    #[test]
    fn test_to_json_string_format() {
        // §3.7: Return format must be {"output", "result", "success", "error"}
        let result =
            CodeModeResult::success("hello world".to_string()).with_result(serde_json::json!(42));
        let json_str = result.to_json_string();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["output"], "hello world");
        assert_eq!(parsed["result"], 42);
        assert_eq!(parsed["success"], true);
        assert!(parsed["error"].is_null());
    }

    #[test]
    fn test_to_json_string_error_format() {
        let result = CodeModeResult::fail("something broke");
        let json_str = result.to_json_string();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["output"], "");
        assert!(parsed["result"].is_null());
        assert_eq!(parsed["success"], false);
        assert_eq!(parsed["error"], "something broke");
    }
}
