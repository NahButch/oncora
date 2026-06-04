#!/usr/bin/env python3
"""Static-site generator for the Oncora docs.

Converts docs/*.md + README.md into a styled HTML site under site/,
with hand-authored SVG illustrations, vendored Mermaid + highlight.js,
a sidebar nav, page banners, and prev/next pagers. No network at build
time; the output works offline when served over http.
"""
import re, html, pathlib, datetime
import markdown

ROOT = pathlib.Path(__file__).resolve().parent.parent
DOCS = ROOT / "docs"
SITE = ROOT / "site"

# ---- mermaid as a superfences custom fence -> <pre class="mermaid"> ----
def mermaid_fence(source, language, css_class, options, md, **kwargs):
    # Escape so the browser decodes textContent back to the original diagram.
    return f'<pre class="mermaid">{html.escape(source)}</pre>'

MD_EXT = [
    "tables", "toc", "attr_list", "def_list", "sane_lists", "md_in_html",
    "pymdownx.superfences", "pymdownx.highlight", "pymdownx.betterem",
    "pymdownx.tilde", "pymdownx.tasklist",
]
MD_CFG = {
    "pymdownx.highlight": {"use_pygments": False, "guess_lang": False},
    "pymdownx.superfences": {
        "custom_fences": [
            {"name": "mermaid", "class": "mermaid", "format": mermaid_fence}
        ]
    },
    "toc": {"permalink": "#", "permalink_class": "headerlink"},
}

# slug, nav-title, H1-fallback, banner svg (or None)
PAGES = [
    ("00-overview",              "Overview",            "banner-pillars"),
    ("01-architecture",          "Architecture",        "banner-architecture"),
    ("02-memory",                "Agent memory",        "banner-memory"),
    ("03-uncertainty-reliability","Uncertainty",        "banner-uncertainty"),
    ("04-knowledge-and-data",    "Knowledge & data",    "banner-ingestion"),
    ("05-tech-decisions",        "Tech decisions",      "banner-tech"),
    ("06-eval-benchmarking",     "Eval & benchmarks",   "banner-eval"),
    ("07-repo-layout",           "Repo layout",         "banner-repo"),
    ("08-roadmap",               "Roadmap & ops",       "banner-roadmap"),
    ("09-validation",            "Validation report",   "banner-eval"),
    ("10-cold-compare",          "Cold-start & SQLite", "banner-tech"),
]
NAV_GROUPS = [
    ("Start", [("index", "Home")] + [(s, t) for (s, t, _) in PAGES[:1]]),
    ("Architecture", [(s, t) for (s, t, _) in PAGES[1:5]]),
    ("Engineering", [(s, t) for (s, t, _) in PAGES[5:]]),
]
BANNERS = {s: b for (s, _, b) in PAGES}
# banner-pillars is really pillars.svg
SVG_FILE = {"banner-pillars": "pillars.svg"}

def svg_for(banner):
    if banner is None:
        return None
    return SVG_FILE.get(banner, banner + ".svg")

def rewrite_links(htmltext):
    def repl(m):
        url = m.group(1)
        if url.startswith(("http://", "https://", "mailto:", "#")):
            return m.group(0)
        anchor = ""
        if "#" in url:
            url, anchor = url.split("#", 1)
            anchor = "#" + anchor
        base = url.rsplit("/", 1)[-1]
        if base == "README.md":
            base = "index.html"
        elif base.endswith(".md"):
            base = base[:-3] + ".html"
        else:
            return m.group(0)
        return f'href="{base}{anchor}"'
    return re.sub(r'href="([^"]+)"', repl, htmltext)

def wrap_tables(htmltext):
    htmltext = htmltext.replace("<table>", '<div class="table-wrap"><table>')
    htmltext = htmltext.replace("</table>", "</table></div>")
    return htmltext

def render_md(path):
    md = markdown.Markdown(extensions=MD_EXT, extension_configs=MD_CFG)
    text = path.read_text(encoding="utf-8")
    out = md.convert(text)
    return wrap_tables(rewrite_links(out))

def nav_html(active):
    rows = []
    for group, items in NAV_GROUPS:
        rows.append(f'<div class="group">{group}</div>')
        for slug, title in items:
            href = "index.html" if slug == "index" else f"{slug}.html"
            num = "" if slug == "index" else f'<span class="num">{slug[:2]}</span>'
            cls = " active" if slug == active else ""
            rows.append(f'<a class="nav-link{cls}" href="{href}">{num}<span>{title}</span></a>')
    return "\n".join(rows)

SHELL = """<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>{title} · Oncora</title>
<meta name="description" content="{desc}"/>
<link rel="icon" href="assets/svg/oncora-mark.svg"/>
<link rel="stylesheet" href="assets/css/hljs-github-dark.css"/>
<link rel="stylesheet" href="assets/css/style.css"/>
</head>
<body>
<button class="menu-btn" aria-label="Menu" onclick="document.body.classList.toggle('nav-open')">☰</button>
<div class="scrim" onclick="document.body.classList.remove('nav-open')"></div>
<div class="layout">
<aside class="sidebar" id="sidebar">
  <a class="brand" href="index.html">
    <img src="assets/svg/oncora-mark.svg" alt="Oncora"/>
    <span>
      <span class="name">Onc<span>ora</span></span>
      <span class="sub">Oncology Reasoning Agents</span>
    </span>
  </a>
  <nav class="nav">
{nav}
  </nav>
  <div class="foot">
    Rust-native · on-prem · MCP<br/>
    <a href="https://github.com/NahButch/oncora">github.com/NahButch/oncora</a><br/>
    <span style="color:#475569">Built {date}</span>
  </div>
</aside>
<main class="main">
{body}
</main>
</div>
<script>
function syncScrim(){{document.querySelector('.scrim').classList.toggle('show',document.body.classList.contains('nav-open'));
  document.getElementById('sidebar').classList.toggle('open',document.body.classList.contains('nav-open'));}}
new MutationObserver(syncScrim).observe(document.body,{{attributes:true,attributeFilter:['class']}});
</script>
<script src="assets/js/highlight.min.js"></script>
<script src="assets/js/rust.min.js"></script>
<script src="assets/js/mermaid.min.js"></script>
<script>
  document.querySelectorAll('pre code').forEach(function(el){{ try{{ hljs.highlightElement(el); }}catch(e){{}} }});
  mermaid.initialize({{
    startOnLoad:true, securityLevel:'loose',
    theme:'base',
    themeVariables:{{
      darkMode:true, background:'#0c1726',
      primaryColor:'#13233c', primaryTextColor:'#e6edf6', primaryBorderColor:'#2dd4bf',
      lineColor:'#7dd3fc', secondaryColor:'#16243d', tertiaryColor:'#0e1b2e',
      fontFamily:"'Segoe UI',system-ui,sans-serif", fontSize:'15px',
      clusterBkg:'#0e1b2e', clusterBorder:'#2b405f',
      actorBkg:'#13233c', actorBorder:'#2dd4bf', actorTextColor:'#e6edf6',
      signalColor:'#9fb3c8', signalTextColor:'#cbd9ea', labelBoxBkgColor:'#13233c', labelBoxBorderColor:'#2b405f'
    }}
  }});
</script>
</body>
</html>
"""

def pager_html(idx):
    parts = ['<div class="pager">']
    if idx > 0:
        s, t, _ = PAGES[idx-1]
        parts.append(f'<a class="prev" href="{s}.html"><span class="lbl">Previous</span><span class="ttl">{t}</span></a>')
    else:
        parts.append('<a class="prev" href="index.html"><span class="lbl">Previous</span><span class="ttl">Home</span></a>')
    if idx < len(PAGES)-1:
        s, t, _ = PAGES[idx+1]
        parts.append(f'<a class="next" href="{s}.html"><span class="lbl">Next</span><span class="ttl">{t}</span></a>')
    parts.append("</div>")
    return "\n".join(parts)

def build():
    SITE.mkdir(exist_ok=True)
    today = datetime.date(2026, 6, 3).isoformat()

    # ---- doc pages ----
    for idx, (slug, title, banner) in enumerate(PAGES):
        body_md = render_md(DOCS / f"{slug}.md")
        svg = svg_for(banner)
        banner_html = (f'<div class="page-banner"><img src="assets/svg/{svg}" alt="{title} illustration"/></div>'
                       if svg else "")
        body = f"""{banner_html}
<article class="content">
{body_md}
{pager_html(idx)}
</article>"""
        page = SHELL.format(title=title, desc=f"Oncora — {title}", nav=nav_html(slug),
                            body=body, date=today)
        (SITE / f"{slug}.html").write_text(page, encoding="utf-8")

    # ---- home page ----
    cards = []
    blurbs = {
        "00-overview": "Executive summary, goals, the four pillars.",
        "01-architecture": "Containers, agent runtime, the end-to-end workflow.",
        "02-memory": "Five memory types, write/consolidation and hybrid read.",
        "03-uncertainty-reliability": "Typed confidence, calibration, abstain/escalate.",
        "04-knowledge-and-data": "Ingestion, KG schema, hybrid retrieval.",
        "05-tech-decisions": "Rust survey, decision tables, risk register.",
        "06-eval-benchmarking": "Golden sets, metrics, CI gating, replay.",
        "07-repo-layout": "Cargo workspace, crates, trait boundaries.",
        "08-roadmap": "Deployment, governance, phased roadmap.",
    }
    for slug, title, _ in PAGES:
        cards.append(f'''<a class="card" href="{slug}.html">
  <div class="k">{slug}</div>
  <h3>{title}</h3>
  <p>{blurbs.get(slug,"")}</p>
</a>''')
    cards_html = "\n".join(cards)
    home_body = f"""
<section class="hero">
  <div class="eyebrow">Oncology Reasoning Agents</div>
  <h1>A reproducible, uncertainty-aware agentic platform for oncology drug discovery</h1>
  <p class="lede">Rust-native, on-prem reasoning agents that work across literature, multi-omics,
  a biomolecular knowledge graph, and imaging — every claim carries provenance and calibrated
  confidence, and the system abstains rather than confabulates.</p>
  <div class="hero-cta">
    <a class="btn primary" href="00-overview.html">Read the overview →</a>
    <a class="btn" href="01-architecture.html">Architecture</a>
    <a class="btn" href="https://github.com/NahButch/oncora">GitHub</a>
  </div>
  <div class="badges">
    <span class="badge"><b>Rust</b> end to end</span>
    <span class="badge">tools via <b>MCP</b> (rmcp)</span>
    <span class="badge"><b>on-prem</b> / VPC-only</span>
    <span class="badge">typed <b>uncertainty</b></span>
    <span class="badge"><b>reproducible</b> replay</span>
  </div>
</section>
<div class="hero-art">
  <img src="assets/svg/hero-overview.svg" alt="Publications and multimodal data flow into the Oncora engine and out to databases, AI, and a cited confidence-scored answer"/>
</div>
<section class="content" style="max-width:1100px">
  <h2 style="border:none">The four pillars</h2>
  <div class="page-banner" style="padding:0;margin:0 0 8px"><img src="assets/svg/pillars.svg" alt="The four pillars"/></div>
  <h2>Explore the design</h2>
  <div class="cards">
{cards_html}
  </div>
  <blockquote>This site is generated from the Markdown specification in <code>docs/</code>.
  The Mermaid diagrams and SVG illustrations render in the browser; serve the folder over http for full fidelity.</blockquote>
</section>
"""
    home = SHELL.format(title="Home", desc="Oncora — reproducible, uncertainty-aware agentic platform for oncology drug discovery.",
                        nav=nav_html("index"), body=home_body, date=today)
    (SITE / "index.html").write_text(home, encoding="utf-8")

    print(f"Built {len(PAGES)+1} pages into {SITE}")

if __name__ == "__main__":
    build()
