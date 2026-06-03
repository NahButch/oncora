//! The knowledge-graph vocabulary: entity and edge kinds.
//!
//! Mirrors the `erDiagram` in `docs/04-knowledge-and-data.md`. Canonical
//! entities are ontology-grounded (carry external URIs); the evidence graph
//! links them with confidence-weighted, provenance-bearing edges.

use serde::{Deserialize, Serialize};

/// The node types in the knowledge graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityKind {
    /// A gene / protein drug target (e.g. HGNC symbol).
    Target,
    Disease,
    Pathway,
    Compound,
    Variant,
    Trial,
    Cohort,
    /// An assertion that can be supported or contradicted.
    Claim,
    /// A piece of evidence for/against a claim.
    Evidence,
    /// A source document/record.
    Source,
}

/// The edge predicates between entities.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeKind {
    InvolvedIn,     // Target -> Pathway
    AssociatedWith, // Target -> Disease
    Modulates,      // Compound -> Target
    LocatedIn,      // Variant -> Target
    Confers,        // Variant -> sensitivity/resistance
    Targets,        // Trial -> Disease
    Tests,          // Trial -> Compound
    About,          // Claim -> any
    Supports,       // Evidence -> Claim
    Contradicts,    // Evidence -> Claim
    DerivedFrom,    // Evidence -> Source
    HasProvenance,  // any -> Provenance
}

impl EdgeKind {
    /// The predicate string used in [`oncora_core::Triple`].
    pub fn predicate(self) -> &'static str {
        match self {
            EdgeKind::InvolvedIn => "involved_in",
            EdgeKind::AssociatedWith => "associated_with",
            EdgeKind::Modulates => "modulates",
            EdgeKind::LocatedIn => "located_in",
            EdgeKind::Confers => "confers",
            EdgeKind::Targets => "targets",
            EdgeKind::Tests => "tests",
            EdgeKind::About => "about",
            EdgeKind::Supports => "supports",
            EdgeKind::Contradicts => "contradicts",
            EdgeKind::DerivedFrom => "derived_from",
            EdgeKind::HasProvenance => "has_provenance",
        }
    }
}
