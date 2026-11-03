use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use sekiro_brain_net::{ApiKey, Backend, ConfigError, Failure, HostedBrain, HostedConfig};
use sekiro_sim::brain::{
    BossBrain, BrainReply, BrainRequest, Memory, OWL, Observation, RequestMode, SOLDIER,
    TallySummary,
};
use serde_json::{Value, json};

const KEY: &str = "test-key-not-real";

struct TestLogger(Mutex<Vec<String>>);
static LOGGER: TestLogger = TestLogger(Mutex::new(Vec::new()));

impl log::Log for TestLogger {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        self.0.lock().unwrap().push(record.args().to_string());
    }
    fn flush(&self) {}
}

struct Response {
    status: u16,
    body: String,
    delay: Duration,
    headers_first: bool,
}

impl Response {
    fn json(body: Value) -> Self {
        Self {
            status: 200,
            body: body.to_string(),
            delay: Duration::ZERO,
            headers_first: false,
        }
    }
}

struct Mock {
    base: String,
    requests: mpsc::Receiver<Value>,
    accepted: Arc<AtomicUsize>,
    started: mpsc::Receiver<()>,
    worker: Option<JoinHandle<()>>,
}

impl Mock {
    fn new(backend: Backend, responses: Vec<Response>) -> Self {
        Self::at(backend.endpoint(), responses)
    }

    /// A mock of the endpoint `/v1/<path>`.
    fn at(path: &str, responses: Vec<Response>) -> Self {
        let path = path.to_string();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (tx, requests) = mpsc::channel();
        let (start_tx, started) = mpsc::channel();
        let accepted = Arc::new(AtomicUsize::new(0));
        let count = accepted.clone();
        // A read timeout lets the mock accept a replacement if the client closes a connection.
        let worker = thread::spawn(move || {
            let mut connection: Option<BufReader<TcpStream>> = None;
            for response in responses {
                let mut first = String::new();
                loop {
                    if connection.is_none() {
                        listener.set_nonblocking(true).unwrap();
                        let deadline = Instant::now() + Duration::from_secs(3);
                        let stream = loop {
                            match listener.accept() {
                                Ok((stream, _)) => break stream,
                                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                    assert!(
                                        Instant::now() < deadline,
                                        "mock accept deadline exceeded"
                                    );
                                    thread::sleep(Duration::from_millis(1));
                                }
                                Err(_) => panic!("mock accept failed"),
                            }
                        };
                        count.fetch_add(1, Ordering::Relaxed);
                        // Windows accepted sockets inherit a nonblocking listener's mode.
                        stream.set_nonblocking(false).unwrap();
                        stream
                            .set_read_timeout(Some(Duration::from_secs(3)))
                            .unwrap();
                        connection = Some(BufReader::new(stream));
                    }
                    first.clear();
                    match connection.as_mut().unwrap().read_line(&mut first) {
                        Ok(n) if n > 0 => break,
                        _ => connection = None,
                    }
                }
                assert_eq!(first.trim(), format!("POST /v1/{path} HTTP/1.1"));
                let reader = connection.as_mut().unwrap();
                let mut length = None;
                let mut authorized = false;
                let mut content_type = false;
                loop {
                    let mut header = String::new();
                    reader.read_line(&mut header).unwrap();
                    if header == "\r\n" {
                        break;
                    }
                    let (name, value) = header.split_once(':').unwrap();
                    match name.to_ascii_lowercase().as_str() {
                        "content-length" => length = Some(value.trim().parse::<usize>().unwrap()),
                        // Do not put the received credential into an assertion diagnostic.
                        "authorization" => authorized = value.trim() == format!("Bearer {KEY}"),
                        "content-type" => {
                            content_type =
                                value.trim().split(';').next() == Some("application/json")
                        }
                        _ => (),
                    }
                }
                assert!(authorized, "mock authorization mismatch");
                assert!(content_type, "mock content type mismatch");
                let mut body = vec![0; length.unwrap()];
                reader.read_exact(&mut body).unwrap();
                tx.send(serde_json::from_slice(&body).unwrap()).unwrap();
                let _ = start_tx.send(());
                let message = format!(
                    "HTTP/1.1 {} Mock\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n{}",
                    response.status,
                    response.body.len(),
                    response.body
                );
                // Timeouts and drop intentionally close the other end before the mock writes.
                if response.headers_first {
                    let (headers, body) = message.split_once("\r\n\r\n").unwrap();
                    let _ = reader.get_mut().write_all(headers.as_bytes());
                    let _ = reader.get_mut().write_all(b"\r\n\r\n");
                    let _ = reader.get_mut().flush();
                    thread::sleep(response.delay);
                    let _ = reader.get_mut().write_all(body.as_bytes());
                } else {
                    thread::sleep(response.delay);
                    let _ = reader.get_mut().write_all(message.as_bytes());
                }
                let _ = reader.get_mut().flush();
            }
        });
        Self {
            base,
            requests,
            accepted,
            started,
            worker: Some(worker),
        }
    }

    fn config(&self, backend: Backend) -> HostedConfig {
        let mut config = HostedConfig::new(backend, ApiKey::new(KEY));
        config.checkpoint_chooser = sekiro_sim::brain::bandit::CheckpointChooser::Decisions;
        config.base_url = self.base.clone();
        // Tests count accepted connections; the warm-up has a test of its own.
        config.warm_up = false;
        // The mock server stands in for the Responses API, so its coach is the API backend.
        config.coach_backend = CoachBackend::Api;
        config.coach_model = CoachBackend::Api.default_model().into();
        config.coach_effort = "low".into();
        config
    }

    fn finish(&mut self) {
        self.worker.take().unwrap().join().unwrap();
    }
}

fn request(id: u64) -> BrainRequest {
    BrainRequest {
        id,
        obs: Observation::default(),
        tally: TallySummary::default(),
        memory: None,
        ..BrainRequest::default()
    }
}

fn poll(brain: &mut HostedBrain) -> BrainReply {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(reply) = brain.poll(0.0) {
            return reply;
        }
        assert!(Instant::now() < deadline, "reply deadline exceeded");
        thread::sleep(Duration::from_millis(1));
    }
}

fn decisions(value: &str, input: u64, output: u64) -> Value {
    json!({"model": "gpt-6-luna", "answers": [{"name": "boss_action", "type": "choice",
        "choice": value, "confidence": 1.0, "probabilities": [{"value": value, "probability": 1.0}]}],
        "usage": {"input_tokens": input, "output_tokens": output}})
}

fn responses(value: &str) -> Value {
    json!({"model": "gpt-6-luna", "status": "completed", "output": [
        {"type": "message", "role": "assistant", "content": [
            {"type": "output_text", "text": json!({"action": value}).to_string()}]}],
        "usage": {"input_tokens": 12, "output_tokens": 4}})
}

fn check_vocabulary(values: &[String]) {
    // His actions, his combo, the tactics and his curated chains, then `lua`.
    let mut expected: Vec<_> = OWL.choices().into_iter().map(|c| c.value).collect();
    expected.push("lua".into());
    assert_eq!(values, expected);
    assert!(values.len() <= 255);
    for want in [
        "3005",
        "3000+3001+3002",
        "hold2",
        "walk_in4",
        "retreat12",
        "walk_in4>3005",
    ] {
        assert!(values.iter().any(|v| v == want), "{want}");
    }
}

#[test]
fn decisions_happy_path_lua_usage_log_and_keep_alive() {
    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(log::LevelFilter::Trace);
    let mut mock = Mock::new(
        Backend::Decisions,
        vec![
            Response::json(decisions("3005", 11, 2)),
            Response::json(decisions("lua", 20, 3)),
        ],
    );
    let mut config = mock.config(Backend::Decisions);
    let path = std::env::temp_dir().join(format!(
        "brain-net-{}-{}.jsonl",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    config.log_path = Some(path.clone());
    assert!(!format!("{config:?}").contains(KEY));
    let mut brain = HostedBrain::new(config, &OWL).unwrap();
    let handle = brain.stats_handle();
    assert_eq!(brain.name(), "decisions");
    brain.submit(&request(41));
    let reply = poll(&mut brain);
    assert_eq!(reply.id, 41);
    assert_eq!(reply.plan.unwrap().actions(), [3005]);
    assert!(brain.poll(0.0).is_none());
    brain.submit(&request(42));
    let reply = poll(&mut brain);
    assert_eq!(reply.id, 42);
    assert!(reply.plan.is_none());
    mock.finish();
    let diagnostics = LOGGER.0.lock().unwrap();
    assert!(
        diagnostics.iter().all(|line| !line.contains(KEY)),
        "HTTP diagnostics must redact the key"
    );
    assert!(
        log::STATIC_MAX_LEVEL <= log::LevelFilter::Debug,
        "ureq-proto raw wire traces must be compiled out"
    );
    drop(diagnostics);
    assert_eq!(
        mock.accepted.load(Ordering::Relaxed),
        1,
        "agent must reuse its TCP connection"
    );
    for id in [41, 42] {
        let body = mock.requests.recv().unwrap();
        assert_eq!(body["model"], "gpt-6-luna");
        let input: Value = serde_json::from_str(body["input"].as_str().unwrap()).unwrap();
        assert_eq!(input["id"], id);
        assert_eq!(body["input"], serde_json::to_string(&request(id)).unwrap());
        let questions = body["questions"].as_array().unwrap();
        assert_eq!(questions.len(), 1);
        let q = &questions[0];
        assert_eq!(q["type"], "choice");
        assert_eq!(q["name"], "boss_action");
        assert!(q["instructions"].as_str().unwrap().contains("Reflexes"));
        let choices = q["choices"].as_array().unwrap();
        check_vocabulary(
            &choices
                .iter()
                .map(|v| v["value"].as_str().unwrap().to_owned())
                .collect::<Vec<_>>(),
        );
        for (choice, action) in choices.iter().zip(OWL.actions) {
            assert_eq!(choice["description"], action.label);
        }
    }
    let stats = brain.stats();
    assert_eq!(stats.requests, 2);
    assert_eq!(stats.answered, 2);
    assert!(stats.failed.is_empty());
    assert_eq!((stats.input_tokens, stats.output_tokens), (31, 5));
    assert!(stats.mean_round_trip_ms > 0.0);
    assert!(stats.max_round_trip_ms >= stats.mean_round_trip_ms);
    assert!(!format!("{stats:?}").contains(KEY));
    let log = std::fs::read_to_string(&path).unwrap();
    assert!(!log.contains(KEY));
    assert!(!log.to_ascii_lowercase().contains("authorization"));
    let lines: Vec<Value> = log
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["chosen"], "3005");
    assert_eq!(lines[1]["chosen"], "lua");
    assert_eq!(lines[1]["status"], "answered");
    assert_eq!(lines[0]["usage"]["input_tokens"], 11);
    drop(brain);
    assert_eq!(handle.snapshot().answered, 2);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn responses_happy_path_combo_and_exact_schema() {
    let mut mock = Mock::new(
        Backend::Responses,
        vec![Response::json(responses("3000+3001+3002"))],
    );
    let mut config = mock.config(Backend::Responses);
    config.model = "test-model-override".into();
    let mut brain = HostedBrain::new(config, &OWL).unwrap();
    assert_eq!(brain.name(), "responses");
    brain.submit(&request(9));
    let reply = poll(&mut brain);
    assert_eq!(reply.id, 9);
    assert_eq!(reply.plan.unwrap().actions(), [3000, 3001, 3002]);
    mock.finish();
    let body = mock.requests.recv().unwrap();
    assert_eq!(body["model"], "test-model-override");
    assert_eq!(body["reasoning"], json!({"effort": "none"}));
    assert_eq!(body["max_output_tokens"], 16);
    assert_eq!(body["store"], false);
    assert_eq!(
        body["input"],
        json!([{"role":"user", "content": serde_json::to_string(&request(9)).unwrap()}])
    );
    let format = &body["text"]["format"];
    assert_eq!(format["type"], "json_schema");
    assert_eq!(format["name"], "boss_action");
    assert_eq!(format["strict"], true);
    assert_eq!(format["schema"]["type"], "object");
    assert_eq!(format["schema"]["properties"]["action"]["type"], "string");
    assert_eq!(format["schema"]["required"], json!(["action"]));
    assert_eq!(format["schema"]["additionalProperties"], false);
    check_vocabulary(
        &format["schema"]["properties"]["action"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        (brain.stats().input_tokens, brain.stats().output_tokens),
        (12, 4)
    );
}

#[test]
fn failures_fall_back_with_specific_reasons_and_never_echo_secrets() {
    let cases = vec![
        (
            Backend::Decisions,
            Response::json(
                json!({"answers": [{"type": "refusal", "name": "boss_action"}], "usage":{"input_tokens":7}}),
            ),
            Failure::Refusal,
        ),
        (
            Backend::Responses,
            Response::json(
                json!({"status": "completed", "output": [{"type": "message", "role": "assistant", "content": [{"type": "refusal", "refusal": KEY}]}]}),
            ),
            Failure::Refusal,
        ),
        (
            Backend::Decisions,
            Response {
                status: 429,
                body: KEY.into(),
                delay: Duration::ZERO,
                headers_first: false,
            },
            Failure::Http(429),
        ),
        (
            Backend::Responses,
            Response {
                status: 500,
                body: KEY.into(),
                delay: Duration::ZERO,
                headers_first: false,
            },
            Failure::Http(500),
        ),
        (
            Backend::Decisions,
            Response {
                status: 200,
                body: KEY.into(),
                delay: Duration::ZERO,
                headers_first: false,
            },
            Failure::MalformedJson,
        ),
        (
            Backend::Responses,
            Response::json(responses("9999")),
            Failure::UnknownAction,
        ),
        (
            Backend::Decisions,
            Response::json(decisions(KEY, 1, 2)),
            Failure::UnknownAction,
        ),
        (
            Backend::Responses,
            Response::json(json!({"status":"incomplete", "output":[]})),
            Failure::Incomplete,
        ),
        (
            Backend::Decisions,
            Response::json(json!({"answers":[]})),
            Failure::InvalidAnswer,
        ),
        (
            Backend::Responses,
            Response::json(
                json!({"status":"completed", "output":[{"type":"message", "role":"assistant", "content":[{"type":"output_text", "text":KEY}]}]}),
            ),
            Failure::InvalidAnswer,
        ),
        (
            Backend::Decisions,
            Response::json(decisions("3000+3001+3002+3000", 0, 0)),
            Failure::InvalidPlan,
        ),
    ];
    for (backend, response, reason) in cases {
        let mut mock = Mock::new(backend, vec![response]);
        let path = std::env::temp_dir().join(format!(
            "brain-net-failure-{}-{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut config = mock.config(backend);
        config.log_path = Some(path.clone());
        let mut brain = HostedBrain::new(config, &OWL).unwrap();
        brain.submit(&request(12));
        let reply = poll(&mut brain);
        assert_eq!(reply.id, 12);
        assert!(reply.plan.is_none());
        let stats = brain.stats();
        assert_eq!(stats.requests, 1);
        assert_eq!(stats.answered, 0);
        assert_eq!(stats.failed.get(&reason), Some(&1));
        assert!(!format!("{stats:?}").contains(KEY));
        let log = std::fs::read_to_string(&path).unwrap();
        assert!(!log.contains(KEY));
        let line: Value = serde_json::from_str(log.trim()).unwrap();
        assert_eq!(line["chosen"], Value::Null);
        assert_eq!(line["status"], "failed");
        if reason == Failure::Refusal && backend == Backend::Decisions {
            assert_eq!(stats.input_tokens, 7);
        }
        mock.finish();
        drop(brain);
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn slow_http_never_blocks_submit_and_timeout_is_bounded() {
    let timeout = Duration::from_millis(500);
    let mut response = Response::json(decisions("3005", 0, 0));
    response.delay = Duration::from_millis(800);
    let mut mock = Mock::new(Backend::Decisions, vec![response]);
    let mut config = mock.config(Backend::Decisions);
    config.timeout = timeout;
    let mut brain = HostedBrain::new(config, &OWL).unwrap();
    let start = Instant::now();
    brain.submit(&request(77));
    assert!(
        start.elapsed() < Duration::from_millis(5),
        "submit must not wait for HTTP"
    );
    let reply = poll(&mut brain);
    assert!(start.elapsed() < timeout + Duration::from_millis(500));
    assert_eq!(reply.id, 77);
    assert!(reply.plan.is_none());
    assert_eq!(brain.stats().failed.get(&Failure::Timeout), Some(&1));
    mock.finish();
}

#[test]
fn drop_in_flight_is_bounded_and_does_not_drain_queued_requests() {
    let timeout = Duration::from_millis(500);
    let mut response = Response::json(decisions("3005", 0, 0));
    response.delay = Duration::from_millis(800);
    let mut mock = Mock::new(Backend::Decisions, vec![response]);
    let mut config = mock.config(Backend::Decisions);
    config.timeout = timeout;
    let mut brain = HostedBrain::new(config, &OWL).unwrap();
    brain.submit(&request(1));
    mock.started.recv_timeout(Duration::from_secs(2)).unwrap();
    for id in 2..20 {
        let start = Instant::now();
        brain.submit(&request(id));
        assert!(start.elapsed() < Duration::from_millis(5));
    }
    let handle = brain.stats_handle();
    let start = Instant::now();
    drop(brain);
    assert!(start.elapsed() < timeout + Duration::from_millis(500));
    assert_eq!(handle.snapshot().failed.get(&Failure::Timeout), Some(&1));
    mock.finish();
    assert_eq!(mock.accepted.load(Ordering::Relaxed), 1);
}

#[test]
fn global_timeout_also_bounds_reading_the_response_body() {
    let timeout = Duration::from_millis(500);
    let mut response = Response::json(responses("3005"));
    response.delay = Duration::from_millis(800);
    response.headers_first = true;
    let mut mock = Mock::new(Backend::Responses, vec![response]);
    let mut config = mock.config(Backend::Responses);
    config.timeout = timeout;
    let mut brain = HostedBrain::new(config, &OWL).unwrap();
    let start = Instant::now();
    brain.submit(&request(3));
    assert!(poll(&mut brain).plan.is_none());
    assert!(start.elapsed() < timeout + Duration::from_millis(500));
    assert_eq!(brain.stats().failed.get(&Failure::Timeout), Some(&1));
    mock.finish();
}

#[test]
fn configuration_errors_and_debug_are_redacted() {
    let mut config = HostedConfig::new(Backend::Decisions, ApiKey::new(KEY));
    config.model = KEY.into();
    config.base_url = format!("http://127.0.0.1/{KEY}");
    config.log_path = Some(KEY.into());
    assert!(!format!("{config:?}").contains(KEY));
    config.timeout = Duration::ZERO;
    let error = HostedBrain::new(config, &OWL).err().unwrap();
    assert_eq!(error, ConfigError::InvalidConfig);
    for error in [
        error,
        ConfigError::MissingKey,
        ConfigError::LogOpen,
        ConfigError::WorkerStart,
    ] {
        assert!(!error.to_string().contains(KEY));
        assert!(!format!("{error:?}").contains(KEY));
    }
    assert!(!format!("{:?}", ApiKey::new(KEY)).contains(KEY));
    assert!(
        ConfigError::MissingKey
            .to_string()
            .contains("OPENAI_API_KEY")
    );
}

#[test]
fn existing_cli_controllers_still_build_without_credentials() {
    for chr in ["c5060", "c1010"] {
        for name in ["lua", "scripted"] {
            let brain = sekiro_brain_net::from_cli(name, None, chr).unwrap();
            assert_eq!(brain.name(), name);
        }
        assert!(sekiro_brain_net::from_cli("unknown", None, chr).is_err());
    }
    assert!(sekiro_brain_net::is_hosted("Decisions") && sekiro_brain_net::is_hosted("responses"));
    assert!(!sekiro_brain_net::is_hosted("scripted"));
}

#[test]
fn the_soldier_plays_his_own_combos_and_sends_his_memory() {
    let mut mock = Mock::new(
        Backend::Decisions,
        vec![Response::json(decisions("3005+3007", 9, 1))],
    );
    let config = mock.config(Backend::Decisions);
    let mut brain = HostedBrain::new(config, &SOLDIER).unwrap();
    let mut memory = Memory::new("c1010");
    memory.fights = 3;
    memory.boss_wins = 2;
    let mut req = request(7);
    req.memory = Some(memory.summary());
    brain.submit(&req);
    let reply = poll(&mut brain);
    assert_eq!(reply.id, 7);
    assert_eq!(reply.plan.unwrap().actions(), [3005, 3007]);
    let body = mock.requests.recv().unwrap();
    let input = body["input"].as_str().unwrap();
    assert!(
        input.contains("\"memory\":{\"fights\":3,\"boss_wins\":2,\"wolf_wins\":0"),
        "{input}"
    );
    let q = &body["questions"][0];
    assert_eq!(q["name"], "boss_action");
    assert!(
        q["instructions"]
            .as_str()
            .unwrap()
            .starts_with("You are an Ashina soldier")
    );
    drop(brain);
    mock.finish();
}

#[test]
fn a_decisions_answer_carries_its_probabilities_as_a_plan_set() {
    let body = json!({"model": "gpt-6-luna", "answers": [{"name": "boss_action", "type": "choice",
        "choice": "3005", "confidence": 0.5, "probabilities": [
            {"value": "3005", "probability": 0.5},
            {"value": "walk_in4>3005", "probability": 0.25},
            {"value": "hold2", "probability": 0.15},
            {"value": "lua", "probability": 0.08},
            {"value": "9999", "probability": 0.02}]}],
        "usage": {"input_tokens": 700, "output_tokens": 3}});
    let mut mock = Mock::new(Backend::Decisions, vec![Response::json(body)]);
    let mut brain = HostedBrain::new(mock.config(Backend::Decisions), &OWL).unwrap();
    assert_eq!(brain.default_mode(), RequestMode::Checkpoints);
    brain.submit(&request(7));
    let reply = poll(&mut brain);
    mock.finish();
    assert_eq!(reply.plan.as_ref().unwrap().actions(), [3005]);
    let set: Vec<_> = reply
        .set
        .iter()
        .map(|w| (w.plan.as_ref().map(|p| p.value()), w.weight))
        .collect();
    // The unknown value is dropped; `lua` is an option that leaves the act to his AI.
    assert_eq!(
        set,
        [
            (Some("3005".to_string()), 0.5),
            (Some("walk_in4>3005".to_string()), 0.25),
            (Some("hold2".to_string()), 0.15),
            (None, 0.08),
        ]
    );
    assert_eq!(brain.usage(), (700, 3));
}

#[test]
fn a_responses_request_leaves_out_what_was_repelled_and_has_no_set() {
    let mut mock = Mock::new(Backend::Responses, vec![Response::json(responses("3005"))]);
    let mut brain = HostedBrain::new(mock.config(Backend::Responses), &OWL).unwrap();
    let mut req = request(3);
    req.excluded = vec!["3003".into(), "walk_in4>3005".into()];
    brain.submit(&req);
    let reply = poll(&mut brain);
    let body = mock.requests.recv().unwrap();
    mock.finish();
    let values = body["text"]["format"]["schema"]["properties"]["action"]["enum"]
        .as_array()
        .unwrap();
    assert!(!values.iter().any(|v| v == "3003" || v == "walk_in4>3005"));
    assert!(values.iter().any(|v| v == "3005"));
    assert!(reply.set.is_empty(), "Responses has no probabilities");
    assert_eq!(reply.plan.unwrap().actions(), [3005]);
}

#[test]
fn warm_up_connects_without_sending_anything() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    let server = thread::spawn(move || {
        listener.set_nonblocking(false).expect("blocking listener");
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut bytes = Vec::new();
        // Reads until the client closes: an empty read means no request was ever sent.
        let _ = stream.read_to_end(&mut bytes);
        tx.send(bytes.len()).unwrap();
    });
    let mut config = HostedConfig::new(Backend::Decisions, ApiKey::new(KEY));
    config.base_url = format!("http://{addr}");
    assert!(config.warm_up, "on by default");
    let brain = HostedBrain::new(config, &OWL).unwrap();
    let sent = rx
        .recv_timeout(Duration::from_secs(3))
        .expect("a connection");
    assert_eq!(sent, 0, "the warm-up sends no bytes, so no API request");
    drop(brain);
    server.join().unwrap();
}

// ---- the coach and the checkpoint chooser ----

use sekiro_brain_net::CoachBackend;
use sekiro_sim::brain::{CoachRequest, SetOffer};

const PLAN: &str = r#"{"version":1,"opener":"3002","sets":[{"name":"a","description":"pressure","plans":[{"plan":"3000+3001","weight":1.0}],"when":null},{"name":"b","description":"bait then thrust","plans":[{"plan":"walk_in4>3013","weight":1.0}],"when":"half_hp"}],"start_set":"a","avoid":[],"notes":"he deflects thrusts"}"#;

fn coach_response(text: &str) -> Value {
    json!({"model": "gpt-6-luna", "status": "completed", "output": [
        {"type": "reasoning", "summary": []},
        {"type": "message", "role": "assistant", "content": [
            {"type": "output_text", "text": text}]}],
        "usage": {"input_tokens": 3100, "output_tokens": 900,
            "output_tokens_details": {"reasoning_tokens": 700}}})
}

fn coach_request(id: u64) -> CoachRequest {
    CoachRequest {
        id,
        fight: 1,
        input: r#"{"fight":1}"#.into(),
    }
}

fn poll_coach(brain: &mut HostedBrain) -> sekiro_sim::brain::CoachReply {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(reply) = brain.poll_coach(0.0) {
            return reply;
        }
        assert!(Instant::now() < deadline, "coach reply deadline exceeded");
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn the_coach_request_has_the_structured_output_shape_and_reports_reasoning_tokens() {
    let mut mock = Mock::at("responses", vec![Response::json(coach_response(PLAN))]);
    let mut config = mock.config(Backend::Coach);
    config.coach_model = "coach-test-model".into();
    let mut brain = HostedBrain::new(config, &SOLDIER).unwrap();
    assert!(brain.coaches());
    brain.submit_coach(&coach_request(5), 0.0);
    let reply = poll_coach(&mut brain);
    mock.finish();
    assert_eq!(reply.id, 5);
    assert_eq!(reply.tokens, [3100, 900, 700]);
    let plan = reply.plan.expect("a plan");
    assert_eq!(plan.opener, "3002");
    assert_eq!(plan.sets.len(), 2);
    assert!(plan.validate(&SOLDIER).usable());
    let body = mock.requests.recv().unwrap();
    assert_eq!(body["model"], "coach-test-model");
    assert_eq!(body["reasoning"], json!({"effort": "low"}));
    assert_eq!(body["store"], false);
    assert_eq!(body["max_output_tokens"], 6000);
    assert_eq!(
        body["input"],
        json!([{"role": "user", "content": r#"{"fight":1}"#}])
    );
    let f = &body["text"]["format"];
    assert_eq!(f["type"], "json_schema");
    assert_eq!(f["name"], "game_plan");
    assert_eq!(f["strict"], true);
    let schema = &f["schema"];
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(
        schema["required"],
        json!([
            "version",
            "opener",
            "sets",
            "start_set",
            "avoid",
            "retry",
            "notes",
            "on_wolf_break",
            "player_principles",
            "boss_principles",
            "boss_bans",
            "boss_phase",
            "counter_weights",
            "checkpoint_prefs",
            "explore",
            "move_controls",
            "lessons",
            "punish_preferences"
        ])
    );
    let choices = schema["$defs"]["choice"]["enum"].as_array().unwrap();
    for want in ["3000+3001", "walk_in4>3013", "hold2", "lua"] {
        assert!(choices.iter().any(|c| c == want), "{want}");
    }
    assert_eq!(
        schema["properties"]["opener"],
        json!({"$ref": "#/$defs/choice"})
    );
    let text = body["instructions"].as_str().unwrap();
    assert!(text.starts_with("You are the coach of an Ashina soldier"));
    assert!(
        text.contains("engaged_time")
            && text.contains("passive above 35")
            && text.contains("once per 5 s"),
        "{text}"
    );
    let stats = brain.stats();
    assert_eq!((stats.coach.requests, stats.coach.answered), (1, 1));
    assert_eq!(stats.coach.reasoning_tokens, 700);
    assert_eq!(stats.requests, 0, "the coach's calls are counted apart");
    let [decide, coach] = brain.call_stats();
    assert_eq!((decide.calls, coach.calls, coach.reasoning), (0, 1, 700));
}

#[test]
fn mid_review_uses_same_wire_format_and_logs_its_phase() {
    let mut mock = Mock::at("responses", vec![Response::json(coach_response(PLAN))]);
    let mut config = mock.config(Backend::Coach);
    let path = std::env::temp_dir().join(format!("sekiro-mid-{}.jsonl", std::process::id()));
    config.log_path = Some(path.clone());
    let mut brain = HostedBrain::new(config, &SOLDIER).unwrap();
    let mut request = coach_request(1);
    request.input = r#"{"phase":"mid","elapsed":45.0}"#.into();
    brain.submit_coach(&request, 45.0);
    assert!(poll_coach(&mut brain).plan.is_some());
    mock.finish();
    drop(brain);
    let body = mock.requests.recv().unwrap();
    assert_eq!(body["input"][0]["content"], request.input);
    let instructions = body["instructions"].as_str().unwrap();
    assert!(instructions.contains("mid-fight review: keep what works"));
    assert!(
        instructions.find("win and win faster").unwrap()
            < instructions.find("then break Wolf posture").unwrap()
    );
    let line: Value = serde_json::from_str(
        std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .last()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(line["kind"], "coach");
    assert_eq!(line["phase"], "mid");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn a_coach_failure_or_refusal_is_an_error_with_no_plan() {
    for (response, want) in [
        (json!({"status": "incomplete", "output": []}), "incomplete"),
        (
            json!({"status": "completed", "output": [{"type": "message", "role": "assistant",
                "content": [{"type": "output_text", "text": "not json"}]}]}),
            "invalid_answer",
        ),
    ] {
        let mut mock = Mock::at("responses", vec![Response::json(response)]);
        let mut brain = HostedBrain::new(mock.config(Backend::Coach), &SOLDIER).unwrap();
        brain.submit_coach(&coach_request(1), 0.0);
        let reply = poll_coach(&mut brain);
        mock.finish();
        assert!(reply.plan.is_none());
        assert_eq!(reply.error.as_deref(), Some(want));
        assert!(!brain.stats().coach.failed.is_empty());
    }
}

fn offers() -> Vec<SetOffer> {
    ["a", "b"]
        .iter()
        .map(|n| SetOffer {
            name: (*n).into(),
            description: format!("set {n}"),
        })
        .collect()
}

#[test]
fn the_checkpoint_chooser_picks_a_set_name_or_keep() {
    let answer = |v: &str| {
        json!({"answers": [{"name": "plan_set", "type": "choice", "choice": v}],
            "usage": {"input_tokens": 600, "output_tokens": 2}})
    };
    let mut mock = Mock::new(
        Backend::Coach,
        vec![
            Response::json(answer("b")),
            Response::json(answer("keep")),
            Response::json(answer("zzz")),
        ],
    );
    let mut brain = HostedBrain::new(mock.config(Backend::Coach), &SOLDIER).unwrap();
    let mut chosen = Vec::new();
    for id in 1..=3 {
        let mut req = request(id);
        req.sets = offers();
        req.active = Some("a".into());
        brain.submit(&req);
        let reply = poll(&mut brain);
        assert!(reply.plan.is_none() && reply.set.is_empty());
        chosen.push(reply.choice);
    }
    mock.finish();
    assert_eq!(
        chosen,
        [Some("b".to_string()), Some("keep".to_string()), None]
    );
    let body = mock.requests.recv().unwrap();
    let q = &body["questions"][0];
    assert_eq!(q["name"], "plan_set");
    assert_eq!(q["type"], "choice");
    assert_eq!(
        q["choices"],
        json!([
            {"value": "a", "description": "set a"},
            {"value": "b", "description": "set b"},
            {"value": "keep", "description": "stay on the current set"},
        ])
    );
    let text = q["instructions"].as_str().unwrap();
    assert!(text.contains("win fast"), "{text}");
    assert!(body["input"].as_str().unwrap().contains("\"active\":\"a\""));
    assert_eq!(brain.stats().failed.get(&Failure::UnknownAction), Some(&1));
}

#[test]
fn without_an_api_key_the_chooser_keeps_the_set_and_sends_nothing() {
    let mut config = HostedConfig::new(Backend::Coach, ApiKey::new("none"));
    config.base_url = "http://127.0.0.1:9".into();
    config.checkpoints = false;
    let mut brain = HostedBrain::new(config, &SOLDIER).unwrap();
    let mut req = request(4);
    req.sets = offers();
    brain.submit(&req);
    let reply = poll(&mut brain);
    assert_eq!((reply.id, reply.choice), (4, None));
    let stats = brain.stats();
    assert_eq!(stats.answered + stats.failed.values().sum::<u64>(), 0);
}

#[test]
fn local_chooser_accepts_no_key_and_never_connects_even_with_warmup_enabled() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut config = HostedConfig::new(Backend::Coach, ApiKey::new(""));
    config.base_url = format!("http://{}", listener.local_addr().unwrap());
    assert!(config.warm_up);
    let mut brain = HostedBrain::new(config, &SOLDIER).unwrap();
    assert_eq!(
        brain.checkpoint_chooser(),
        sekiro_sim::brain::bandit::CheckpointChooser::Local
    );
    let mut req = request(4);
    req.sets = offers();
    brain.submit(&req);
    assert_eq!(poll(&mut brain).id, 4);
    thread::sleep(Duration::from_millis(20));
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(brain.stats().requests, 0);
}

/// A temp directory holding a copy of the fake CLI and its settings.
struct FakeCli {
    dir: std::path::PathBuf,
    exe: std::path::PathBuf,
    log: std::path::PathBuf,
}

impl FakeCli {
    fn new(mode: &str, plan: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "fake-coach-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join(if cfg!(windows) { "fake.exe" } else { "fake" });
        std::fs::copy(env!("CARGO_BIN_EXE_fake_coach_cli"), &exe).unwrap();
        let log = dir.join("calls.jsonl");
        let mut cfg = exe.as_os_str().to_owned();
        cfg.push(".cfg");
        std::fs::write(
            cfg,
            json!({"mode": mode, "plan": plan, "log": log.to_string_lossy()}).to_string(),
        )
        .unwrap();
        Self { dir, exe, log }
    }

    fn calls(&self) -> Vec<Value> {
        std::fs::read_to_string(&self.log)
            .unwrap_or_default()
            // A running fake may still be writing its final record.
            .split_inclusive('\n')
            .filter(|line| line.ends_with('\n'))
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    fn brain(&self, backend: CoachBackend, timeout: Duration) -> HostedBrain {
        let mut config = HostedConfig::new(Backend::Coach, ApiKey::new("none"));
        config.checkpoints = false;
        config.warm_up = false;
        config.coach_backend = backend;
        config.coach_model = "model-x".into();
        config.coach_cmd = Some(self.exe.clone());
        config.coach_timeout = timeout;
        HostedBrain::new(config, &SOLDIER).unwrap()
    }
}

impl Drop for FakeCli {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn args_of(call: &Value) -> Vec<String> {
    call["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn the_claude_coach_runs_the_cli_with_the_prompt_on_stdin_and_reads_its_envelope() {
    let fake = FakeCli::new("ok", PLAN);
    let mut brain = fake.brain(CoachBackend::Claude, Duration::from_secs(20));
    brain.submit_coach(&coach_request(7), 0.0);
    let reply = poll_coach(&mut brain);
    assert_eq!(reply.error, None);
    assert_eq!(reply.plan.unwrap().notes, "he deflects thrusts");
    // 120 input + 300 cached, 45 output; the CLI reports no reasoning tokens.
    assert_eq!(reply.tokens, [420, 45, 0]);
    let calls = fake.calls();
    assert_eq!(calls.len(), 1);
    let args = args_of(&calls[0]);
    let instructions = args[11].clone();
    let mcp_file = args[14].clone();
    assert!(instructions.starts_with("You are the coach"));
    assert_eq!(
        args,
        vec![
            "-p",
            "--model",
            "model-x",
            "--output-format",
            "json",
            "--no-session-persistence",
            "--tools",
            "",
            "--json-schema",
            &args[9],
            "--system-prompt",
            &instructions,
            "--strict-mcp-config",
            "--mcp-config",
            &mcp_file,
            "--setting-sources",
            "",
            "--safe-mode",
            "--effort",
            "low"
        ]
    );
    assert_eq!(calls[0]["mcp"], r#"{"mcpServers":{}}"#);
    let at = args.iter().position(|a| a == "--json-schema").unwrap();
    let schema: Value = serde_json::from_str(&args[at + 1]).unwrap();
    assert_eq!(schema["required"][1], "opener");
    assert!(
        calls[0]["stdin_head"]
            .as_str()
            .unwrap()
            .starts_with("Input (fight review):")
    );
    assert!(
        calls[0]["stdin_len"].as_u64().unwrap() < 300,
        "only the tiny fight summary and output reminder on stdin"
    );
    // Run from a scratch directory, not the repo, and cleaned up afterwards.
    let cwd = std::path::PathBuf::from(calls[0]["cwd"].as_str().unwrap());
    assert!(cwd.to_string_lossy().contains("sekiro-coach-"), "{cwd:?}");
    assert!(!cwd.exists(), "the scratch directory is removed afterwards");
    assert_eq!(brain.stats().coach.input_tokens, 420);
}

#[test]
fn the_codex_coach_reads_the_last_message_file() {
    let fake = FakeCli::new("ok", PLAN);
    let mut brain = fake.brain(CoachBackend::Codex, Duration::from_secs(20));
    brain.submit_coach(&coach_request(8), 0.0);
    let reply = poll_coach(&mut brain);
    assert_eq!(reply.error, None);
    assert_eq!(reply.plan.unwrap().start_set, "a");
    let args = args_of(&fake.calls()[0]);
    // Isolated: none of the user's config, rules, AGENTS.md, memories, plugins or apps, and our
    // instructions replace Codex's system prompt.
    let n = args.len();
    assert_eq!(
        args[..15],
        [
            "exec",
            "--skip-git-repo-check",
            "--ephemeral",
            "--ignore-user-config",
            "--ignore-rules",
            "-s",
            "read-only",
            "-m",
            "model-x",
            "-c",
            "model_reasoning_effort=\"medium\"",
            "-c",
            "project_doc_max_bytes=0",
            "-c",
            &args[14],
        ]
    );
    assert!(args[14].starts_with("model_instructions_file=\""));
    for feature in [
        "memories",
        "apps",
        "plugins",
        "browser_use",
        "computer_use",
        "multi_agent",
    ] {
        assert!(
            args.windows(2)
                .any(|w| w[0] == "--disable" && w[1] == feature),
            "{feature} disabled"
        );
    }
    assert_eq!(args[n - 5], "--output-schema");
    assert_eq!(args[n - 3], "-o");
    assert_eq!(args[n - 1], "-");
    assert_eq!(reply.tokens, [10000, 1000, 0]);
    assert_eq!(brain.stats().coach.input_tokens, 10000);
    let mut config = HostedConfig::new(Backend::Coach, ApiKey::new("none"));
    config.checkpoints = false;
    config.warm_up = false;
    config.coach_backend = CoachBackend::Codex;
    config.coach_model = "configured-model".into();
    config.coach_effort = "high".into();
    config.coach_cmd = Some(fake.exe.clone());
    let mut configured = HostedBrain::new(config, &SOLDIER).unwrap();
    configured.submit_coach(&coach_request(9), 0.0);
    assert!(poll_coach(&mut configured).plan.is_some());
    let calls = fake.calls();
    let args = args_of(calls.last().unwrap());
    assert!(args.iter().any(|a| a == "model_reasoning_effort=\"high\""));
    assert!(
        args.windows(2)
            .any(|a| a[0] == "-m" && a[1] == "configured-model")
    );
}

#[test]
fn a_missing_a_failing_and_a_hanging_cli_leave_no_plan() {
    // Missing.
    let fake = FakeCli::new("ok", PLAN);
    let mut config = HostedConfig::new(Backend::Coach, ApiKey::new("none"));
    config.checkpoints = false;
    config.warm_up = false;
    config.coach_backend = CoachBackend::Claude;
    config.coach_cmd = Some(fake.dir.join("no-such-cli"));
    let mut brain = HostedBrain::new(config, &SOLDIER).unwrap();
    brain.submit_coach(&coach_request(1), 0.0);
    let reply = poll_coach(&mut brain);
    assert!(reply.plan.is_none());
    assert!(reply.error.unwrap().contains("not found on PATH"));
    // Failing exit status.
    let fake = FakeCli::new("fail", PLAN);
    let mut brain = fake.brain(CoachBackend::Claude, Duration::from_secs(20));
    brain.submit_coach(&coach_request(2), 0.0);
    let reply = poll_coach(&mut brain);
    assert!(reply.plan.is_none());
    assert!(reply.error.unwrap().contains("CLI failed"));
    // Garbage in the envelope.
    let fake = FakeCli::new("ok", "not a plan");
    let mut brain = fake.brain(CoachBackend::Claude, Duration::from_secs(20));
    brain.submit_coach(&coach_request(3), 0.0);
    assert!(poll_coach(&mut brain).plan.is_none());
    // Hanging: killed at the deadline.
    let fake = FakeCli::new("hang", PLAN);
    let mut brain = fake.brain(CoachBackend::Claude, Duration::from_millis(600));
    let start = Instant::now();
    brain.submit_coach(&coach_request(4), 0.0);
    let reply = poll_coach(&mut brain);
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "{:?}",
        start.elapsed()
    );
    assert_eq!(reply.error.as_deref(), Some("timeout"));
    assert_eq!(brain.stats().coach.failed.get(&Failure::Timeout), Some(&1));
}

#[test]
fn dropping_the_brain_kills_a_running_cli() {
    let fake = FakeCli::new("hang", PLAN);
    let brain = {
        let mut b = fake.brain(CoachBackend::Claude, Duration::from_secs(60));
        b.submit_coach(&coach_request(1), 0.0);
        b
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while fake.calls().is_empty() {
        assert!(Instant::now() < deadline, "the fake CLI never started");
        thread::sleep(Duration::from_millis(10));
    }
    let start = Instant::now();
    drop(brain);
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "{:?}",
        start.elapsed()
    );
}

#[test]
fn the_coach_mode_checkpoint_request_is_tiny() {
    let mut mock = Mock::new(
        Backend::Coach,
        vec![Response::json(json!({
            "answers": [{"name": "plan_set", "type": "choice", "choice": "keep"}],
            "usage": {"input_tokens": 150, "output_tokens": 2}}))],
    );
    let mut brain = HostedBrain::new(mock.config(Backend::Coach), &SOLDIER).unwrap();
    // Four sets with the longest descriptions the game plan allows.
    let mut req = request(9);
    req.sets = ["press", "bait", "rush", "wait_then_strike"]
        .iter()
        .map(|n| SetOffer {
            name: (*n).into(),
            description: "x".repeat(140),
        })
        .collect();
    let plan = sekiro_sim::brain::GamePlan {
        notes: "Pressure him; answer his counter rather than repeat a deflected combo. ".repeat(6),
        sets: vec![sekiro_sim::brain::coach::PlanSet {
            name: "bait".into(),
            description: "A short defensive beat, then punish his recovery".into(),
            ..Default::default()
        }],
        principles: vec![
            "Wolf waits for slow setups".into(),
            "He deflects predictable extensions".into(),
        ],
        move_controls: vec![sekiro_sim::brain::move_bandit::Control {
            plan: "3000".into(),
            confidence: 0.85,
            lock: true,
        }],
        ..Default::default()
    };
    let mut input = json!({"why":"half_hp","set":"bait","boss_hp":0.48,
        "boss_posture":0.2,"wolf_hp":0.91,"wolf_posture":0.12,"dist":3.4});
    input["coach"] = plan.checkpoint_context(
        Some("bait"),
        &["Attack until deflected, then answer the counter".into()],
        &[
            "Old setup failed".into(),
            "Punish recovery with short slashes".into(),
        ],
    );
    let brief = input.to_string();
    req.brief = Some(brief.clone());
    // Everything the full request would carry stays out of the body.
    req.memory = Some(Memory::new("c1010").summary());
    brain.submit(&req);
    let reply = poll(&mut brain);
    mock.finish();
    assert_eq!(reply.choice.as_deref(), Some("keep"));
    let body = mock.requests.recv().unwrap();
    let bytes = serde_json::to_string(&body).unwrap().len();
    eprintln!("coach-mode checkpoint request body: {bytes} bytes");
    // Raised from 1200 for 768 bytes of coach reasoning, JSON escaping and its instructions.
    assert!(bytes < 2600, "{bytes} bytes: {body}");
    assert_eq!(body["input"], brief);
    assert!(brief.len() < 1000, "bounded reasoning plus scalars");
    let text = body["questions"][0]["instructions"].as_str().unwrap();
    assert!(text.len() < 500 && !text.contains("tally"), "{text}");
    assert!(
        text.contains("damage/min")
            && text.contains("low boss damage")
            && text.contains("Passive>35% fails")
            && text.contains("Counters/Mikiri/jump/deflect/guard fail")
            && text.contains("Compare arsenal rates after 3 uses")
            && text.contains("choose set")
            && text.contains("follow coach notes, intent, principles and newest lessons")
            && text.contains("Adapt the coach's plan to the moment")
    );
    // Input tokens are counted per call kind.
    let [decide, coach] = brain.call_stats();
    assert_eq!((decide.calls, decide.input, coach.calls), (1, 150, 0));
}

#[test]
fn subscription_tokens_are_a_proxy_and_stop_calls_without_any_dollars() {
    for backend in [CoachBackend::Codex, CoachBackend::Claude] {
        let fake = FakeCli::new("ok", PLAN);
        let mut config = HostedConfig::new(Backend::Coach, ApiKey::new("none"));
        config.checkpoints = false;
        config.warm_up = false;
        config.coach_backend = backend;
        config.coach_model = backend.default_model().into();
        config.coach_cmd = Some(fake.exe.clone());
        config.token_budget = Some(1);
        config.budget_usd = Some(0.000001);
        let log = fake.dir.join("telemetry.jsonl");
        config.log_path = Some(log.clone());
        let mut brain = HostedBrain::new(config, &SOLDIER).unwrap();
        brain.submit_coach(&coach_request(1), 0.0);
        assert!(poll_coach(&mut brain).plan.is_some());
        let costs = brain.costs();
        let tokens = if backend == CoachBackend::Codex {
            11000
        } else {
            465
        };
        assert_eq!(costs.totals.plan_tokens, tokens);
        assert_eq!(costs.totals.tokens(), tokens);
        assert_eq!(costs.totals.api_tokens, 0);
        assert_eq!(costs.totals.billed_usd, 0.0);
        assert_eq!(costs.totals.unknown_billed, 0);
        assert!(!costs.totals.line("session").contains('$'));
        assert!(costs.budget_notice.unwrap().contains("token budget"));
        brain.submit_coach(&coach_request(2), 0.0);
        assert!(poll_coach(&mut brain).plan.is_none());
        assert_eq!(fake.calls().len(), 1);
        let text = std::fs::read_to_string(log).unwrap();
        let line: Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(line["billed"], false);
        assert!(line["usd_estimate"].is_null());
        assert!(line["cost_usd"].is_null());
        assert_eq!(line["sekiro.tokens.proxy"], true);
        assert_eq!(line["gen_ai.request.model"], backend.default_model());
        assert_eq!(
            line["gen_ai.usage.input_tokens"],
            line["usage"]["input_tokens"]
        );
        assert_eq!(
            line["sekiro.usage.cached_input_tokens"],
            if backend == CoachBackend::Codex {
                2000
            } else {
                300
            }
        );
        eprintln!("sample telemetry: {text}");
    }
}

#[test]
fn subscription_usage_does_not_exhaust_a_positive_usd_budget() {
    let fake = FakeCli::new("ok", PLAN);
    let mut config = HostedConfig::new(Backend::Coach, ApiKey::new("none"));
    config.checkpoints = false;
    config.warm_up = false;
    config.coach_backend = CoachBackend::Codex;
    config.coach_cmd = Some(fake.exe.clone());
    config.budget_usd = Some(0.000001);
    let mut brain = HostedBrain::new(config, &SOLDIER).unwrap();
    for id in 1..=2 {
        brain.submit_coach(&coach_request(id), 0.0);
        assert!(poll_coach(&mut brain).plan.is_some());
        assert!(!brain.budget_exhausted());
    }
    assert_eq!(fake.calls().len(), 2);
    assert_eq!(brain.costs().totals.plan_tokens, 22000);
}

#[test]
fn unusable_cli_plans_still_spend_the_token_budget() {
    for backend in [CoachBackend::Codex, CoachBackend::Claude] {
        let fake = FakeCli::new("ok", "not a plan");
        let mut config = HostedConfig::new(Backend::Coach, ApiKey::new("none"));
        config.checkpoints = false;
        config.warm_up = false;
        config.coach_backend = backend;
        config.coach_cmd = Some(fake.exe.clone());
        config.token_budget = Some(1);
        let mut brain = HostedBrain::new(config, &SOLDIER).unwrap();
        brain.submit_coach(&coach_request(1), 0.0);
        assert!(poll_coach(&mut brain).plan.is_none());
        assert_eq!(
            brain.costs().totals.plan_tokens,
            if backend == CoachBackend::Codex {
                11000
            } else {
                465
            }
        );
        assert!(brain.budget_exhausted());
        brain.submit_coach(&coach_request(2), 0.0);
        assert!(poll_coach(&mut brain).plan.is_none());
        assert_eq!(fake.calls().len(), 1);
    }
}

#[test]
fn api_budget_stops_queued_calls_and_unknown_prices_fail_closed() {
    for unknown in [false, true] {
        let mut mock = Mock::new(
            Backend::Decisions,
            vec![Response::json(decisions("3005", 250, 2))],
        );
        let mut config = mock.config(Backend::Decisions);
        config.budget_usd = Some(0.000025);
        if unknown {
            config.model = "unknown-model".into();
        }
        let mut brain = HostedBrain::new(config, &OWL).unwrap();
        // Queue both before the first finishes: admission must be checked inside the worker.
        brain.submit(&request(1));
        brain.submit(&request(2));
        assert!(poll(&mut brain).plan.is_some());
        assert!(poll(&mut brain).plan.is_none());
        mock.finish();
        assert_eq!(mock.requests.try_iter().count(), 1);
        let costs = brain.costs();
        assert_eq!(costs.totals.calls, 1);
        assert_eq!(costs.totals.api_tokens, 252);
        assert_eq!(costs.totals.plan_tokens, 0);
        assert!(brain.budget_exhausted());
        if unknown {
            assert_eq!(costs.totals.unknown_billed, 1);
            assert!(costs.totals.line("session").contains("unknown"));
        } else {
            assert!((costs.totals.billed_usd - 0.000025).abs() < 1e-12);
        }
    }
}
