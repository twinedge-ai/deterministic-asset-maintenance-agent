export const API_BASE = "/api/asset-agent";

export type RiskLevel = "LOW" | "MEDIUM" | "HIGH" | "UNKNOWN";

export type WindowFeatures = {
  flow_rate_std_m3h: number;
  motor_current_std_a: number;
  vibration_max_mms: number;
  suction_pressure_min_bar: number;
};

export type DesignSpecs = {
  rated_flow_m3h: number;
  rated_head_m: number;
  rated_speed_rpm: number;
  npshr_m: number;
  efficiency_percent: number;
  power_kw: number;
  specific_gravity: number;
  design_suction_pressure_barg: number;
  max_discharge_pressure_barg: number;
  min_flow_m3h: number;
  max_flow_m3h: number;
  min_speed_rpm: number;
  pump_weight_kg: number;
  baseplate_weight_kg: number;
  stages: number;
};

export type ObservationSnapshot = {
  asset_id: string;
  timestamp: string;
  as_of_timestamp: string;
  window_seconds: number;
  sample_count: number;
  flow_rate_m3h: number;
  suction_pressure_bar: number;
  discharge_pressure_bar: number;
  motor_current_a: number;
  motor_voltage_v: number;
  vibration_rms_mms: number;
  bearing_temperature_c: number;
  liquid_temperature_c: number;
  liquid_temperature_source: string;
  speed_rpm: number;
  status: number;
  window_features: WindowFeatures;
  design_specs: DesignSpecs;
};

export type DerivedFeatures = {
  missing_inputs: string[];
  validation_errors: string[];
  suction_pressure_abs_kpa: number | null;
  discharge_pressure_abs_kpa: number | null;
  vapor_pressure_kpa: number | null;
  npsha_m: number | null;
  npshr_m: number | null;
  npsh_margin_m: number | null;
  npsh_margin_ratio: number | null;
  npshr_method: string;
  pump_head_m: number | null;
  pressure_delta_kpa: number | null;
  npsh_score: number;
  suction_score: number;
  vibration_score: number;
  current_score: number;
  thermal_score: number;
  flow_instability_score: number;
  current_instability_score: number;
  cavitation_evidence_score: number;
  confidence: number;
  limitations: string[];
};

export type BeliefSummary = {
  mean_damage: number;
  damage_variance: number;
  confidence: number;
  belief_method: string;
};

export type PredictionSummary = {
  rul_hours: number;
  rul_basis: string;
  failure_probability_24h: number;
  failure_probability_7d: number;
  failure_probability_30d: number;
  cvar95: number;
  cvar_basis: string;
};

export type Recommendation = {
  recommended_action: string;
  action_confidence: number;
  decision_reason: string;
  lowest_cost_action: string;
};

export type TemplateExplanation = {
  source: string;
  text: string;
  guard_passed: boolean;
  fallback_used: boolean;
  prompt_hash: string;
  guard_reasons: string[];
};

export type AnalysisResult = {
  asset_id: string;
  risk_level: RiskLevel;
  features: DerivedFeatures;
  belief: BeliefSummary;
  prediction: PredictionSummary;
  counterfactual_costs: Record<string, number>;
  recommendation: Recommendation;
  recommended_action: string;
  recommended_checklist: string[];
  limitations: string[];
  explanation: TemplateExplanation;
  equations_used: string[];
};

export type AgentOutputNode = {
  signal: string;
  node_id: string;
  data_type: "float" | "text" | "uint" | "bool";
  value: unknown;
};

export type AgentOutputSnapshot = {
  endpoint: string;
  namespace_uri: string;
  asset_id: string;
  initialized: boolean;
  sequence: number;
  updated_at: string | null;
  source_case_id: string | null;
  node_count: number;
  nodes: Record<string, AgentOutputNode>;
};

export type AgentOutputStatus = AgentOutputSnapshot & {
  ok: boolean;
  connected: boolean;
  mode: string;
};

export type OpcStatus = {
  connected: boolean;
  mode: string;
  endpoint: string;
  namespace_uri: string;
  namespace_resolution?: unknown;
  asset_id: string;
  latest_as_of_timestamp?: string;
  latest_temperature_source?: string;
  latest_temperature_c?: number;
  operational_node_count: number;
  design_spec_node_count: number;
  command_nodes_blocked: string[];
  freshness: {
    mode: string;
    source_timestamp?: string;
    as_of_timestamp?: string;
    status: string;
  };
  replay_trace: string;
};

export type SnapshotResponse = {
  ok: boolean;
  source: string;
  selector: string;
  trace: string;
  snapshot: ObservationSnapshot;
  design_spec_validation_errors: string[];
  temperature: {
    liquid_temperature_c: number;
    source: string;
  };
};

export type AnalyzeResponse = {
  ok: boolean;
  case_id: string;
  result: AnalysisResult;
  agent_output: AgentOutputSnapshot;
};

export type ChecklistItem = {
  item_id: string;
  item_text: string;
  fault_type: string;
  successes: number;
  attempts: number;
  expected_success: number;
};

export type ThresholdProposal = {
  asset_id: string;
  threshold_name: string;
  default_value: number;
  current_value: number;
  proposed_value: number | null;
  evidence_count: number;
  status: string;
  last_updated: string;
};

export type CalibrationSummary = {
  false_alarm_count: number;
  missed_cavitation_count: number;
  review_required_count: number;
};

export type LearningSummary = {
  checklist: ChecklistItem[];
  thresholds: ThresholdProposal[];
  calibration: CalibrationSummary;
};

export type LearningSummaryResponse = {
  ok: boolean;
  summary: LearningSummary;
};

export type ChecklistResponse = {
  ok: boolean;
  items: ChecklistItem[];
};

export type LlmStatus = {
  llm_available: boolean;
  source: string;
  fallback_used: boolean;
  status: {
    available: boolean;
    provider: string;
    reason?: string;
    binary_path?: string;
    model_path?: string;
    message?: string;
  };
  message: string;
};

export type LlmTrace = {
  source: string;
  prompt_hash: string;
  prompt_text: string;
  raw_output: string;
  guarded_output: string;
  guard_passed: boolean;
  fallback_used: boolean;
  guard_reasons: string[];
  runner: {
    provider: string;
    model_path?: string;
    binary_path?: string;
    seed: number;
    max_tokens: number;
    temperature: number;
  };
};

export type LlmExplainResponse = {
  ok: boolean;
  case_id: string;
  llm_run_id: string;
  risk_level: RiskLevel;
  recommended_action: string;
  llm: LlmTrace;
};

export type FeedbackInput = {
  confirmed_cavitation: boolean;
  solved_item_id?: string;
  false_alarm: boolean;
  missed_cavitation: boolean;
  notes?: string;
  post_action?: Record<string, unknown>;
};

export type FeedbackResponse = {
  ok: boolean;
  result: {
    case_id: string;
    feedback_sequence: number;
    top_checklist_item_id?: string;
    threshold_proposals: ThresholdProposal[];
    calibration_summary: CalibrationSummary;
  };
};

async function apiFetch<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${API_BASE}${path}`, {
    ...init,
    cache: "no-store",
    headers: {
      "content-type": "application/json",
      ...(init?.headers ?? {}),
    },
  });
  const payload = (await response.json()) as T & { error?: string };
  if (!response.ok) {
    throw new Error(payload.error ?? `${response.status} ${response.statusText}`);
  }
  return payload;
}

export function getOpcStatus() {
  return apiFetch<OpcStatus>("/api/opc/status");
}

export function getSnapshot(assetId = "hp_pump_1") {
  return apiFetch<SnapshotResponse>(`/api/opc/snapshot?asset_id=${assetId}`);
}

export function getOutputStatus() {
  return apiFetch<AgentOutputStatus>("/api/opc/output/status");
}

export function analyzeSnapshot(snapshot: ObservationSnapshot) {
  return apiFetch<AnalyzeResponse>("/api/analyze", {
    method: "POST",
    body: JSON.stringify(snapshot),
  });
}

export function getLearningSummary() {
  return apiFetch<LearningSummaryResponse>("/api/learning/summary");
}

export function getChecklist() {
  return apiFetch<ChecklistResponse>("/api/checklist");
}

export function getLlmStatus() {
  return apiFetch<LlmStatus>("/api/llm/status");
}

export function explainCase(caseId: string) {
  return apiFetch<LlmExplainResponse>("/api/llm/explain", {
    method: "POST",
    body: JSON.stringify({ case_id: caseId }),
  });
}

export function submitFeedback(caseId: string, feedback: FeedbackInput) {
  return apiFetch<FeedbackResponse>(`/api/cases/${caseId}/feedback`, {
    method: "POST",
    body: JSON.stringify(feedback),
  });
}
