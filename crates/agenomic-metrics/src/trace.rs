//! A minimal, faithful model of the canonical **v0.3** run trace, plus
//! [`metrics_from_trace_v03`].
//!
//! This is *not* a re-implementation of the spec's validation or crypto (that
//! is the job of the canonical/ledger crates). It is a tolerant, read-only
//! projection of the [`agenomic/v0.3` trace](https://agenomic.dev/spec/v0.3/trace-event.schema.json)
//! exposing exactly the fields the Phase 2 metrics need. Unknown fields are
//! ignored and unknown event types deserialize to [`EventType::Unknown`], so a
//! single malformed corner never sinks the whole report.

use serde::{Deserialize, Serialize};

use crate::metrics::{
    causal_coverage, compliance_confidence, controllability, decision_explainability,
    policy_adherence, provenance_coverage, trace_completeness, WeightedConfidence,
};
use crate::report::MetricsReport;
use crate::MetricsError;

/// The enumerated v0.3 trace event vocabulary
/// (`schemas/v0.3/event-type-registry.json`).
///
/// Unrecognized strings deserialize to [`EventType::Unknown`] rather than
/// failing, keeping the metrics layer resilient to forward-compatible
/// additions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventType {
    /// `run.started`
    #[serde(rename = "run.started")]
    RunStarted,
    /// `agent.loaded`
    #[serde(rename = "agent.loaded")]
    AgentLoaded,
    /// `genome.resolved`
    #[serde(rename = "genome.resolved")]
    GenomeResolved,
    /// `prompt.loaded`
    #[serde(rename = "prompt.loaded")]
    PromptLoaded,
    /// `policy.loaded`
    #[serde(rename = "policy.loaded")]
    PolicyLoaded,
    /// `memory.read`
    #[serde(rename = "memory.read")]
    MemoryRead,
    /// `knowledge.retrieve`
    #[serde(rename = "knowledge.retrieve")]
    KnowledgeRetrieve,
    /// `llm.requested`
    #[serde(rename = "llm.requested")]
    LlmRequested,
    /// `llm.responded`
    #[serde(rename = "llm.responded")]
    LlmResponded,
    /// `decision.made`
    #[serde(rename = "decision.made")]
    Decision,
    /// `tool.call.proposed`
    #[serde(rename = "tool.call.proposed")]
    ToolCallProposed,
    /// `policy.check.performed`
    #[serde(rename = "policy.check.performed")]
    PolicyCheckPerformed,
    /// `tool.call.approved`
    #[serde(rename = "tool.call.approved")]
    ToolCallApproved,
    /// `tool.call.executed`
    #[serde(rename = "tool.call.executed")]
    ToolCallExecuted,
    /// `tool.result.observed`
    #[serde(rename = "tool.result.observed")]
    ToolResultObserved,
    /// `memory.write.proposed`
    #[serde(rename = "memory.write.proposed")]
    MemoryWriteProposed,
    /// `memory.write.committed`
    #[serde(rename = "memory.write.committed")]
    MemoryWriteCommitted,
    /// `human.review.requested`
    #[serde(rename = "human.review.requested")]
    HumanReviewRequested,
    /// `human.review.approved`
    #[serde(rename = "human.review.approved")]
    HumanReviewApproved,
    /// `human.review.rejected`
    #[serde(rename = "human.review.rejected")]
    HumanReviewRejected,
    /// `human.review.modified`
    #[serde(rename = "human.review.modified")]
    HumanReviewModified,
    /// `risk.score.updated`
    #[serde(rename = "risk.score.updated")]
    RiskScoreUpdated,
    /// `compliance.check.performed`
    #[serde(rename = "compliance.check.performed")]
    ComplianceCheckPerformed,
    /// `alignment.check.performed`
    #[serde(rename = "alignment.check.performed")]
    AlignmentCheckPerformed,
    /// `error.raised`
    #[serde(rename = "error.raised")]
    ErrorRaised,
    /// `run.completed`
    #[serde(rename = "run.completed")]
    RunCompleted,
    /// `run.replay.started`
    #[serde(rename = "run.replay.started")]
    RunReplayStarted,
    /// `run.replay.completed`
    #[serde(rename = "run.replay.completed")]
    RunReplayCompleted,
    /// Any event type not in the v0.3 registry.
    #[serde(other)]
    #[default]
    Unknown,
}

impl EventType {
    /// The conservative baseline set of event types that **every** well-formed
    /// run must contain: a start and a completion. This is the default
    /// `expected` set used by [`metrics_from_trace_v03`] for the Trace
    /// Completeness Score.
    ///
    /// A policy-specific required set (e.g. mandating a `policy.check.performed`
    /// before every `tool.call.executed`) should be supplied directly to
    /// [`trace_completeness`](crate::trace_completeness) instead.
    ///
    /// # Examples
    /// ```
    /// use agenomic_metrics::EventType;
    /// assert_eq!(EventType::baseline_expected().len(), 2);
    /// ```
    pub fn baseline_expected() -> Vec<EventType> {
        vec![EventType::RunStarted, EventType::RunCompleted]
    }
}

/// Provenance attached to an event.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Provenance {
    /// IDs of upstream events/artifacts this event used.
    #[serde(default)]
    pub used: Vec<String>,
    /// Evidence-unit references supporting this event.
    #[serde(default)]
    pub evidence_units: Vec<String>,
}

impl Provenance {
    fn has_support(&self) -> bool {
        !self.used.is_empty() || !self.evidence_units.is_empty()
    }
}

/// A single inline policy check carried on an event.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct PolicyCheck {
    /// `pass` | `fail` | `warning` | `not_applicable` (other strings ignored).
    #[serde(default)]
    pub status: Option<String>,
}

/// One append-only trace event (only the fields the metrics need).
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Event {
    /// Stable event id (referenced by the execution graph).
    #[serde(default)]
    pub event_id: String,
    /// Canonical event type.
    #[serde(rename = "type", default)]
    pub event_type: EventType,
    /// Inline provenance, if recorded.
    #[serde(default)]
    pub provenance: Option<Provenance>,
    /// Inline policy checks, if recorded.
    #[serde(default)]
    pub policy_checks: Vec<PolicyCheck>,
}

/// A node in the causal execution graph.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct GraphNode {
    /// Node id (an `event_id` for `kind == "event"`).
    #[serde(default)]
    pub id: String,
    /// Node kind (`event`, `artifact`, …).
    #[serde(default)]
    pub kind: String,
}

/// A directed edge in the causal execution graph.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct GraphEdge {
    /// Source node id.
    #[serde(default)]
    pub from: String,
    /// Destination node id.
    #[serde(default)]
    pub to: String,
    /// Edge type (`caused_by`, `produced`, `supported_by`, …).
    #[serde(rename = "type", default)]
    pub edge_type: String,
}

/// The causal execution graph.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ExecutionGraph {
    /// Graph nodes.
    #[serde(default)]
    pub nodes: Vec<GraphNode>,
    /// Graph edges.
    #[serde(default)]
    pub edges: Vec<GraphEdge>,
}

/// A compliance or alignment check entry.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Check {
    /// `pass` | `fail` | `warning` | `not_applicable`.
    #[serde(default)]
    pub status: Option<String>,
}

/// Identity of the agent that produced the run.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Agent {
    /// `agent://org/name` identifier.
    #[serde(default)]
    pub agent_id: String,
    /// Content-addressed genome version (`sha256:…`). Used by the drift
    /// detector (P2-2) to classify a drift as intended vs unintended.
    #[serde(default)]
    pub genome_version: String,
}

/// A tolerant projection of the canonical v0.3 run trace.
///
/// Every field is optional/defaulted so that partial or forward-compatible
/// JSON still parses — validation is the canonical crate's responsibility,
/// not this metrics projection's.
///
/// # Examples
/// ```
/// use agenomic_metrics::TraceV03;
/// let json = r#"{
///   "run_id": "run_1",
///   "events": [
///     {"event_id": "e1", "type": "run.started"},
///     {"event_id": "e2", "type": "run.completed"}
///   ]
/// }"#;
/// let trace = TraceV03::from_json(json).unwrap();
/// assert_eq!(trace.events.len(), 2);
/// ```
#[derive(Clone, Debug, Default, Deserialize)]
pub struct TraceV03 {
    /// Run identifier.
    #[serde(default)]
    pub run_id: String,
    /// Producing agent.
    #[serde(default)]
    pub agent: Agent,
    /// The append-only event stream, in order.
    #[serde(default)]
    pub events: Vec<Event>,
    /// The causal execution graph.
    #[serde(default)]
    pub execution_graph: ExecutionGraph,
    /// Run-level compliance checks.
    #[serde(default)]
    pub compliance_checks: Vec<Check>,
    /// Run-level alignment checks.
    #[serde(default)]
    pub alignment_checks: Vec<Check>,
}

impl TraceV03 {
    /// Parses a canonical v0.3 trace from JSON.
    ///
    /// # Errors
    /// Returns [`MetricsError::TraceParse`] if the input is not valid JSON for
    /// this (tolerant) projection.
    ///
    /// # Examples
    /// ```
    /// use agenomic_metrics::TraceV03;
    /// assert!(TraceV03::from_json("not json").is_err());
    /// ```
    pub fn from_json(json: &str) -> Result<Self, MetricsError> {
        serde_json::from_str(json).map_err(|e| MetricsError::TraceParse(e.to_string()))
    }

    /// The event types present in the trace, in order.
    pub fn event_types(&self) -> Vec<EventType> {
        self.events.iter().map(|e| e.event_type).collect()
    }
}

fn status_pass_fail_warn(status: Option<&str>) -> Option<bool> {
    match status {
        Some("pass") => Some(true),
        Some("fail") | Some("warning") => Some(false),
        _ => None, // not_applicable / unknown / absent → not an applicable rule
    }
}

/// Computes every metric that is derivable from a **single** canonical v0.3
/// trace.
///
/// Cross-run / replay / runtime metrics (behavioral drift, replay fidelity,
/// runtime variance, alignment stability, …) require more than one trace and
/// are left `None`; they are populated by the cloud drift detector (P2-2) and
/// replay matrix (P2-3).
///
/// What it computes from one trace:
/// * **TCS** against [`EventType::baseline_expected`];
/// * **PAS** from inline `policy_checks` plus run-level `compliance_checks`;
/// * **DES** over `decision.made` events carrying provenance;
/// * **causal coverage** from the execution graph;
/// * **provenance coverage** over claim-bearing events;
/// * **compliance confidence** from `compliance_checks`;
/// * **controllability** from `human.review.*` events (only when review was
///   requested).
///
/// # Examples
/// ```
/// use agenomic_metrics::{metrics_from_trace_v03, TraceV03};
/// let json = r#"{
///   "events": [
///     {"event_id": "e1", "type": "run.started"},
///     {"event_id": "e2", "type": "run.completed"}
///   ],
///   "compliance_checks": [{"status": "pass"}]
/// }"#;
/// let trace = TraceV03::from_json(json).unwrap();
/// let report = metrics_from_trace_v03(&trace);
/// assert_eq!(report.trace_completeness, Some(1.0));
/// assert_eq!(report.compliance_confidence, Some(1.0));
/// // No two windows → no behavioral drift from a single trace.
/// assert_eq!(report.behavioral_drift, None);
/// ```
pub fn metrics_from_trace_v03(trace: &TraceV03) -> MetricsReport {
    let mut report = MetricsReport::empty();

    // --- Trace Completeness Score ---------------------------------------
    let captured = trace.event_types();
    report.trace_completeness = Some(trace_completeness(
        &captured,
        &EventType::baseline_expected(),
    ));

    // --- Policy Adherence Score -----------------------------------------
    let mut applicable = 0usize;
    let mut passed = 0usize;
    for event in &trace.events {
        for pc in &event.policy_checks {
            if let Some(ok) = status_pass_fail_warn(pc.status.as_deref()) {
                applicable += 1;
                passed += usize::from(ok);
            }
        }
    }
    for check in &trace.compliance_checks {
        if let Some(ok) = status_pass_fail_warn(check.status.as_deref()) {
            applicable += 1;
            passed += usize::from(ok);
        }
    }
    if applicable > 0 {
        report.policy_adherence = Some(policy_adherence(passed, applicable, None));
    }

    // --- Decision Explainability ----------------------------------------
    let decisions = trace
        .events
        .iter()
        .filter(|e| e.event_type == EventType::Decision)
        .count();
    if decisions > 0 {
        let explained = trace
            .events
            .iter()
            .filter(|e| e.event_type == EventType::Decision)
            .filter(|e| e.provenance.as_ref().is_some_and(Provenance::has_support))
            .count();
        report.decision_explainability = Some(decision_explainability(explained, decisions));
    }

    // --- Causal Coverage ------------------------------------------------
    // Non-root events = every event after the first (by append order).
    // An event "has a parent link" if its id is the destination of any edge.
    if !trace.events.is_empty() {
        let with_incoming: std::collections::HashSet<&str> = trace
            .execution_graph
            .edges
            .iter()
            .map(|e| e.to.as_str())
            .collect();
        let non_root = trace.events.len().saturating_sub(1);
        let with_parent = trace
            .events
            .iter()
            .skip(1)
            .filter(|e| with_incoming.contains(e.event_id.as_str()))
            .count();
        report.causal_coverage = Some(causal_coverage(with_parent, non_root));
    }

    // --- Provenance Coverage --------------------------------------------
    // Claims = events that assert an outcome: decisions and model answers.
    let is_claim = |t: EventType| matches!(t, EventType::Decision | EventType::LlmResponded);
    let claims = trace
        .events
        .iter()
        .filter(|e| is_claim(e.event_type))
        .count();
    if claims > 0 {
        let supported = trace
            .events
            .iter()
            .filter(|e| is_claim(e.event_type))
            .filter(|e| e.provenance.as_ref().is_some_and(Provenance::has_support))
            .count();
        report.provenance_coverage = Some(provenance_coverage(supported, claims));
    }

    // --- Compliance Confidence ------------------------------------------
    let confidences: Vec<WeightedConfidence> = trace
        .compliance_checks
        .iter()
        .filter_map(|c| match c.status.as_deref() {
            Some("pass") => Some(WeightedConfidence::new(1.0, 1.0)),
            Some("warning") => Some(WeightedConfidence::new(1.0, 0.5)),
            Some("fail") => Some(WeightedConfidence::new(1.0, 0.0)),
            _ => None,
        })
        .collect();
    if !confidences.is_empty() {
        report.compliance_confidence = Some(compliance_confidence(&confidences));
    }

    // --- Controllability -------------------------------------------------
    let requested = trace
        .events
        .iter()
        .filter(|e| e.event_type == EventType::HumanReviewRequested)
        .count();
    if requested > 0 {
        let respected = trace
            .events
            .iter()
            .filter(|e| {
                matches!(
                    e.event_type,
                    EventType::HumanReviewApproved
                        | EventType::HumanReviewRejected
                        | EventType::HumanReviewModified
                )
            })
            .count();
        report.controllability = Some(controllability(respected, 0, 0, requested));
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    const RICH: &str = include_str!("../tests/fixtures/rich_trace.json");

    #[test]
    fn parses_unknown_event_type_as_unknown() {
        let json = r#"{"events":[{"event_id":"e","type":"totally.made.up"}]}"#;
        let trace = TraceV03::from_json(json).unwrap();
        assert_eq!(trace.events[0].event_type, EventType::Unknown);
    }

    #[test]
    fn rich_trace_metrics() {
        let trace = TraceV03::from_json(RICH).unwrap();
        let report = metrics_from_trace_v03(&trace);

        // run.started + run.completed both present.
        assert_eq!(report.trace_completeness, Some(1.0));
        // evt_02 policy_check pass + compliance_check pass → 2/2.
        assert_eq!(report.policy_adherence, Some(1.0));
        // evt_03 (llm.responded) carries provenance → 1/1.
        assert_eq!(report.provenance_coverage, Some(1.0));
        // 4 events, evt_02 & evt_03 have incoming edges, evt_04 does not → 2/3.
        let cc = report.causal_coverage.unwrap();
        assert!((cc - 2.0 / 3.0).abs() < 1e-12);
        // Single compliance pass.
        assert_eq!(report.compliance_confidence, Some(1.0));
        // No decisions, no human review, single trace.
        assert_eq!(report.decision_explainability, None);
        assert_eq!(report.controllability, None);
        assert_eq!(report.behavioral_drift, None);
    }

    #[test]
    fn empty_trace_is_well_behaved() {
        let trace = TraceV03::default();
        let report = metrics_from_trace_v03(&trace);
        // No run.started/run.completed captured → 0 of 2 required.
        assert_eq!(report.trace_completeness, Some(0.0));
        assert_eq!(report.policy_adherence, None);
        assert_eq!(report.causal_coverage, None);
    }
}
