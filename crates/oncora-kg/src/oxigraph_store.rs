//! Real graph backend via **Oxigraph** (an RDF quad store with SPARQL).
//!
//! Implements the same [`GraphStore`] trait as [`crate::InMemoryGraphStore`],
//! so retrieval/memory are unchanged whether edges live in the in-memory
//! reference store or Oxigraph. This is the canon's ontology/RDF layer (see
//! `docs/04-knowledge-and-data.md`).
//!
//! RDF triples are `(subject, predicate, object)`; per-edge **confidence** is
//! carried in the quad's fourth component (the named graph), encoded as
//! `urn:oncora:conf:<value>`. Entities/predicates are minted as
//! `urn:oncora:<token>` IRIs. `default-features = false` keeps this a pure-Rust
//! in-memory store (no native RocksDB).

use async_trait::async_trait;
use oncora_core::{Confidence, GraphStore, OncoraError, Result, Triple};
use oxigraph::model::{
    GraphName, NamedNode, NamedOrBlankNode, NamedOrBlankNodeRef, Quad, Term, TermRef,
};
use oxigraph::store::Store;

const ENT: &str = "urn:oncora:";
const PRED: &str = "urn:oncora:pred:";
const CONF: &str = "urn:oncora:conf:";

/// A [`GraphStore`] backed by an in-memory Oxigraph store.
pub struct OxigraphGraphStore {
    store: Store,
}

fn storage(e: impl std::fmt::Display) -> OncoraError {
    OncoraError::Storage(format!("oxigraph: {e}"))
}

/// Percent-encode a token so any string becomes a valid IRI path component
/// (unreserved RFC 3986 chars pass through; everything else becomes `%XX`).
fn enc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Inverse of [`enc`].
fn dec(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn entity_iri(token: &str) -> Result<NamedNode> {
    NamedNode::new(format!("{ENT}{}", enc(token))).map_err(storage)
}
fn pred_iri(token: &str) -> Result<NamedNode> {
    NamedNode::new(format!("{PRED}{}", enc(token))).map_err(storage)
}

/// Strip a known IRI prefix and percent-decode back to the original token.
fn strip(iri: &str, prefix: &str) -> String {
    dec(iri.strip_prefix(prefix).unwrap_or(iri))
}

/// Decode an Oxigraph quad back into our [`Triple`].
fn quad_to_triple(q: &Quad) -> Result<Triple> {
    let subject = match &q.subject {
        NamedOrBlankNode::NamedNode(n) => strip(n.as_str(), ENT).to_string(),
        other => return Err(storage(format!("unexpected subject {other}"))),
    };
    let predicate = strip(q.predicate.as_str(), PRED).to_string();
    let object = match &q.object {
        Term::NamedNode(n) => strip(n.as_str(), ENT).to_string(),
        other => return Err(storage(format!("unexpected object {other}"))),
    };
    let confidence = match &q.graph_name {
        GraphName::NamedNode(n) => strip(n.as_str(), CONF)
            .parse::<f64>()
            .map_err(|_| storage("confidence not a float"))?,
        _ => 1.0,
    };
    Ok(Triple::new(
        subject,
        predicate,
        object,
        Confidence::new(confidence),
    ))
}

impl OxigraphGraphStore {
    pub fn new_in_memory() -> Result<Self> {
        Ok(Self {
            store: Store::new().map_err(storage)?,
        })
    }

    fn collect(
        &self,
        iter: impl Iterator<Item = std::result::Result<Quad, oxigraph::store::StorageError>>,
    ) -> Result<Vec<Triple>> {
        let mut out = Vec::new();
        for q in iter {
            out.push(quad_to_triple(&q.map_err(storage)?)?);
        }
        Ok(out)
    }
}

#[async_trait]
impl GraphStore for OxigraphGraphStore {
    async fn assert(&self, triple: Triple) -> Result<()> {
        let s = entity_iri(&triple.subject)?;
        let p = pred_iri(&triple.predicate)?;
        let o = entity_iri(&triple.object)?;
        let g = NamedNode::new(format!("{CONF}{}", triple.confidence.get())).map_err(storage)?;
        let quad = Quad::new(s, p, o, g);
        self.store.insert(&quad).map_err(storage)?;
        Ok(())
    }

    async fn neighbors(&self, subject: &str) -> Result<Vec<Triple>> {
        let s = entity_iri(subject)?;
        let iter = self.store.quads_for_pattern(
            Some(NamedOrBlankNodeRef::NamedNode(s.as_ref())),
            None,
            None,
            None,
        );
        self.collect(iter)
    }

    async fn related(&self, entity: &str) -> Result<Vec<Triple>> {
        let node = entity_iri(entity)?;
        let mut out = self.collect(self.store.quads_for_pattern(
            Some(NamedOrBlankNodeRef::NamedNode(node.as_ref())),
            None,
            None,
            None,
        ))?;
        out.extend(self.collect(self.store.quads_for_pattern(
            None,
            None,
            Some(TermRef::NamedNode(node.as_ref())),
            None,
        ))?);
        Ok(out)
    }
}
