"use client";

import {
  Activity,
  AlertTriangle,
  BrainCircuit,
  ChevronRight,
  CircuitBoard,
  DatabaseZap,
  Gauge,
  MessageSquareText,
  RefreshCw,
  Route,
  Send,
  Server,
  ShieldCheck,
  Sparkles,
  Wrench,
  type LucideIcon,
} from "lucide-react";
import type { FormEvent, ReactNode } from "react";
import { useEffect, useMemo, useRef, useState } from "react";
import { PumpScene } from "../components/PumpScene";
import {
  API_BASE,
  AnalyzeResponse,
  AgentOutputSnapshot,
  ChecklistItem,
  ChecklistResponse,
  FeedbackResponse,
  LlmExplainResponse,
  LlmStatus,
  OpcStatus,
  RiskLevel,
  SnapshotResponse,
  analyzeSnapshot,
  explainCase,
  getChecklist,
  getLearningSummary,
  getLlmStatus,
  getOpcStatus,
  getSnapshot,
  submitFeedback,
  type DerivedFeatures,
  type LearningSummaryResponse,
  type ObservationSnapshot,
} from "../lib/api";

type FeedbackFormState = {
  confirmed_cavitation: boolean;
  false_alarm: boolean;
  missed_cavitation: boolean;
  notes: string;
};

type DashboardState = {
  status: OpcStatus | null;
  snapshot: SnapshotResponse | null;
  analysis: AnalyzeResponse | null;
  output: AgentOutputSnapshot | null;
  learning: LearningSummaryResponse | null;
  checklist: ChecklistResponse | null;
  llmStatus: LlmStatus | null;
};

const EMPTY_STATE: DashboardState = {
  status: null,
  snapshot: null,
  analysis: null,
  output: null,
  learning: null,
  checklist: null,
  llmStatus: null,
};

const FLOW_STEPS: Array<{
  label: string;
  icon: LucideIcon;
  targetId: string;
  detail: (state: DashboardState) => string;
}> = [
  {
    label: "OPC UA",
    icon: Server,
    targetId: "data-contract-panel",
    detail: (state) => state.status?.mode ?? "offline",
  },
  {
    label: "Snapshot",
    icon: CircuitBoard,
    targetId: "live-snapshot-panel",
    detail: (state) => state.snapshot?.selector ?? "pending",
  },
  {
    label: "NPSH",
    icon: Gauge,
    targetId: "deterministic-state-panel",
    detail: (state) => formatValue(state.analysis?.result.features.npsh_margin_m, "m"),
  },
  {
    label: "Belief",
    icon: BrainCircuit,
    targetId: "belief-panel",
    detail: (state) => percent(state.analysis?.result.belief.mean_damage),
  },
  {
    label: "Policy",
    icon: ShieldCheck,
    targetId: "policy-panel",
    detail: (state) => titleize(state.analysis?.result.recommendation.lowest_cost_action),
  },
  {
    label: "Explain",
    icon: MessageSquareText,
    targetId: "explanation-panel",
    detail: (state) => state.analysis?.result.explanation.source ?? "template",
  },
  {
    label: "Publish",
    icon: Route,
    targetId: "data-contract-panel",
    detail: (state) => (state.output?.initialized ? `seq ${state.output.sequence}` : "ready"),
  },
  {
    label: "Learn",
    icon: DatabaseZap,
    targetId: "learning-panel",
    detail: (state) => `${state.learning?.summary.checklist.length ?? 0} checks`,
  },
];

export default function Page() {
  const [state, setState] = useState<DashboardState>(EMPTY_STATE);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [llmLoading, setLlmLoading] = useState(false);
  const [llmResult, setLlmResult] = useState<LlmExplainResponse | null>(null);
  const [feedbackResult, setFeedbackResult] = useState<FeedbackResponse | null>(null);
  const [feedbackSubmitting, setFeedbackSubmitting] = useState(false);
  const [selectedChecklistId, setSelectedChecklistId] = useState("");
  const [feedback, setFeedback] = useState<FeedbackFormState>({
    confirmed_cavitation: true,
    false_alarm: false,
    missed_cavitation: false,
    notes: "",
  });
  const loadedRef = useRef(false);

  const loadDashboard = async (background = false) => {
    setError(null);
    if (background) {
      setRefreshing(true);
    } else {
      setLoading(true);
    }

    try {
      const [status, snapshot, learning, checklist, llmStatus] = await Promise.all([
        getOpcStatus(),
        getSnapshot("hp_pump_1"),
        getLearningSummary(),
        getChecklist(),
        getLlmStatus(),
      ]);
      const analysis = await analyzeSnapshot(snapshot.snapshot);
      setState({ status, snapshot, analysis, output: analysis.agent_output, learning, checklist, llmStatus });
      setSelectedChecklistId((current) => current || checklist.items[0]?.item_id || "");
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Dashboard load failed");
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  };

  useEffect(() => {
    if (loadedRef.current) {
      return;
    }
    loadedRef.current = true;
    void loadDashboard();
  }, []);

  const analysis = state.analysis?.result ?? null;
  const snapshot = state.snapshot?.snapshot ?? null;
  const riskLevel = analysis?.risk_level ?? "UNKNOWN";
  const features = analysis?.features ?? null;
  const checklist = state.checklist?.items ?? [];
  const recommendedChecks = mergeRecommendedChecks(analysis?.recommended_checklist ?? [], checklist);
  const costRows = useMemo(
    () => buildCostRows(analysis?.counterfactual_costs ?? {}),
    [analysis?.counterfactual_costs],
  );
  const schematicMeasurements = useMemo(
    () => buildSchematicMeasurements(snapshot, features),
    [snapshot, features],
  );
  const topCost = costRows[0]?.cost ?? 0;
  const llmAvailable = state.llmStatus?.llm_available === true;

  const submitOperatorFeedback = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const caseId = state.analysis?.case_id;
    if (!caseId) {
      return;
    }
    setFeedbackSubmitting(true);
    setError(null);
    try {
      const result = await submitFeedback(caseId, {
        confirmed_cavitation: feedback.confirmed_cavitation,
        false_alarm: feedback.false_alarm,
        missed_cavitation: feedback.missed_cavitation,
        solved_item_id: selectedChecklistId || undefined,
        notes: feedback.notes || undefined,
        post_action: {
          source: "operator_dashboard",
          risk_level: riskLevel,
          recommended_action: analysis?.recommended_action,
        },
      });
      const [learning, checklistResponse] = await Promise.all([getLearningSummary(), getChecklist()]);
      setFeedbackResult(result);
      setState((current) => ({ ...current, learning, checklist: checklistResponse }));
      setFeedback((current) => ({ ...current, notes: "" }));
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Feedback submit failed");
    } finally {
      setFeedbackSubmitting(false);
    }
  };

  const runLlmExplanation = async () => {
    const caseId = state.analysis?.case_id;
    if (!caseId || !llmAvailable) {
      return;
    }
    setLlmLoading(true);
    setError(null);
    try {
      setLlmResult(await explainCase(caseId));
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "mistral.rs explanation failed");
    } finally {
      setLlmLoading(false);
    }
  };

  return (
    <main className={`dashboard risk-${riskLevel.toLowerCase()}`}>
      <header className="topbar">
        <div>
          <p className="eyebrow">HP Pump 1</p>
          <h1>AssetPilot</h1>
          <p className="topbar-subtitle">
            Deterministic self-improving maintenance agent for industrial assets
          </p>
        </div>
        <div className="topbar-actions">
          <StatusChip
            icon={Server}
            label={state.status?.mode ?? "API"}
            value={state.status?.connected ? "live" : "replay"}
            tone={state.status ? "ok" : "idle"}
          />
          <StatusChip icon={ShieldCheck} label="core" value="deterministic" tone="ok" />
          <StatusChip
            icon={Sparkles}
            label="mistral.rs"
            value={state.llmStatus?.source ?? "checking"}
            tone={llmAvailable ? "ok" : "idle"}
          />
          <StatusChip
            icon={Route}
            label="output"
            value={state.output?.initialized ? `seq ${state.output.sequence}` : "ready"}
            tone={state.output ? "ok" : "idle"}
          />
          <button className="icon-button" type="button" onClick={() => void loadDashboard(true)}>
            <RefreshCw size={17} aria-hidden="true" />
            <span>{refreshing ? "Refreshing" : "Refresh"}</span>
          </button>
        </div>
      </header>

      {error ? (
        <section className="error-strip" role="alert">
          <AlertTriangle size={18} aria-hidden="true" />
          <span>{error}</span>
          <code>{API_BASE}</code>
        </section>
      ) : null}

      <section className="scene-band" aria-busy={loading}>
        <div className="scene-status">
          <p className="eyebrow">Cavitation risk</p>
          <div className={`risk-badge risk-badge-${riskLevel.toLowerCase()}`}>{riskLevel}</div>
          <div className="scene-metrics">
            <Metric label="NPSHA" value={formatValue(features?.npsha_m, "m")} />
            <Metric label="NPSHR" value={formatValue(features?.npshr_m, "m")} />
            <Metric label="Margin" value={formatValue(features?.npsh_margin_m, "m")} />
            <Metric label="Evidence" value={percent(features?.cavitation_evidence_score)} />
          </div>
        </div>
        <PumpScene
          riskLevel={riskLevel}
          npshMarginM={features?.npsh_margin_m ?? null}
          flowRateM3h={snapshot?.flow_rate_m3h ?? null}
          cavitationScore={features?.cavitation_evidence_score ?? 0}
          measurements={schematicMeasurements}
          modeLabel={state.status?.mode ?? "replay"}
          windowLabel={`${snapshot?.window_seconds ?? 0}s / ${snapshot?.sample_count ?? 0} samples`}
        />
      </section>

      <section className="flow-rail" aria-label="agent flow">
        {FLOW_STEPS.map((step, index) => {
          const Icon = step.icon;
          return (
            <button
              aria-label={`Show ${step.label}`}
              className="flow-step"
              key={step.label}
              onClick={() => scrollToPanel(step.targetId)}
              type="button"
            >
              <div className="flow-node">
                <Icon size={18} aria-hidden="true" />
              </div>
              <div>
                <strong>{step.label}</strong>
                <span>{step.detail(state)}</span>
              </div>
              {index < FLOW_STEPS.length - 1 ? <ChevronRight className="flow-chevron" size={17} /> : null}
            </button>
          );
        })}
      </section>

      <section className="workspace-grid">
        <Panel id="live-snapshot-panel" title="Live Snapshot" icon={Activity}>
          <dl className="metric-list">
            <DataRow label="Asset" value={snapshot?.asset_id ?? "hp_pump_1"} />
            <DataRow label="As of" value={snapshot?.as_of_timestamp ?? "pending"} />
            <DataRow label="Window" value={`${snapshot?.window_seconds ?? 0}s / ${snapshot?.sample_count ?? 0} samples`} />
            <DataRow label="Flow" value={formatValue(snapshot?.flow_rate_m3h, "m3/h")} />
            <DataRow label="Suction" value={formatValue(snapshot?.suction_pressure_bar, "bar")} />
            <DataRow label="Discharge" value={formatValue(snapshot?.discharge_pressure_bar, "bar")} />
            <DataRow label="Vibration" value={formatValue(snapshot?.vibration_rms_mms, "mm/s")} />
            <DataRow label="Bearing temp" value={formatValue(snapshot?.bearing_temperature_c, "C")} />
          </dl>
        </Panel>

        <Panel id="deterministic-state-panel" title="Deterministic State" icon={Gauge}>
          <div className="stacked-bars">
            <ScoreBar label="NPSH" value={features?.npsh_score ?? 0} />
            <ScoreBar label="Suction" value={features?.suction_score ?? 0} />
            <ScoreBar label="Vibration" value={features?.vibration_score ?? 0} />
            <ScoreBar label="Current" value={features?.current_score ?? 0} />
            <ScoreBar label="Thermal" value={features?.thermal_score ?? 0} />
          </div>
          <dl className="compact-list">
            <DataRow label="Pump head" value={formatValue(features?.pump_head_m, "m")} />
            <DataRow label="Confidence" value={percent(features?.confidence)} />
            <DataRow label="NPSHr method" value={features?.npshr_method ?? "pending"} />
          </dl>
        </Panel>

        <Panel id="belief-panel" title="Belief And RUL" icon={BrainCircuit}>
          <div className="metric-grid">
            <Metric label="Damage belief" value={percent(analysis?.belief.mean_damage)} />
            <Metric label="Belief confidence" value={percent(analysis?.belief.confidence)} />
            <Metric label="RUL" value={formatValue(analysis?.prediction.rul_hours, "h")} />
            <Metric label="CVaR95" value={formatCurrency(analysis?.prediction.cvar95)} />
          </div>
          <div className="probability-row">
            <Metric label="24h fail" value={percent(analysis?.prediction.failure_probability_24h)} />
            <Metric label="7d fail" value={percent(analysis?.prediction.failure_probability_7d)} />
            <Metric label="30d fail" value={percent(analysis?.prediction.failure_probability_30d)} />
          </div>
        </Panel>

        <Panel id="policy-panel" title="Policy" icon={ShieldCheck}>
          <p className="decision-text">{titleize(analysis?.recommended_action)}</p>
          <p className="supporting-text">{analysis?.recommendation.decision_reason ?? "Awaiting deterministic analysis."}</p>
          <div className="cost-table">
            {costRows.map((row) => (
              <div className="cost-row" key={row.action}>
                <span>{titleize(row.action)}</span>
                <div className="cost-track">
                  <div style={{ width: `${topCost > 0 ? Math.max(8, (row.cost / topCost) * 100) : 0}%` }} />
                </div>
                <strong>{formatCurrency(row.cost)}</strong>
              </div>
            ))}
          </div>
        </Panel>

        <Panel id="operator-checks-panel" title="Operator Checks" icon={Wrench}>
          <ol className="check-list">
            {recommendedChecks.slice(0, 5).map((item, index) => (
              <li key={`${item.item_id}-${item.item_text}`}>
                <span>{index + 1}</span>
                <div>
                  <strong>{item.item_text}</strong>
                  <small>{item.item_id ? `${item.successes}/${item.attempts} success` : "deterministic recommendation"}</small>
                </div>
              </li>
            ))}
          </ol>
          <form className="feedback-form" onSubmit={submitOperatorFeedback}>
            <label>
              Resolved check
              <select value={selectedChecklistId} onChange={(event) => setSelectedChecklistId(event.target.value)}>
                {checklist.map((item) => (
                  <option key={item.item_id} value={item.item_id}>
                    {item.item_text}
                  </option>
                ))}
              </select>
            </label>
            <div className="toggle-row">
              <label>
                <input
                  checked={feedback.confirmed_cavitation}
                  onChange={(event) =>
                    setFeedback((current) => ({ ...current, confirmed_cavitation: event.target.checked }))
                  }
                  type="checkbox"
                />
                Confirmed cavitation
              </label>
              <label>
                <input
                  checked={feedback.false_alarm}
                  onChange={(event) => setFeedback((current) => ({ ...current, false_alarm: event.target.checked }))}
                  type="checkbox"
                />
                False alarm
              </label>
              <label>
                <input
                  checked={feedback.missed_cavitation}
                  onChange={(event) =>
                    setFeedback((current) => ({ ...current, missed_cavitation: event.target.checked }))
                  }
                  type="checkbox"
                />
                Missed cavitation
              </label>
            </div>
            <label>
              Notes
              <textarea
                value={feedback.notes}
                onChange={(event) => setFeedback((current) => ({ ...current, notes: event.target.value }))}
                rows={3}
              />
            </label>
            <button className="primary-button" disabled={!state.analysis || feedbackSubmitting} type="submit">
              <Send size={16} aria-hidden="true" />
              <span>{feedbackSubmitting ? "Saving" : "Save feedback"}</span>
            </button>
          </form>
          {feedbackResult ? (
            <p className="inline-result">
              Feedback #{feedbackResult.result.feedback_sequence} stored for {feedbackResult.result.case_id}.
            </p>
          ) : null}
        </Panel>

        <Panel id="explanation-panel" title="Explanation" icon={MessageSquareText} wide>
          <div className="explanation-grid">
            <div>
              <div className="trace-heading">
                <span>Deterministic core</span>
                <StatusPill tone={analysis?.explanation.guard_passed ? "ok" : "warn"}>
                  {analysis?.explanation.source ?? "pending"}
                </StatusPill>
              </div>
              <p className="explanation-text">{analysis?.explanation.text ?? "Awaiting deterministic explanation."}</p>
              <div className="source-strip">
                {(analysis?.equations_used ?? []).map((source) => (
                  <code key={source}>{source}</code>
                ))}
              </div>
            </div>
            <div>
              <div className="trace-heading">
                <span>mistral.rs explanation only</span>
                <StatusPill tone={llmAvailable ? "ok" : "idle"}>
                  {state.llmStatus?.source ?? "offline"}
                </StatusPill>
              </div>
              <button
                className="secondary-button"
                disabled={!state.analysis || !llmAvailable || llmLoading}
                onClick={() => void runLlmExplanation()}
                type="button"
              >
                <Sparkles size={16} aria-hidden="true" />
                <span>{llmLoading ? "Running mistral.rs" : "Run mistral.rs"}</span>
              </button>
              <TraceBlock result={llmResult} status={state.llmStatus} />
            </div>
          </div>
        </Panel>

        <Panel id="learning-panel" title="Learning" icon={DatabaseZap}>
          <div className="metric-grid">
            <Metric
              label="False alarms"
              value={String(state.learning?.summary.calibration.false_alarm_count ?? 0)}
            />
            <Metric
              label="Missed"
              value={String(state.learning?.summary.calibration.missed_cavitation_count ?? 0)}
            />
            <Metric
              label="Review"
              value={String(state.learning?.summary.calibration.review_required_count ?? 0)}
            />
          </div>
          <div className="threshold-list">
            {(state.learning?.summary.thresholds ?? []).map((threshold) => (
              <div className="threshold-row" key={threshold.threshold_name}>
                <div>
                  <strong>{titleize(threshold.threshold_name)}</strong>
                  <small>{threshold.status}</small>
                </div>
                <span>
                  {formatValue(threshold.current_value, "")}
                  {threshold.proposed_value !== null ? ` -> ${formatValue(threshold.proposed_value, "")}` : ""}
                </span>
              </div>
            ))}
          </div>
        </Panel>

        <Panel id="data-contract-panel" title="Data Contract" icon={Route}>
          <dl className="metric-list">
            <DataRow label="Endpoint" value={state.status?.endpoint ?? "pending"} />
            <DataRow label="Namespace" value={state.status?.namespace_uri ?? "pending"} />
            <DataRow label="Operational nodes" value={String(state.status?.operational_node_count ?? 0)} />
            <DataRow label="Design nodes" value={String(state.status?.design_spec_node_count ?? 0)} />
            <DataRow label="Output endpoint" value={state.output?.endpoint ?? "pending"} />
            <DataRow label="Output nodes" value={String(state.output?.node_count ?? 0)} />
            <DataRow
              label="Last publish"
              value={state.output?.initialized ? `seq ${state.output.sequence}` : "pending"}
            />
            <DataRow label="Trace" value={basename(state.status?.replay_trace)} />
          </dl>
          <div className="blocked-nodes">
            {(state.status?.command_nodes_blocked ?? []).map((node) => (
              <code key={node}>{node}</code>
            ))}
          </div>
        </Panel>
      </section>
    </main>
  );
}

function Panel({
  id,
  title,
  icon: Icon,
  children,
  wide = false,
}: {
  id?: string;
  title: string;
  icon: LucideIcon;
  children: ReactNode;
  wide?: boolean;
}) {
  return (
    <section className={`panel ${wide ? "panel-wide" : ""}`} id={id}>
      <div className="panel-title">
        <Icon size={18} aria-hidden="true" />
        <h2>{title}</h2>
      </div>
      {children}
    </section>
  );
}

function StatusChip({
  icon: Icon,
  label,
  value,
  tone,
}: {
  icon: LucideIcon;
  label: string;
  value: string;
  tone: "ok" | "warn" | "idle";
}) {
  return (
    <div className={`status-chip status-${tone}`}>
      <Icon size={16} aria-hidden="true" />
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function StatusPill({ tone, children }: { tone: "ok" | "warn" | "idle"; children: ReactNode }) {
  return <span className={`status-pill status-${tone}`}>{children}</span>;
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="metric">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function DataRow({ label, value }: { label: string; value: string }) {
  return (
    <>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </>
  );
}

function ScoreBar({ label, value }: { label: string; value: number }) {
  const width = Math.max(2, Math.min(100, value * 100));
  return (
    <div className="score-row">
      <span>{label}</span>
      <div className="score-track">
        <div style={{ width: `${width}%` }} />
      </div>
      <strong>{percent(value)}</strong>
    </div>
  );
}

function TraceBlock({ result, status }: { result: LlmExplainResponse | null; status: LlmStatus | null }) {
  if (!result) {
    return (
      <div className="trace-block">
        <dl>
          <DataRow label="Runner" value={status?.status.provider ?? "mistralrs"} />
          <DataRow label="Status" value={status?.status.reason ?? status?.status.message ?? "not checked"} />
          <DataRow label="Fallback" value={status?.fallback_used ? "true" : "false"} />
        </dl>
      </div>
    );
  }
  return (
    <div className="trace-block">
      <dl>
        <DataRow label="Run id" value={result.llm_run_id} />
        <DataRow label="Prompt hash" value={result.llm.prompt_hash} />
        <DataRow label="Guard" value={result.llm.guard_passed ? "passed" : "blocked"} />
        <DataRow label="Fallback" value={result.llm.fallback_used ? "true" : "false"} />
      </dl>
      <details>
        <summary>Prompt</summary>
        <pre>{result.llm.prompt_text}</pre>
      </details>
      <details>
        <summary>Output</summary>
        <pre>{result.llm.guarded_output || result.llm.raw_output}</pre>
      </details>
      {result.llm.guard_reasons.length > 0 ? (
        <ul className="guard-reasons">
          {result.llm.guard_reasons.map((reason) => (
            <li key={reason}>{reason}</li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}

function mergeRecommendedChecks(recommended: string[], ranked: ChecklistItem[]) {
  const byText = new Map(ranked.map((item) => [normalize(item.item_text), item]));
  if (recommended.length === 0) {
    return ranked;
  }
  return recommended.map((itemText) => {
    const rankedItem = byText.get(normalize(itemText));
    return (
      rankedItem ?? {
        item_id: itemText,
        item_text: itemText,
        fault_type: "cavitation",
        successes: 0,
        attempts: 0,
        expected_success: 0,
      }
    );
  });
}

function buildCostRows(costs: Record<string, number>) {
  return Object.entries(costs)
    .map(([action, cost]) => ({ action, cost }))
    .sort((left, right) => right.cost - left.cost);
}

function buildSchematicMeasurements(
  snapshot: ObservationSnapshot | null,
  features: DerivedFeatures | null,
) {
  return [
    { label: "Flow", value: formatValue(snapshot?.flow_rate_m3h, "m3/h") },
    { label: "Suction", value: formatValue(snapshot?.suction_pressure_bar, "bar"), tone: "risk" as const },
    { label: "Discharge", value: formatValue(snapshot?.discharge_pressure_bar, "bar") },
    { label: "Current", value: formatValue(snapshot?.motor_current_a, "A") },
    { label: "Voltage", value: formatValue(snapshot?.motor_voltage_v, "V", 0) },
    { label: "Vibration", value: formatValue(snapshot?.vibration_rms_mms, "mm/s"), tone: "risk" as const },
    { label: "Bearing temp", value: formatValue(snapshot?.bearing_temperature_c, "C") },
    { label: "Liquid temp", value: formatValue(snapshot?.liquid_temperature_c, "C") },
    { label: "Speed", value: formatValue(snapshot?.speed_rpm, "rpm", 0) },
    { label: "NPSHA", value: formatValue(features?.npsha_m, "m") },
    { label: "NPSHR", value: formatValue(features?.npshr_m, "m") },
    { label: "NPSH margin", value: formatValue(features?.npsh_margin_m, "m"), tone: "risk" as const },
    { label: "Rated flow", value: formatValue(snapshot?.design_specs.rated_flow_m3h, "m3/h") },
    { label: "Rated head", value: formatValue(snapshot?.design_specs.rated_head_m, "m") },
    { label: "Rated speed", value: formatValue(snapshot?.design_specs.rated_speed_rpm, "rpm", 0) },
    { label: "Power", value: formatValue(snapshot?.design_specs.power_kw, "kW", 0) },
    { label: "Stages", value: snapshot?.design_specs.stages ? `${snapshot.design_specs.stages}` : "UNKNOWN" },
  ];
}

function scrollToPanel(targetId: string) {
  document.getElementById(targetId)?.scrollIntoView({
    behavior: "smooth",
    block: "start",
  });
}

function titleize(value: string | undefined) {
  if (!value) {
    return "Pending";
  }
  return value
    .replaceAll("_", " ")
    .replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function formatValue(value: number | null | undefined, unit: string, digits = 2) {
  if (value === null || value === undefined || !Number.isFinite(value)) {
    return "UNKNOWN";
  }
  return `${value.toFixed(digits)}${unit ? ` ${unit}` : ""}`;
}

function formatCurrency(value: number | null | undefined) {
  if (value === null || value === undefined || !Number.isFinite(value)) {
    return "$0";
  }
  return new Intl.NumberFormat("en-US", {
    currency: "USD",
    maximumFractionDigits: 0,
    style: "currency",
  }).format(value);
}

function percent(value: number | null | undefined) {
  if (value === null || value === undefined || !Number.isFinite(value)) {
    return "0%";
  }
  return `${Math.round(value * 100)}%`;
}

function normalize(value: string) {
  return value.trim().toLowerCase().replace(/\.$/, "");
}

function basename(value: string | undefined) {
  if (!value) {
    return "pending";
  }
  return value.split("/").at(-1) ?? value;
}
