#!/usr/bin/env python3
"""Bulk-fetch PubMed abstracts via NCBI E-utilities into the input-data folder.

esearch each topic for PMIDs, dedup + cap at TARGET, then efetch abstracts in
batches and write per-article .txt files + corpus.jsonl. Source: PubMed.
Polite rate limiting (no API key). DOIs preserved per record.
"""
import json, time, pathlib, urllib.parse, urllib.request, xml.etree.ElementTree as ET

OUT = pathlib.Path("/home/tom_b/oncora-input-data")
(OUT / "docs").mkdir(parents=True, exist_ok=True)
EUTILS = "https://eutils.ncbi.nlm.nih.gov/entrez/eutils"
TARGET = 15000
PER_TOPIC = 1500
TOPICS = [
    "glioblastoma", "glioma", "glioblastoma immunotherapy", "IDH mutant glioma",
    "low grade glioma", "glioma stem cells", "glioblastoma temozolomide",
    "brain tumor", "diffuse midline glioma", "oligodendroglioma",
    "astrocyte", "microglia", "neuroinflammation", "astrocyte inflammation",
    "glial inflammation", "microglia neuroinflammation",
    "reactive astrocytes neurodegeneration", "neuroinflammation neurodegeneration",
    "tumor microenvironment glioma", "blood brain barrier inflammation",
]

def get(url):
    for attempt in range(4):
        try:
            with urllib.request.urlopen(url, timeout=30) as r:
                return r.read()
        except Exception as e:
            time.sleep(1.0 + attempt)
    raise RuntimeError(f"failed: {url}")

# 1) esearch -> ordered unique PMIDs with first-seen topic
pmid_topic = {}
order = []
for topic in TOPICS:
    q = urllib.parse.quote(topic)
    url = (f"{EUTILS}/esearch.fcgi?db=pubmed&term={q}&retmax={PER_TOPIC}"
           f"&sort=relevance&datetype=pdat&mindate=2018&maxdate=2025&retmode=json")
    d = json.loads(get(url))
    ids = d["esearchresult"]["idlist"]
    for pid in ids:
        if pid not in pmid_topic:
            pmid_topic[pid] = topic
            order.append(pid)
    print(f"esearch {topic!r}: +{len(ids)} (unique total {len(order)})")
    time.sleep(0.4)

pmids = order[:TARGET]
print(f"fetching abstracts for {len(pmids)} PMIDs")

# 2) efetch abstracts in batches
def text_of(el):
    return "".join(el.itertext()).strip() if el is not None else ""

records = []
BATCH = 200
for i in range(0, len(pmids), BATCH):
    chunk = pmids[i:i + BATCH]
    url = f"{EUTILS}/efetch.fcgi?db=pubmed&id={','.join(chunk)}&rettype=abstract&retmode=xml"
    xml = get(url)
    root = ET.fromstring(xml)
    for art in root.findall(".//PubmedArticle"):
        pmid = text_of(art.find(".//MedlineCitation/PMID"))
        title = text_of(art.find(".//Article/ArticleTitle"))
        parts = [text_of(a) for a in art.findall(".//Abstract/AbstractText")]
        abstract = " ".join(p for p in parts if p).strip()
        doi = ""
        for aid in art.findall(".//ArticleIdList/ArticleId"):
            if aid.get("IdType") == "doi":
                doi = (aid.text or "").strip()
        journal = text_of(art.find(".//Journal/Title"))
        year = text_of(art.find(".//JournalIssue/PubDate/Year"))
        if len(abstract) < 50:
            continue
        topic = pmid_topic.get(pmid, "pubmed")
        records.append({"id": f"PMID:{pmid}", "pmid": pmid, "doi": doi, "topic": topic,
                        "title": title, "journal": journal, "date": year, "text": abstract})
    print(f"  efetch batch {i // BATCH + 1}: parsed total {len(records)}")
    time.sleep(0.4)

# 3) write corpus + per-article txt files
for r in records:
    (OUT / "docs" / f"{r['topic'].replace(' ', '-')}_PMID{r['pmid']}.txt").write_text(
        f"PMID: {r['pmid']}\nDOI: {r['doi']}\nTopic: {r['topic']}\nTitle: {r['title']}\n"
        f"Journal: {r['journal']}\nYear: {r['date']}\n\n{r['text']}\n\n"
        f"Source: PubMed (https://pubmed.ncbi.nlm.nih.gov/{r['pmid']}/)\n", encoding="utf-8")
with (OUT / "corpus.jsonl").open("w", encoding="utf-8") as f:
    for r in records:
        f.write(json.dumps(r, ensure_ascii=False) + "\n")

from collections import Counter
c = Counter(r["topic"] for r in records)
print(f"\nDONE: {len(records)} docs")
for t, n in sorted(c.items()):
    print(f"  {t}: {n}")
