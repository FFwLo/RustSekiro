//! A stand-in for the `claude` and `codex` CLIs, used only by the offline tests (through
//! `HostedConfig::coach_cmd`, the `SEKIRO_COACH_CMD` override). It never talks to anything.
//!
//! Its settings are a JSON file named like the executable plus `.cfg` (the tests copy the
//! executable into their own temp directory): `plan` (the game plan text to answer with),
//! `mode` (`ok`, `fail`, `hang`) and `log` (a file to append the arguments, the working
//! directory and the prompt's first characters and length to).

use std::io::{Read, Write};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut stdin = String::new();
    let _ = std::io::stdin().read_to_string(&mut stdin);
    let mut cfg_path = std::env::current_exe().expect("exe path").into_os_string();
    cfg_path.push(".cfg");
    let cfg: serde_json::Value = std::fs::read_to_string(&cfg_path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    if let Some(path) = cfg["log"].as_str()
        && let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
    {
        let cwd = std::env::current_dir().unwrap_or_default();
        let mcp = args
            .iter()
            .position(|a| a == "--mcp-config")
            .and_then(|i| args.get(i + 1))
            .and_then(|p| std::fs::read_to_string(p).ok());
        let _ = writeln!(
            f,
            "{}",
            serde_json::json!({"args": args, "stdin_len": stdin.len(),
                "stdin_head": stdin.chars().take(40).collect::<String>(),
                "cwd": cwd.to_string_lossy(), "mcp": mcp})
        );
    }
    match cfg["mode"].as_str().unwrap_or("ok") {
        "hang" => std::thread::sleep(std::time::Duration::from_secs(60)),
        "fail" => std::process::exit(3),
        _ => {}
    }
    let plan = cfg["plan"].as_str().unwrap_or_default();
    if args.first().map(String::as_str) == Some("exec") {
        let out = args
            .iter()
            .position(|a| a == "-o")
            .and_then(|i| args.get(i + 1))
            .expect("-o file");
        std::fs::write(out, plan).expect("write answer");
        assert!(args.iter().any(|a| a == "--json"));
        for event in [
            serde_json::json!({"type":"thread.started", "thread_id":"fake-thread"}),
            serde_json::json!({"type":"turn.started"}),
            serde_json::json!({"type":"item.completed", "item":{"id":"item_0", "type":"agent_message", "text":"ignored: final answer is in -o"}}),
            serde_json::json!({"type":"turn.completed", "usage":{"input_tokens":10000, "cached_input_tokens":2000, "output_tokens":1000}}),
        ] {
            println!("{event}");
        }
    } else {
        let envelope = serde_json::json!({
            "type": "result", "subtype": "success", "is_error": false,
            "result": format!("```json\n{plan}\n```"),
            "usage": {"input_tokens": 120, "output_tokens": 45, "cache_read_input_tokens": 300},
            "total_cost_usd": 0.0012
        });
        println!("{envelope}");
    }
}
