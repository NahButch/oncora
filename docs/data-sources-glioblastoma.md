# Public Data Sources — Glioblastoma (fully-open subset)

> Working catalog of **fully open, no-registration, no-DUA** public data sources with glioblastoma (GBM) content, scoped to Oncora's four modalities. Registration-gated (e.g. GLASS, BraTS, Broad SCP downloads) and controlled-access (dbGaP / NIH face-reconstruction / DACO) sources are **deliberately excluded** — see [§6 Excluded](#6-excluded-not-fully-open).
>
> Each source is something `oncora-ingest` could pull into a content-addressed snapshot (BLAKE3, immutable) without a human in the access loop. This catalog feeds [04-knowledge-and-data.md](04-knowledge-and-data.md); it is not authoritative for tech choices ([05-tech-decisions.md](05-tech-decisions.md)).

**Verification:** light reachability + sizing check performed **2026-06-03** (canonical URLs returned live content unless flagged). Sizes are published figures where available, otherwise **estimates** (marked `~est`). Download times assume a sustained **~100 Mbps (12.5 MB/s)** link — divide by your real throughput. Portal/API sources are query-based, so "size" is a typical GBM-relevant pull, not the whole registry.

---

## 1. Genomics & multi-omics

| Source | Open scope (no reg / no DUA) | Access | Format | Size (GBM-relevant) | DL @100Mbps | Provenance / version |
|---|---|---|---|---|---|---|
| **TCGA-GBM** (NCI GDC) | Processed/derived only: RNA-seq STAR counts, methylation β, CNV (gene + segment), masked-somatic MAF, clinical/biospecimen. *Raw BAMs + germline are controlled — excluded.* | GDC REST API · `gdc-client` | TSV / MAF / TXT | ~5–15 GB `~est` (617 cases) | ~7–20 min | GDC Data Release (GRCh38), per-file UUID + release tag |
| **CPTAC-GBM** (PDC) | Open processed mass-spec: proteome `PDC000204`, phosphoproteome `PDC000205`, acetylome `PDC000245`; matched genomics open in GDC (CPTAC-3). | PDC GraphQL/REST · portal HTTP | TSV / matrices | ~1–5 GB `~est` (99 tumors + 10 normal) | ~1.5–7 min | Tied to CPTAC3; PDC study-versioned |
| **CGGA** (Chinese Glioma Genome Atlas) | All download links open: `mRNAseq_693`, `mRNAseq_325`, `WEseq_286`, methylation, miRNA_198, scRNA, proteomics(35), MRI image-genomic(268), clinical. | HTTP direct (per-dataset gzip TSV) | TSV.gz | ~1–3 GB core RNA-seq+clinical; ~5–20 GB whole site `~est` | ~1.5–25 min | Datasets named by sample count; page-dated (last upd. 2025-09-18), no semver |
| **Ivy GAP** (Allen Institute) | Core open: anatomic-structure RNA-seq (~270 samples), ISH/H&E images, de-identified clinical. *Partner "Clinical & Genomic Database" needs registration — excluded.* | Portal HTTP · Allen API | CSV / images | ~1–3 GB tabular `~est` (ISH image set much larger, streamed) | ~1.5–5 min (tabular) | Single fixed Allen release, manuscript-linked snapshot |
| **REMBRANDT** | Fully open legacy glioma cohort: 566 expression arrays, 834 CNV arrays, clinical. (Old NCI portal retired → migrated.) | GEO HTTP/FTP · G-DOC | CEL / TSV | ~2–6 GB `~est` (671 patients) | ~3–8 min | **GSE108476** = stable citable snapshot (2018) |
| **ICGC / PCAWG brain-CNS** | Open **consensus simple somatic mutations** (SNV/indel), consensus CNV/SV, histology, mutational signatures for non-US projects. *BAMs/germline/US-project calls controlled — excluded.* | S3-compatible (anon AWS CLI) / HTTP on `object.genomeinformatics.org` bucket `icgc25k-open` | VCF / TSV | ~1–5 GB brain-CNS SSM subset `~est` | ~1.5–7 min | Final ICGC release_28 + frozen PCAWG consensus. ⚠️ legacy `dcc.icgc.org` **retired 2024** — do not use |

**URLs:** TCGA-GBM `portal.gdc.cancer.gov/projects/TCGA-GBM` (API `api.gdc.cancer.gov`) · CPTAC-GBM `proteomic.datacommons.cancer.gov/pdc/study/PDC000204` · CGGA `cgga.org.cn/download.jsp` · Ivy GAP `glioblastoma.alleninstitute.org` · REMBRANDT `ncbi.nlm.nih.gov/geo/query/acc.cgi?acc=GSE108476` · ICGC open bucket `object.genomeinformatics.org/icgc25k-open/`

---

## 2. Imaging (MRI / digital pathology)

| Source | Open scope | Access | Format | Size | Cases | DL @100Mbps | Version |
|---|---|---|---|---|---|---|---|
| **UPENN-GBM** (TCIA) | **Fully open** (TCIA usage policy + citation). Pre-op mpMRI + tumor segmentations + radiomic features + de-id clinical (+ histopath slides). | NBIA Data Retriever (DICOM) · IBM Aspera (NIfTI/slides) | DICOM / NIfTI / NDPI / CSV | 357 GB full (DICOM 139 + NIfTI 69 + histopath 149); **MRI-only ≈69 GB** | 630 | ~8 hr full · ~1.5 hr MRI-only | v2 (2022-10-24) |
| **UCSF-PDGM** (TCIA) | **Fully open, CC BY 4.0.** Preoperative diffuse-glioma MRI incl. GBM, with molecular labels. | IBM Aspera · CSV metadata | NIfTI (+bvec/bval) | 142 GB | 495 | ~3.2 hr | v5 (2025-05-30) |
| **TCGA-GBM/LGG digital pathology** | **Open-access** H&E whole-slide images (GDC; *images* are not controlled). Also mirror `gs://gdc-tcga-phs000178-open/`. | `gdc-client` (+ manifest) · GCS bucket · ISB-CGC BigQuery | SVS (Aperio) | multi-TB corpus; GBM+LGG subset large (0.1–1+ GB/slide) | ~860 GBM / ~516 LGG cases, multi-slide | budget **hours–days** | GDC Data Releases |

**Hub:** The Cancer Imaging Archive — `cancerimagingarchive.net` (REST API + NBIA Data Retriever). Tooling note: several collections require the **IBM Aspera Connect** plugin in addition to NBIA. ⚠️ **TCGA-GBM radiology MRI** and **RHUH-GBM** are *not* listed here — both carry NIH controlled-access flags (see [§6](#6-excluded-not-fully-open)).

---

## 3. Single-cell & spatial

| Source | Open scope | Access | Format | Size | DL @100Mbps | Provenance |
|---|---|---|---|---|---|---|
| **Neftel et al. 2019** GBM scRNA-seq | Fully open (use **GEO**, not login-gated Broad SCP). Defining GBM cell states (NPC/OPC/AC/MES-like). | GEO bulk TAR + FTP | per-cell matrices | ~639 MB (28 tumors) | ~51 s | **GSE131928**, static (Cell 2019, doi:10.1016/j.cell.2019.06.024) |
| **GBmap** integrated GBM atlas | Fully open, anonymous CELLxGENE download. ~1.1M cells, 26 datasets, 240 patients. | CELLxGENE bulk (.h5ad / .rds) · Zenodo | h5ad / rds | core few-GB; extended ~10 GB `~est` | core ~min · extended ~13 min | Static (Neuro-Oncology 2025, doi:10.1093/neuonc/noaf113); Zenodo `10.5281/zenodo.6962901` |

---

## 4. Aggregator portals & functional genomics

| Source | Open scope | Access | Typical GBM pull | Cadence |
|---|---|---|---|---|
| **cBioPortal** | Fully open public instance + open REST API. Glioma studies `gbm_tcga`, `lgg_tcga`, CPTAC-GBM, MSK panels. | REST (OpenAPI) + per-study tarball | study tarball ~tens of MB (~secs) | rolling additions |
| **UCSC Xena** | Fully open public hubs (TCGA, GDC, Treehouse, PCAWG). | Xena Hubs + `UCSCXenaTools` (R/py) + browser | TCGA-GBM/LGG matrix ~tens–hundreds MB (~secs–1 min) | tracks upstream GDC |
| **DepMap / CCLE** | Fully open, unrestricted. ~60+ CNS/glioma lines; CRISPR (Chronos) + expression. | Portal CSV bulk + Custom Downloads + AWS Open Data mirror | key files ~hundreds MB each (~24 s/file); full release multi-GB | **quarterly** (cur. 25Q2) |
| **GDSC + PRISM** | Both fully open. GDSC dose-response (~1000 lines); PRISM Repurposing (1448 cmpds × 499 lines). | GDSC: Sanger FTP mirror (recommended) / web · PRISM: DepMap portal CSV | GDSC ~tens MB (~secs); PRISM secondary ~hundreds MB (~tens s) | GDSC v8.5; PRISM 19Q4 static · ⚠️ `cancerrxgene.org` web DL returned HTTP 410 to automated clients — use **Sanger FTP** `ftp.sanger.ac.uk/pub/project/cancerrxgene/releases/current_release/` |

---

## 5. Knowledge bases & clinical (KG / provenance feeds)

| Source | Open scope | Access | Size | Cadence |
|---|---|---|---|---|
| **CIViC** | Fully open, **public domain (CC0)**. Curated variant/therapeutic evidence (IDH1, EGFRvIII, BRAF V600E, MGMT context…). | GraphQL API + nightly/monthly TSV & VCF dumps (`CIViCpy`) | dumps a few MB (~secs) | **nightly** TSV/VCF + monthly tags |
| **ClinicalTrials.gov** | Fully open, no API key. (Oncora already wires the Clinical Trials MCP.) | REST API v2 (JSON/CSV, OpenAPI 3.0) + full bulk | GBM result set ~tens MB (~secs); full registry GB-scale | refreshed **daily** (Mon–Fri ~14:00 UTC) |

---

## Aggregate sizing (planning estimate)

Tiered, because imaging dominates. At ~100 Mbps:

| Tier | Contents | Size | Time |
|---|---|---|---|
| **A — Omics + single-cell + KG + clinical** | §1 (open subsets) + §3 + §4 + §5 | ~25–60 GB `~est` | ~35 min – 1.3 hr |
| **B — + MRI imaging** | A + UPENN-GBM (NIfTI 69 GB) + UCSF-PDGM (142 GB) | +~211 GB | ~5–6 hr total |
| **C — + digital pathology WSI** | B + TCGA-GBM/LGG slides | + hundreds of GB–TB | **hours–days** (budget separately) |

**Takeaway:** everything *except* imaging is an overnight-or-faster pull and trivially snapshot-able. MRI adds a few hours; pathology WSIs are the only multi-day, multi-TB commitment — pin a slide subset rather than the full corpus unless histopathology is in scope.

**Recommended first four** (coverage per effort): **cBioPortal** (omics+clinical, instant API) · **CGGA** (large independent validation cohort) · **UPENN-GBM** (imaging-with-labels) · **CIViC** (CC0 KG seed).

---

## 6. Excluded (not fully-open)

Logged so they aren't silently rediscovered as "open":

- **GLASS** (primary↔recurrent gliomas) — Synapse account required.
- **BraTS** (segmentation-labelled mpMRI) — Synapse registration required.
- **Broad Single Cell Portal** downloads (incl. Neftel `SCP393`) — login required → use GEO `GSE131928`.
- **TCGA-GBM radiology MRI** (TCIA) — NIH Controlled-Data-Access flag (face-reconstructable). *(Pathology WSIs are separately open — kept in §2.)*
- **RHUH-GBM** (TCIA) — NIH Controlled-Data-Access flag on raw MR.
- **dbGaP-controlled tiers** of TCGA/CPTAC/ICGC (raw reads, germline, US-project calls), **Kids First / PBTA controlled** pediatric data, **HTAN controlled** atlases.

---

*Light-verification caveats:* GDC and ClinicalTrials portals are client-rendered (live, but won't render via plain HTTP fetch); `cancerrxgene.org` returned HTTP 410 to automated clients (browser-only / use FTP). Re-verify URLs at snapshot time — provenance requires it.
