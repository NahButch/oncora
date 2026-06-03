# Oncora docs site
PY := build/venv/bin/python

.PHONY: site serve setup clean

setup:          ## create the build venv and install the markdown toolchain
	python3 -m venv build/venv
	build/venv/bin/pip install -q --upgrade pip
	build/venv/bin/pip install -q markdown pymdown-extensions

site:           ## generate the static HTML site into site/
	$(PY) build/build_site.py

serve: site     ## build, then serve the site over http at :8137
	$(PY) -m http.server 8137 --directory site

clean:          ## remove generated HTML (keeps assets + sources)
	rm -f site/*.html
