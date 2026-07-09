//! End-to-end tests for the inference engine foundation (Phase 41).
//! Runs on CPU with the weight-free `MockModel` — no GPU/weights required.

use justapi_inference::{
    AcceptanceStats, Engine, EngineDevice, FinishReason, MockModel, Model, ModelError,
    SamplingParams,
};

#[test]
fn mock_tokenize_detokenize_roundtrip() {
    let text = "hello";
    let ids = MockModel::tokenize(text);
    assert_eq!(ids, vec![104, 101, 108, 108, 111]);
    assert_eq!(MockModel::detokenize(&ids), text);
}

#[test]
fn mock_generate_respects_max_tokens() {
    let model = MockModel::new(26);
    let params = SamplingParams {
        max_tokens: 10,
        ..Default::default()
    };
    let count = std::cell::Cell::new(0usize);
    let reason = model
        .generate(&[0], &params, &|t| {
            count.set(count.get() + 1);
            assert!(!t.text.is_empty());
            true
        })
        .unwrap();
    assert_eq!(count.get(), 10);
    assert_eq!(reason, FinishReason::Length);
}

#[test]
fn mock_generate_stops_on_stop_token() {
    let model = MockModel::new(26);
    // Starting from prompt [0], next is 1 ('b'); stop when we produce token 3.
    let params = SamplingParams {
        max_tokens: 100,
        stop_tokens: vec![3],
        ..Default::default()
    };
    let produced: Vec<u32> = Vec::new();
    let produced = std::sync::Mutex::new(produced);
    let reason = model
        .generate(&[0], &params, &|t| {
            produced.lock().unwrap().push(t.id);
            true
        })
        .unwrap();
    assert_eq!(reason, FinishReason::Stop);
    assert!(produced.lock().unwrap().contains(&3));
}

#[test]
fn device_discover_includes_cpu() {
    let devices = EngineDevice::discover();
    assert!(devices.contains(&EngineDevice::Cpu));
}

#[test]
fn engine_registers_and_lists_mock() {
    let engine = Engine::new(EngineDevice::Cpu).unwrap();
    assert!(engine.list_models().is_empty());
    engine.register_mock("demo");
    let models = engine.list_models();
    assert_eq!(models, vec!["demo".to_string()]);
}

#[tokio::test]
async fn engine_generate_streams_tokens() {
    let engine = Engine::new(EngineDevice::Cpu).unwrap();
    engine.register_mock("demo");
    let params = SamplingParams {
        max_tokens: 8,
        ..Default::default()
    };
    let mut rx = engine.generate("demo", &[0], params).unwrap();

    let mut count = 0;
    let mut last_reason = None;
    while let Some(tok) = rx.recv().await {
        count += 1;
        last_reason = tok.finish_reason;
    }
    assert_eq!(count, 8);
    // Stream ended by length; no final token carries a finish reason in mock.
    assert!(last_reason.is_none() || last_reason == Some(FinishReason::Length));
}

#[test]
fn engine_generate_unknown_model_errors() {
    let engine = Engine::new(EngineDevice::Cpu).unwrap();
    let err = engine.generate("nope", &[0], SamplingParams::default());
    assert!(matches!(err, Err(ModelError::NotFound(_))));
}

#[test]
#[cfg(not(feature = "real"))]
fn load_without_real_feature_is_explicit_error() {
    let engine = Engine::new(EngineDevice::Cpu).unwrap();
    // Without the `real` feature this must fail loudly, not silently no-op.
    let err = engine.load("m", std::path::Path::new("/tmp/nonexistent-model"));
    assert!(matches!(err, Err(ModelError::FeatureRequired("real"))));
}

#[test]
#[cfg(feature = "real")]
fn load_with_real_feature_attempts_load() {
    let engine = Engine::new(EngineDevice::Cpu).unwrap();
    // With the `real` feature, load attempts to read the directory and fails
    // with a concrete error (not FeatureRequired).
    let err = engine.load("m", std::path::Path::new("/tmp/nonexistent-model"));
    assert!(err.is_err());
    assert!(!matches!(err, Err(ModelError::FeatureRequired(_))));
}

#[test]
fn engine_speculative_decoding_end_to_end() {
    // Register a speculative model served through the normal Engine path:
    // target == draft (perfect draft) → acceptance rate 1.0 and gamma+1 tokens
    // emitted per verify step. Streamed over the same tokio mpsc as plain gen.
    let engine = Engine::new(EngineDevice::Cpu).unwrap();
    let target = std::sync::Arc::new(MockModel::new(32));
    let draft = std::sync::Arc::new(MockModel::new(32));
    engine.register_speculative("spec", target, draft, 4, 7);

    let params = SamplingParams {
        max_tokens: 20,
        temperature: 0.0,
        ..Default::default()
    };

    // Collect the speculatively-decoded stream.
    let mut spec_rx = engine.generate("spec", &[0], params.clone()).unwrap();
    let mut spec_ids = Vec::new();
    while let Some(tok) = spec_rx.blocking_recv() {
        spec_ids.push(tok.id);
    }
    assert_eq!(spec_ids.len(), 20);

    // Correctness: must equal plain target decode (speculation is lossless).
    engine.register("__plain", std::sync::Arc::new(MockModel::new(32)));
    let mut plain_rx = engine.generate("__plain", &[0], params).unwrap();
    let mut plain_ids = Vec::new();
    while let Some(tok) = plain_rx.blocking_recv() {
        plain_ids.push(tok.id);
    }
    assert_eq!(
        spec_ids, plain_ids,
        "speculation must not change the output"
    );
}

#[test]
fn acceptance_rate_perfect_draft_is_one() {
    // Direct verify of AcceptanceStats with a perfect draft.
    let target = MockModel::new(32);
    let draft = MockModel::new(32);
    let params = SamplingParams {
        max_tokens: 30,
        temperature: 0.0,
        ..Default::default()
    };
    let out = std::cell::RefCell::new(Vec::new());
    let (_finish, stats) =
        justapi_inference::speculative_generate(&target, &draft, &[0], &params, 4, 11, &|t| {
            out.borrow_mut().push(t.id);
            true
        })
        .unwrap();
    assert!((stats.acceptance_rate() - 1.0).abs() < 1e-9);
    assert_eq!(stats.tokens_emitted(), out.borrow().len());
    let _ = AcceptanceStats::default();
}
