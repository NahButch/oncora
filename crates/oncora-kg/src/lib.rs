//! # oncora-kg
//!
//! The biomolecular knowledge layer. In production this is a **dual store**
//! (see `docs/04-knowledge-and-data.md`): `oxigraph` holds ontology-grounded
//! canonical entities (URIs from GO/Reactome/ChEMBL/UMLS/MONDO/HGNC) queried by
//! SPARQL, and `cozo` holds the evidence/assertion graph with per-edge
//! confidence, provenance, and time-travel queried by Datalog.
//!
//! Here we expose the [`schema`] (the entity and edge vocabulary), an
//! [`InMemoryGraphStore`] reference backend, and — under `--features cozo` — a
//! [`CozoGraphStore`] backed by a real CozoDB engine, both implementing
//! [`GraphStore`].

pub mod schema;

#[cfg(feature = "oxigraph")]
mod oxigraph_store;
#[cfg(feature = "oxigraph")]
pub use oxigraph_store::OxigraphGraphStore;

use std::sync::Mutex;

use async_trait::async_trait;
use oncora_core::{GraphStore, OncoraError, Result, Triple};

/// In-memory reference [`GraphStore`] — a simple triple list.
#[derive(Default)]
pub struct InMemoryGraphStore {
    triples: Mutex<Vec<Triple>>,
}

impl InMemoryGraphStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl GraphStore for InMemoryGraphStore {
    async fn assert(&self, triple: Triple) -> Result<()> {
        self.triples
            .lock()
            .map_err(|_| OncoraError::Storage("graph lock poisoned".into()))?
            .push(triple);
        Ok(())
    }

    async fn neighbors(&self, subject: &str) -> Result<Vec<Triple>> {
        Ok(self
            .triples
            .lock()
            .map_err(|_| OncoraError::Storage("graph lock poisoned".into()))?
            .iter()
            .filter(|t| t.subject == subject)
            .cloned()
            .collect())
    }

    async fn related(&self, entity: &str) -> Result<Vec<Triple>> {
        Ok(self
            .triples
            .lock()
            .map_err(|_| OncoraError::Storage("graph lock poisoned".into()))?
            .iter()
            .filter(|t| t.subject == entity || t.object == entity)
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oncora_core::Confidence;

    /// Backend-agnostic conformance test for any [`GraphStore`]. Every backend
    /// (in-memory, cozo, …) must pass it — the point of the trait boundary.
    pub async fn conformance(kg: &dyn GraphStore) {
        kg.assert(Triple::new(
            "EGFR",
            "associated_with",
            "NSCLC",
            Confidence::new(0.9),
        ))
        .await
        .unwrap();
        kg.assert(Triple::new(
            "EGFR",
            "involved_in",
            "MAPK",
            Confidence::new(0.8),
        ))
        .await
        .unwrap();

        // neighbors(EGFR): two outgoing edges, confidence preserved.
        let mut nbrs = kg.neighbors("EGFR").await.unwrap();
        nbrs.sort_by(|a, b| a.predicate.cmp(&b.predicate));
        assert_eq!(nbrs.len(), 2);
        assert_eq!(nbrs[0].object, "NSCLC");
        assert!((nbrs[0].confidence.get() - 0.9).abs() < 1e-9);

        // related(NSCLC): matches the edge by object.
        let rel = kg.related("NSCLC").await.unwrap();
        assert_eq!(rel.len(), 1);
        assert_eq!(rel[0].subject, "EGFR");
    }

    #[tokio::test]
    async fn in_memory_conforms() {
        conformance(&InMemoryGraphStore::new()).await;
    }

    #[cfg(feature = "oxigraph")]
    #[tokio::test]
    async fn oxigraph_conforms() {
        conformance(&OxigraphGraphStore::new_in_memory().unwrap()).await;
    }
}
