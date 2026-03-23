use std::io::{Read, Write};
use std::net::TcpListener;
use crate::types::RuntimeCtx;

pub fn run_server(ctx: RuntimeCtx, bind: &str, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let addr = format!("{bind}:{port}");
    let listener = TcpListener::bind(&addr)?;
    println!("Listening on http://{addr}");

    for stream in listener.incoming() {
        match stream {
            Ok(mut s) => {
                let mut buf = [0u8; 4096];
                let n = s.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]);
                let first_line = request.lines().next().unwrap_or("");
                let mut parts = first_line.splitn(3, ' ');
                let method = parts.next().unwrap_or("");
                let path = parts.next().unwrap_or("/");

                let (status, body) = handle_request(method, path, &ctx);
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = s.write_all(response.as_bytes());
            }
            Err(_) => {}
        }
    }
    Ok(())
}

fn handle_request(method: &str, path: &str, ctx: &RuntimeCtx) -> (&'static str, String) {
    if method != "GET" {
        return ("405 Method Not Allowed", r#"{"error":"method not allowed"}"#.to_string());
    }
    match path {
        "/health" => ("200 OK", r#"{"ok":true}"#.to_string()),
        "/status" => ("200 OK", build_status(ctx)),
        "/goals" => ("200 OK", build_goals(ctx)),
        _ => ("404 Not Found", r#"{"error":"not found"}"#.to_string()),
    }
}

fn build_status(ctx: &RuntimeCtx) -> String {
    let goals = &ctx.state.goals;
    let mut pending = 0usize;
    let mut doing = 0usize;
    let mut blocked = 0usize;
    let mut done = 0usize;
    let mut aborted = 0usize;
    let mut other = 0usize;
    for g in goals {
        match g.status.as_str() {
            "pending" => pending += 1,
            "doing" | "recovering" => doing += 1,
            "blocked" => blocked += 1,
            "done" => done += 1,
            "aborted" => aborted += 1,
            _ => other += 1,
        }
    }
    let worker_count = ctx.state.workers.len();
    format!(
        r#"{{"organism_id":"{org}","mode":"{mode}","policy_enforcement":"{policy}","continuity_epoch":{epoch},"goal_counts":{{"pending":{pending},"doing":{doing},"blocked":{blocked},"done":{done},"aborted":{aborted},"other":{other}}},"worker_count":{worker_count}}}"#,
        org = ctx.identity.organism_id,
        mode = ctx.state.mode.as_str(),
        policy = ctx.state.policy.enforcement.as_str(),
        epoch = ctx.state.continuity_epoch,
    )
}

fn build_goals(ctx: &RuntimeCtx) -> String {
    let items: Vec<String> = ctx.state.goals.iter().map(|g| {
        let tags = g.tags.iter()
            .map(|t| format!("\"{}\"", t.replace('"', "\\\"")))
            .collect::<Vec<_>>()
            .join(",");
        let delegated = match &g.delegated_to {
            Some(p) => format!("\"{}\"", p.replace('"', "\\\"")),
            None => "null".to_string(),
        };
        format!(
            r#"{{"id":"{id}","title":"{title}","status":"{status}","priority":{priority},"tags":[{tags}],"delegated_to":{delegated}}}"#,
            id = g.goal_id,
            title = g.title.replace('"', "\\\""),
            status = g.status,
            priority = g.priority,
        )
    }).collect();
    format!("[{}]", items.join(","))
}
