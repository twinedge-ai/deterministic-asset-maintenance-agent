use asset_agent_core::{
    config::validate_snapshot_design_specs,
    features::analyze_cavitation,
    load_model_config,
    replay::{read_trace, select_as_of, write_jsonl},
    validate_source_bundle,
};
use asset_agent_memory::{FeedbackInput, LearningSettings, MemoryStore};
use asset_agent_opcua::frame_buffer::FrameBuffer;
use asset_agent_opcua::live_client::{read_live_snapshot, LiveOpcUaConfig};
use asset_agent_opcua::output_server::{AgentOutputConfig, AgentOutputState};
use std::{env, path::PathBuf};
use tokio::runtime::Runtime;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    let command = args.get(1).map(String::as_str).unwrap_or("config-check");
    match command {
        "config-check" => config_check(),
        "replay" => replay_command(&args[2..]),
        "improvement-demo" => improvement_demo_command(&args[2..]),
        "output-smoke" => output_smoke_command(&args[2..]),
        "live" => live_command(&args[2..]),
        other => anyhow::bail!("unknown command: {other}"),
    }
}

fn replay_command(args: &[String]) -> anyhow::Result<()> {
    let trace = required_flag(args, "--trace")?;
    let output = required_flag(args, "--output")?;
    let selector = optional_flag(args, "--as-of").unwrap_or_else(|| "trace:last".to_string());
    let window_seconds = optional_flag(args, "--window-seconds")
        .map(|value| value.parse::<i64>())
        .transpose()?
        .unwrap_or(10);
    let root = env::var("ASSET_AGENT_PROJECT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    let asset_id = env::var("ASSET_AGENT_ASSET_ID").unwrap_or_else(|_| "hp_pump_1".to_string());
    let config = load_model_config(&root, &asset_id)?;

    let snapshots = read_trace(&trace)?;
    let selected = select_as_of(&snapshots, &selector)?.clone();
    let buffer = FrameBuffer::from_snapshots(10_000, snapshots);
    let window = buffer.window_ending_at(&selected.as_of_timestamp, window_seconds)?;
    let validation_errors = validate_snapshot_design_specs(&selected, &config);
    let analysis = analyze_cavitation(&selected, &config);
    let record = serde_json::json!({
        "ok": validation_errors.is_empty(),
        "mode": "replay",
        "selector": selector,
        "window_seconds": window_seconds,
        "window_sample_count": window.len(),
        "snapshot": selected,
        "design_spec_validation_errors": validation_errors,
        "analysis": analysis
    });
    write_jsonl(&output, &[record])?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "ok": validation_errors.is_empty(),
            "output": output,
            "window_sample_count": window.len(),
            "risk_level": analysis.risk_level
        }))?
    );
    Ok(())
}

fn output_smoke_command(args: &[String]) -> anyhow::Result<()> {
    let trace = optional_flag(args, "--trace")
        .unwrap_or_else(|| "traces/hp_pump_1_low_suction_pressure.jsonl".to_string());
    let selector = optional_flag(args, "--as-of").unwrap_or_else(|| "trace:last".to_string());
    let root = env::var("ASSET_AGENT_PROJECT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    let asset_id = env::var("ASSET_AGENT_ASSET_ID").unwrap_or_else(|_| "hp_pump_1".to_string());
    let config = load_model_config(&root, &asset_id)?;
    let snapshots = read_trace(&trace)?;
    let selected = select_as_of(&snapshots, &selector)?;
    let analysis = analyze_cavitation(selected, &config);
    let output_state = AgentOutputState::new(AgentOutputConfig::default(), asset_id.clone());
    let before = output_state.snapshot()?;
    let after = output_state.publish_analysis(Some("output-smoke"), &analysis, &config.versions)?;

    anyhow::ensure!(
        !before.initialized,
        "output state should start uninitialized"
    );
    anyhow::ensure!(after.initialized, "output state was not initialized");
    anyhow::ensure!(
        after.sequence == 1,
        "unexpected output sequence {}",
        after.sequence
    );
    anyhow::ensure!(
        after
            .nodes
            .contains_key(&format!("agent:{asset_id}.decision.recommended_action")),
        "recommended action output node missing"
    );

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "ok": true,
            "asset_id": asset_id,
            "endpoint": after.endpoint,
            "namespace_uri": after.namespace_uri,
            "sequence": after.sequence,
            "node_count": after.node_count,
            "source_case_id": after.source_case_id,
            "risk_level": analysis.risk_level,
            "recommended_action": analysis.recommendation.recommended_action
        }))?
    );
    Ok(())
}

fn live_command(args: &[String]) -> anyhow::Result<()> {
    let endpoint =
        optional_flag(args, "--endpoint").unwrap_or_else(|| "opc.tcp://127.0.0.1:4840".to_string());
    let namespace_uri = optional_flag(args, "--namespace-uri")
        .unwrap_or_else(|| "urn:twinedge:opcua-edge".to_string());
    let asset_id = optional_flag(args, "--asset")
        .or_else(|| env::var("ASSET_AGENT_ASSET_ID").ok())
        .unwrap_or_else(|| "hp_pump_1".to_string());
    let window_seconds = optional_flag(args, "--window-seconds")
        .map(|value| value.parse::<u32>())
        .transpose()?
        .unwrap_or(10);
    let record_path = optional_flag(args, "--record");
    let root = env::var("ASSET_AGENT_PROJECT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    let config = load_model_config(&root, &asset_id)?;

    let runtime = Runtime::new()?;
    let live = runtime.block_on(read_live_snapshot(&LiveOpcUaConfig {
        endpoint: endpoint.clone(),
        namespace_uri: namespace_uri.clone(),
        asset_id: asset_id.clone(),
        window_seconds,
    }))?;

    let validation_errors = validate_snapshot_design_specs(&live.snapshot, &config);
    let analysis = analyze_cavitation(&live.snapshot, &config);
    let ok = validation_errors.is_empty()
        && analysis.features.validation_errors.is_empty()
        && analysis.features.missing_inputs.is_empty();
    let record = serde_json::json!({
        "ok": ok,
        "mode": "live_opcua",
        "endpoint": endpoint,
        "namespace_uri": namespace_uri,
        "namespace_resolution": &live.namespace,
        "nodes_read": &live.nodes_read,
        "snapshot": &live.snapshot,
        "design_spec_validation_errors": validation_errors,
        "analysis": &analysis
    });

    if let Some(path) = record_path.as_ref() {
        write_jsonl(path, std::slice::from_ref(&record))?;
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "ok": ok,
            "mode": "live_opcua",
            "endpoint": record["endpoint"],
            "namespace_index": live.namespace.namespace_index,
            "asset_id": asset_id,
            "risk_level": analysis.risk_level,
            "npsh_margin_m": analysis.features.npsh_margin_m,
            "nodes_read": live.nodes_read.len(),
            "record": record_path
        }))?
    );
    Ok(())
}

fn improvement_demo_command(args: &[String]) -> anyhow::Result<()> {
    let db_path = optional_flag(args, "--db")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/asset-agent-phase5-demo.sqlite3"));
    let trace = optional_flag(args, "--trace")
        .unwrap_or_else(|| "traces/hp_pump_1_high_vibration.jsonl".to_string());
    let selector = optional_flag(args, "--as-of").unwrap_or_else(|| "trace:last".to_string());
    let feedback_count = optional_flag(args, "--feedback-count")
        .map(|value| value.parse::<usize>())
        .transpose()?
        .unwrap_or(3);
    let solved_item_id =
        optional_flag(args, "--solved-item-id").unwrap_or_else(|| "check_suction_strainer".into());
    let root = env::var("ASSET_AGENT_PROJECT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    let asset_id = env::var("ASSET_AGENT_ASSET_ID").unwrap_or_else(|_| "hp_pump_1".to_string());
    let config = load_model_config(&root, &asset_id)?;
    let settings = LearningSettings::from_model_config(&config);
    let snapshots = read_trace(&trace)?;
    let selected = select_as_of(&snapshots, &selector)?.clone();
    let analysis = analyze_cavitation(&selected, &config);
    let store = MemoryStore::open(&db_path)?;
    let case_id = store.store_analysis_case(&selected, &analysis)?;

    let mut last_feedback = None;
    for index in 0..feedback_count {
        last_feedback = Some(store.submit_feedback(
            &case_id,
            FeedbackInput {
                confirmed_cavitation: true,
                solved_item_id: Some(solved_item_id.clone()),
                false_alarm: false,
                missed_cavitation: false,
                notes: Some(format!("phase5_demo_feedback_{}", index + 1)),
                post_action: None,
            },
            &settings,
        )?);
    }

    let learning = store.learning_summary(&settings)?;
    let top_item = learning.checklist.first();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "ok": true,
            "db_path": db_path,
            "case_id": case_id,
            "trace": trace,
            "feedback_count": feedback_count,
            "risk_level": analysis.risk_level,
            "npsh_margin_m": analysis.features.npsh_margin_m,
            "top_checklist_item": top_item,
            "thresholds": learning.thresholds,
            "calibration": learning.calibration,
            "last_feedback": last_feedback
        }))?
    );
    Ok(())
}

fn config_check() -> anyhow::Result<()> {
    let root = env::var("ASSET_AGENT_PROJECT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    let asset_id = env::var("ASSET_AGENT_ASSET_ID").unwrap_or_else(|_| "hp_pump_1".to_string());
    let config = load_model_config(root, &asset_id)?;
    let issues = validate_source_bundle(&config);
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "ok": issues.is_empty(),
            "asset_id": config.asset.asset_id,
            "source_bundle_issues": issues
        }))?
    );
    Ok(())
}

fn required_flag(args: &[String], name: &str) -> anyhow::Result<String> {
    optional_flag(args, name).ok_or_else(|| anyhow::anyhow!("missing required flag {name}"))
}

fn optional_flag(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}
