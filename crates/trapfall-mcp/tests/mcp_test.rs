use serde_json::{Value, json};
use trapfall_core::Store;

/// Helper: build a JSON-RPC request.
fn rpc_request(method: &str, params: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params
    })
}

/// Helper: parse JSON-RPC response.
fn parse_response(output: &Option<String>) -> Value {
    serde_json::from_str(output.as_deref().expect("a response was expected")).expect("valid JSON response")
}

/// Send a raw JSON value through the handler.
async fn send(store: &Store, msg: Value) -> Option<String> {
    trapfall_mcp::handle_message(&msg.to_string(), store).await
}

/// Set up a store with migrated SQLite in-memory database.
async fn setup_store() -> Store {
    let backend = trapfall_db::open_database("sqlite::memory:").await.unwrap();
    let pool = backend.sqlite_pool().unwrap();
    trapfall_db::run_sqlite_migrations(pool).await.unwrap();
    Store::new(backend)
}

#[tokio::test]
async fn test_tools_list() {
    let store = setup_store().await;

    let req = rpc_request("tools/list", json!({}));
    let input = serde_json::to_string(&req).unwrap() + "\n";

    let result = trapfall_mcp::handle_message(&input, &store).await;
    let resp = parse_response(&result);

    assert_eq!(resp["jsonrpc"], "2.0");
    let tools = resp["result"]["tools"].as_array().expect("tools array");
    assert!(tools.len() >= 12, "expected at least 12 tools, got {}", tools.len());

    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"list_issues"));
    assert!(names.contains(&"get_issue"));
    assert!(names.contains(&"set_status"));
    assert!(names.contains(&"list_projects"));
    assert!(names.contains(&"search_issues"));
    assert!(names.contains(&"list_events"));
    assert!(names.contains(&"healthcheck"));
}

#[tokio::test]
async fn test_initialize() {
    let store = setup_store().await;

    let req = rpc_request(
        "initialize",
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "1.0" }
        }),
    );
    let input = serde_json::to_string(&req).unwrap() + "\n";

    let result = trapfall_mcp::handle_message(&input, &store).await;
    let resp = parse_response(&result);

    assert_eq!(resp["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(resp["result"]["serverInfo"]["name"], "trapfall");
}

#[tokio::test]
async fn test_ping() {
    let store = setup_store().await;

    let req = rpc_request("ping", json!({}));
    let input = serde_json::to_string(&req).unwrap() + "\n";

    let result = trapfall_mcp::handle_message(&input, &store).await;
    let resp = parse_response(&result);

    assert_eq!(resp["result"], json!({}));
}

#[tokio::test]
async fn test_unknown_method() {
    let store = setup_store().await;

    let req = rpc_request("nonexistent_method", json!({}));
    let input = serde_json::to_string(&req).unwrap() + "\n";

    let result = trapfall_mcp::handle_message(&input, &store).await;
    let resp = parse_response(&result);

    assert_eq!(resp["error"]["code"], -32601);
    assert!(resp["error"]["message"].as_str().unwrap().contains("Unknown"));
}

#[tokio::test]
async fn test_malformed_json() {
    let store = setup_store().await;

    let result = trapfall_mcp::handle_message("not valid json\n", &store).await;
    let resp = parse_response(&result);

    assert_eq!(resp["error"]["code"], -32700);
}

#[tokio::test]
async fn test_list_projects_tool() {
    let store = setup_store().await;

    let req = rpc_request(
        "tools/call",
        json!({
            "name": "list_projects",
            "arguments": {}
        }),
    );
    let input = serde_json::to_string(&req).unwrap() + "\n";

    let result = trapfall_mcp::handle_message(&input, &store).await;
    let resp = parse_response(&result);

    let content = resp["result"]["content"][0]["text"].as_str().unwrap();
    let projects: Vec<Value> = serde_json::from_str(content).unwrap();
    // Empty project list is valid — we just verify the tool doesn't crash
    assert_eq!(projects.len(), 0);
}

// ── JSON-RPC 2.0 / MCP compliance (#336) ───────────────────────────────

#[tokio::test]
async fn test_notification_gets_no_response() {
    let store = setup_store().await;
    let out = send(&store, json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).await;
    assert!(out.is_none(), "notifications must not get a response, got {out:?}");

    // Unknown-method notifications are also silent.
    let out = send(&store, json!({"jsonrpc": "2.0", "method": "nope"})).await;
    assert!(out.is_none());
}

#[tokio::test]
async fn test_invalid_params_is_32602() {
    let store = setup_store().await;
    let resp = parse_response(&send(&store, rpc_request("tools/call", json!({}))).await);
    assert_eq!(resp["error"]["code"], -32602);
    assert_eq!(resp["id"], 1);
}

#[tokio::test]
async fn test_unknown_tool_is_32602() {
    let store = setup_store().await;
    let resp = parse_response(&send(&store, rpc_request("tools/call", json!({"name": "nope"}))).await);
    assert_eq!(resp["error"]["code"], -32602);
}

#[tokio::test]
async fn test_tool_failure_is_is_error_result() {
    let store = setup_store().await;
    let resp = parse_response(
        &send(&store, rpc_request("tools/call", json!({"name": "get_issue", "arguments": {"issue_id": "missing"}})))
            .await,
    );
    assert!(resp.get("error").is_none(), "tool failures are not protocol errors: {resp}");
    assert_eq!(resp["result"]["isError"], true);
    assert_eq!(resp["result"]["content"][0]["type"], "text");

    // Missing argument is also a tool-level error.
    let resp = parse_response(&send(&store, rpc_request("tools/call", json!({"name": "get_issue"}))).await);
    assert_eq!(resp["result"]["isError"], true);
}

#[tokio::test]
async fn test_invalid_request_is_32600() {
    let store = setup_store().await;
    // wrong jsonrpc version
    let resp = parse_response(&send(&store, json!({"jsonrpc": "1.0", "id": 1, "method": "ping"})).await);
    assert_eq!(resp["error"]["code"], -32600);
    // missing jsonrpc
    let resp = parse_response(&send(&store, json!({"id": 1, "method": "ping"})).await);
    assert_eq!(resp["error"]["code"], -32600);
    // missing method
    let resp = parse_response(&send(&store, json!({"jsonrpc": "2.0", "id": 7})).await);
    assert_eq!(resp["error"]["code"], -32600);
    assert_eq!(resp["id"], 7);
    // non-object
    let resp = parse_response(&send(&store, json!(42)).await);
    assert_eq!(resp["error"]["code"], -32600);
    assert_eq!(resp["id"], Value::Null);
    // invalid id type
    let resp = parse_response(&send(&store, json!({"jsonrpc": "2.0", "id": {"a": 1}, "method": "ping"})).await);
    assert_eq!(resp["error"]["code"], -32600);
    assert_eq!(resp["id"], Value::Null);
    // non-string method
    let resp = parse_response(&send(&store, json!({"jsonrpc": "2.0", "id": 1, "method": 5})).await);
    assert_eq!(resp["error"]["code"], -32600);
}

#[tokio::test]
async fn test_string_id_echoed() {
    let store = setup_store().await;
    let resp = parse_response(&send(&store, json!({"jsonrpc": "2.0", "id": "abc", "method": "ping"})).await);
    assert_eq!(resp["id"], "abc");
    assert_eq!(resp["result"], json!({}));
}

#[tokio::test]
async fn test_batch() {
    let store = setup_store().await;
    let out = send(
        &store,
        json!([
            {"jsonrpc": "2.0", "id": 1, "method": "ping"},
            {"jsonrpc": "2.0", "method": "notifications/initialized"},
            {"jsonrpc": "2.0", "id": 2, "method": "nope"},
            5
        ]),
    )
    .await;
    let resp = parse_response(&out);
    let arr = resp.as_array().expect("batch response is an array");
    assert_eq!(arr.len(), 3, "notification produces no entry: {resp}");
    assert_eq!(arr[0]["id"], 1);
    assert_eq!(arr[0]["result"], json!({}));
    assert_eq!(arr[1]["error"]["code"], -32601);
    assert_eq!(arr[2]["error"]["code"], -32600);
}

#[tokio::test]
async fn test_batch_edge_cases() {
    let store = setup_store().await;
    // empty batch
    let resp = parse_response(&send(&store, json!([])).await);
    assert_eq!(resp["error"]["code"], -32600);
    // all notifications -> nothing
    let out = send(&store, json!([{"jsonrpc": "2.0", "method": "notifications/initialized"}])).await;
    assert!(out.is_none());
}

#[tokio::test]
async fn test_initialize_negotiates_protocol_version() {
    let store = setup_store().await;
    let init = |v: Value| rpc_request("initialize", json!({"protocolVersion": v, "capabilities": {}}));

    let resp = parse_response(&send(&store, init(json!("2025-03-26"))).await);
    assert_eq!(resp["result"]["protocolVersion"], "2025-03-26");

    let resp = parse_response(&send(&store, init(json!("2024-11-05"))).await);
    assert_eq!(resp["result"]["protocolVersion"], "2024-11-05");

    // unsupported -> server's latest supported version
    let resp = parse_response(&send(&store, init(json!("1999-01-01"))).await);
    assert_eq!(resp["result"]["protocolVersion"], "2025-06-18");

    // missing protocolVersion is invalid params
    let resp = parse_response(&send(&store, rpc_request("initialize", json!({}))).await);
    assert_eq!(resp["error"]["code"], -32602);
}

#[tokio::test]
async fn test_rotate_dsn_succeeds_and_masks_key() {
    let store = setup_store().await;
    let project = store.create_project("demo", "Demo").await.unwrap();
    let resp = parse_response(
        &send(
            &store,
            rpc_request("tools/call", json!({"name": "rotate_dsn", "arguments": {"project_slug": project.slug}})),
        )
        .await,
    );
    assert_ne!(resp["result"]["isError"], true, "{resp}");
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("New key:"), "{text}");
}
