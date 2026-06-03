//! # oncora-kg
//!
//! The biomolecular knowledge layer. In production this is a **dual store**
//! (see `docs/04-knowledge-and-data.md`): `oxigraph` holds ontology-grounded
//! canonical entities (URIs from GO/Reactome/ChEMBL/UMLS/MONDO/HGNC) queried by
//! SPARQL, and `cozo` holds the evidence/assertion graph with per-edge
//! confidence, provenance, and time-travel queried by Datalog.
//!
//! Here we expose the [`schema`] (the entity and edge vocabulary) and an
//! [`InMemoryGraphStore`] reference backend implementing [`GraphStore`].

pub mod schema;

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

    #[tokio::test]
    async fn asserts_and_queries_edges() {
        let kg = InMemoryGraphStore::new();
        kg.assert(Triple::new(
            "EGFR",
            "associated_with",
            "NSCLC",
            Confidence::new(0.9),
        ))
        .await
        .unwrap();
        assert_eq!(kg.neighbors("EGFR").await.unwrap().len(), 1);
        assert_eq!(kg.related("NSCLC").await.unwrap().len(), 1);
    }
}
