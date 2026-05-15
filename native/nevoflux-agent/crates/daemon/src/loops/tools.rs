//! LLM-callable tool handlers for /loop (spec §10).
//!
//! Five tools share one dispatcher so registration in Phase 9
//! (builtin-wasm + mcp_tool_executor) is a single name-list per call site.

use crate::loops::manager::{CreateLoopArgs, LoopManager};
use crate::loops::tool_classes::is_forbidden_in_iteration;
use crate::loops::types::LoopId;
use nevoflux_storage::connection::Database;
use nevoflux_storage::models::current_timestamp;
use nevoflux_storage::repositories::LoopRepository;
use serde_json::{json, Value};

/// Execution context for a tool call.
///
/// `is_iteration: false` ⇒ main-session call (sidebar-driven LLM).
/// `is_iteration: true`  ⇒ call from inside a loop iteration's AgentRunner.
/// `own_loop_id` is set only when `is_iteration` so dispatch can enforce
/// "iterations may only cancel their own loop_id" and similar rules.
#[derive(Debug, Clone)]
pub struct ToolCallContext {
    pub session_id: String,
    pub is_iteration: bool,
    pub own_loop_id: Option<LoopId>,
}

pub async fn execute_loop_tool(
    name: &str,
    args: &Value,
    ctx: &ToolCallContext,
    mgr: &LoopManager,
    db: &Database,
) -> Result<Value, String> {
    if ctx.is_iteration && is_forbidden_in_iteration(name) {
        return Err(format!("{name} is forbidden inside loop iterations"));
    }
    match name {
        "loop.create" => loop_create(args, ctx, mgr).await,
        "loop.list" => loop_list(args, ctx, db),
        "loop.cancel" => loop_cancel(args, ctx, mgr).await,
        "loop.scratchpad.get" => scratchpad_get(args, ctx, db),
        "loop.scratchpad.set" => scratchpad_set(args, ctx, mgr, db).await,
        _ => Err(format!("unknown loop tool: {name}")),
    }
}

async fn loop_create(
    args: &Value,
    ctx: &ToolCallContext,
    mgr: &LoopManager,
) -> Result<Value, String> {
    if ctx.is_iteration {
        return Err("loop.create cannot be called from inside an iteration".into());
    }
    let trigger_expr_text = args
        .get("trigger_expr")
        .and_then(|v| v.as_str())
        .ok_or("trigger_expr required")?
        .to_string();
    let prompt_text = args
        .get("prompt_text")
        .and_then(|v| v.as_str())
        .map(String::from);
    let wrapped_skill = args.get("wrapped_skill").map(|v| v.to_string());
    // Optional `mode` arg: one of "chat" | "browser" | "agent". Defaults to
    // Chat (matches `server.rs::parse_agent_mode` semantics).
    let mode = args
        .get("mode")
        .and_then(|v| v.as_str())
        .map(crate::loops::manager::db_str_to_agent_mode)
        .unwrap_or(nevoflux_builtin_wasm::AgentMode::Chat);

    let id = mgr
        .create_loop(CreateLoopArgs {
            session_id: ctx.session_id.clone(),
            trigger_expr_text,
            prompt_text,
            wrapped_skill,
            mode,
        })
        .await?;
    Ok(json!({ "loop_id": id.0 }))
}

fn loop_list(_args: &Value, ctx: &ToolCallContext, db: &Database) -> Result<Value, String> {
    let rows = LoopRepository::new(db)
        .list_by_session(&ctx.session_id)
        .map_err(|e| e.to_string())?;
    let out: Vec<Value> = rows
        .iter()
        .map(|r| {
            let preview: String = r.scratchpad.chars().take(120).collect();
            json!({
                "loop_id": r.id,
                "state": r.state.as_str(),
                "trigger_expr": r.trigger_expr,
                "iteration_count": r.iteration_count,
                "scratchpad_preview": preview,
            })
        })
        .collect();
    Ok(json!(out))
}

async fn loop_cancel(
    args: &Value,
    ctx: &ToolCallContext,
    mgr: &LoopManager,
) -> Result<Value, String> {
    let target = args
        .get("loop_id")
        .and_then(|v| v.as_str())
        .ok_or("loop_id required")?;
    if ctx.is_iteration {
        match &ctx.own_loop_id {
            Some(own) if own.as_ref() == target => {}
            _ => return Err("iteration may only cancel its own loop_id".into()),
        }
    }
    mgr.cancel_loop(&LoopId(target.to_string()), false).await?;
    Ok(json!({ "cancelled": true }))
}

fn scratchpad_get(args: &Value, ctx: &ToolCallContext, db: &Database) -> Result<Value, String> {
    let target = args
        .get("loop_id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| ctx.own_loop_id.as_ref().map(|i| i.0.clone()))
        .ok_or("loop_id required (no current iteration context)")?;
    let rec = LoopRepository::new(db)
        .get(&target)
        .map_err(|e| e.to_string())?
        .ok_or("loop not found")?;
    Ok(json!({ "content": rec.scratchpad, "bytes": rec.scratchpad.len() }))
}

async fn scratchpad_set(
    args: &Value,
    ctx: &ToolCallContext,
    mgr: &LoopManager,
    db: &Database,
) -> Result<Value, String> {
    if !ctx.is_iteration {
        return Err("loop.scratchpad.set is only callable from inside an iteration".into());
    }
    let own = ctx
        .own_loop_id
        .as_ref()
        .ok_or("no own_loop_id in iteration context")?;
    let content = args
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or("content (string) required")?;
    if content.len() > 4096 {
        return Err(format!(
            "content exceeds 4096 bytes (got {})",
            content.len()
        ));
    }
    LoopRepository::new(db)
        .update_scratchpad(&own.0, content, current_timestamp())
        .map_err(|e| e.to_string())?;
    mgr.events()
        .scratchpad_changed(&ctx.session_id, own, content)
        .await;
    Ok(json!({ "bytes_written": content.len() }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loops::manager::LoopManager;
    use nevoflux_storage::models::CreateSessionParams;
    use nevoflux_storage::Storage;

    fn setup() -> (Storage, LoopManager) {
        let storage = Storage::open_in_memory().unwrap();
        storage
            .sessions()
            .create(CreateSessionParams::new().with_id("s1").with_title("t"))
            .unwrap();
        let mgr = LoopManager::start(storage.database().clone());
        (storage, mgr)
    }

    fn ctx(is_iter: bool, own: Option<&str>) -> ToolCallContext {
        ToolCallContext {
            session_id: "s1".into(),
            is_iteration: is_iter,
            own_loop_id: own.map(|s| LoopId(s.into())),
        }
    }

    #[tokio::test]
    async fn loop_create_then_list_includes_it() {
        let (storage, mgr) = setup();
        let res = execute_loop_tool(
            "loop.create",
            &json!({ "trigger_expr": "time:5m", "prompt_text": "x" }),
            &ctx(false, None),
            &mgr,
            storage.database(),
        )
        .await
        .unwrap();
        let id = res.get("loop_id").unwrap().as_str().unwrap().to_string();

        let list = execute_loop_tool(
            "loop.list",
            &json!({}),
            &ctx(false, None),
            &mgr,
            storage.database(),
        )
        .await
        .unwrap();
        let arr = list.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0].get("loop_id").unwrap().as_str().unwrap(), id);
        assert_eq!(arr[0].get("state").unwrap().as_str().unwrap(), "pending");
        assert_eq!(
            arr[0].get("trigger_expr").unwrap().as_str().unwrap(),
            "time:5m"
        );
    }

    #[tokio::test]
    async fn loop_create_blocked_in_iteration() {
        let (storage, mgr) = setup();
        let err = execute_loop_tool(
            "loop.create",
            &json!({ "trigger_expr": "time:5m", "prompt_text": "x" }),
            &ctx(true, Some("aaa")),
            &mgr,
            storage.database(),
        )
        .await
        .unwrap_err();
        assert!(
            err.contains("forbidden") || err.contains("inside an iteration"),
            "got: {err}"
        );
    }

    #[tokio::test]
    async fn scratchpad_set_rejects_oversize() {
        let (storage, mgr) = setup();
        let id = execute_loop_tool(
            "loop.create",
            &json!({ "trigger_expr": "time:5m", "prompt_text": "x" }),
            &ctx(false, None),
            &mgr,
            storage.database(),
        )
        .await
        .unwrap()
        .get("loop_id")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

        let big = "x".repeat(4097);
        let err = execute_loop_tool(
            "loop.scratchpad.set",
            &json!({ "content": big }),
            &ctx(true, Some(&id)),
            &mgr,
            storage.database(),
        )
        .await
        .unwrap_err();
        assert!(err.contains("4096"), "got: {err}");
    }

    #[tokio::test]
    async fn scratchpad_set_persists_under_limit() {
        let (storage, mgr) = setup();
        let id = execute_loop_tool(
            "loop.create",
            &json!({ "trigger_expr": "time:5m", "prompt_text": "x" }),
            &ctx(false, None),
            &mgr,
            storage.database(),
        )
        .await
        .unwrap()
        .get("loop_id")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

        execute_loop_tool(
            "loop.scratchpad.set",
            &json!({ "content": "k=v" }),
            &ctx(true, Some(&id)),
            &mgr,
            storage.database(),
        )
        .await
        .unwrap();

        let got = execute_loop_tool(
            "loop.scratchpad.get",
            &json!({ "loop_id": id }),
            &ctx(false, None),
            &mgr,
            storage.database(),
        )
        .await
        .unwrap();
        assert_eq!(got.get("content").unwrap().as_str().unwrap(), "k=v");
        assert_eq!(got.get("bytes").unwrap().as_i64().unwrap(), 3);
    }

    #[tokio::test]
    async fn cancel_other_loop_from_iteration_rejected() {
        let (storage, mgr) = setup();
        let id = execute_loop_tool(
            "loop.create",
            &json!({ "trigger_expr": "time:5m", "prompt_text": "x" }),
            &ctx(false, None),
            &mgr,
            storage.database(),
        )
        .await
        .unwrap()
        .get("loop_id")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

        let err = execute_loop_tool(
            "loop.cancel",
            &json!({ "loop_id": &id }),
            &ctx(true, Some("self")),
            &mgr,
            storage.database(),
        )
        .await
        .unwrap_err();
        assert!(err.contains("only cancel its own"), "got: {err}");
    }

    #[tokio::test]
    async fn scratchpad_set_outside_iteration_rejected() {
        let (storage, mgr) = setup();
        let id = execute_loop_tool(
            "loop.create",
            &json!({ "trigger_expr": "time:5m", "prompt_text": "x" }),
            &ctx(false, None),
            &mgr,
            storage.database(),
        )
        .await
        .unwrap()
        .get("loop_id")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

        let err = execute_loop_tool(
            "loop.scratchpad.set",
            &json!({ "content": "k=v" }),
            &ctx(false, None),
            &mgr,
            storage.database(),
        )
        .await
        .unwrap_err();
        assert!(err.contains("iteration"), "got: {err}");
        let _ = id;
    }

    #[tokio::test]
    async fn unknown_tool_name_errors() {
        let (storage, mgr) = setup();
        let err = execute_loop_tool(
            "loop.invented",
            &json!({}),
            &ctx(false, None),
            &mgr,
            storage.database(),
        )
        .await
        .unwrap_err();
        assert!(err.contains("unknown loop tool"));
    }
}
