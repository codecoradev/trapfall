use serde_json::{Value, json};

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
fn parse_response(output: &str) -> Value {
    serde_json::from_str(output).expect("valid JSON response")
}

#[tokio::test]
async fn test_tools_list() {
    let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
    let store = trapfall_core::Store::new(pool.clone());

    let req = rpc_request("tools/list", json!({}));
    let input = serde_json::to_string(&req).unwrap() + "\n";

    let result = trapfall_mcp::handle_message(&input, &pool, &store).await;
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
    let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
    let store = trapfall_core::Store::new(pool.clone());

    let req = rpc_request(
        "initialize",
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "1.0" }
        }),
    );
    let input = serde_json::to_string(&req).unwrap() + "\n";

    let result = trapfall_mcp::handle_message(&input, &pool, &store).await;
    let resp = parse_response(&result);

    assert_eq!(resp["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(resp["result"]["serverInfo"]["name"], "trapfall");
}

#[tokio::test]
async fn test_ping() {
    let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
    let store = trapfall_core::Store::new(pool.clone());

    let req = rpc_request("ping", json!({}));
    let input = serde_json::to_string(&req).unwrap() + "\n";

    let result = trapfall_mcp::handle_message(&input, &pool, &store).await;
    let resp = parse_response(&result);

    assert_eq!(resp["result"], json!({}));
}

#[tokio::test]
async fn test_unknown_method() {
    let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
    let store = trapfall_core::Store::new(pool.clone());

    let req = rpc_request("nonexistent_method", json!({}));
    let input = serde_json::to_string(&req).unwrap() + "\n";

    let result = trapfall_mcp::handle_message(&input, &pool, &store).await;
    let resp = parse_response(&result);

    assert!(resp["error"]["code"].is_number());
    assert!(resp["error"]["message"].as_str().unwrap().contains("Unknown"));
}

#[tokio::test]
async fn test_malformed_json() {
    let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
    let store = trapfall_core::Store::new(pool.clone());

    let result = trapfall_mcp::handle_message("not valid json\n", &pool, &store).await;
    let resp = parse_response(&result);

    assert_eq!(resp["error"]["code"], -32700);
}

#[tokio::test]
async fn test_list_projects_tool() {
    let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();

    // Create schema + seed
    trapfall_core::Store::new(pool.clone()).create_project("test-project", "Test Project").await.unwrap();

    let store = trapfall_core::Store::new(pool.clone());
    let req = rpc_request(
        "tools/call",
        json!({
            "name": "list_projects",
            "arguments": {}
        }),
    );
    let input = serde_json::to_string(&req).unwrap() + "\n";

    let result = trapfall_mcp::handle_message(&input, &pool, &store).await;
    let resp = parse_response(&result);

    let content = resp["result"]["content"][0]["text"].as_str().unwrap();
    let projects: Vec<Value> = serde_json::from_str(content).unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0]["slug"], "test-project");
}
