"""Build the project website into site/dist/.

    python -m pip install -r site/requirements.txt
    python site/build.py

The home page is site/templates/home.html filled with sections of README.md;
the tutorial, guide, troubleshooting and changelog pages are rendered from
docs/FIRST_MOD.md, docs/MODEL_PREPARATION.md, docs/TROUBLESHOOTING.md and
CHANGELOG.md. Relative links in the Markdown are rewritten to site pages,
copied images or GitHub URLs. After writing, every internal link, image and
#anchor in the output is checked, and the build fails (exit code 1) on any
problem.

Languages come from site/languages.py. English pages are at the site root;
each other language has a home page built from its own README (README.zh-CN.md)
and its own pages below its prefix (zh-cn/). Pages that exist in several
languages point at each other with hreflang links, and every page has a
language menu in its header.
"""

from __future__ import annotations

import datetime
import html
import json
import posixpath
import re
import shutil
import struct
import subprocess
import sys
from html.parser import HTMLParser
from pathlib import Path
from string import Template
from urllib.parse import quote, unquote, urlsplit

try:
    from markdown_it import MarkdownIt
    from markdown_it.token import Token
    from mdit_py_plugins.anchors import anchors_plugin
except ImportError:
    sys.exit("error: markdown-it-py and mdit-py-plugins are missing. Run: python -m pip install -r site/requirements.txt")

from languages import ENGLISH, LANGUAGES, Language, Page, display_width

SITE = Path(__file__).resolve().parent
ROOT = SITE.parent
DIST = SITE / "dist"
TEMPLATES = SITE / "templates"
STATIC = SITE / "static"
IMAGES_SRC = "docs/images"


class BuildError(Exception):
    pass


HOME = ENGLISH.home
# Markdown sources that become English site pages, keyed by repository path.
DOC_PAGES = ENGLISH.docs
TUTORIAL = DOC_PAGES["docs/FIRST_MOD.md"]

NOT_FOUND = Page(
    path="404.html",
    title="Page not found – Deadlock Mod Builder",
    description="This page does not exist on the Deadlock Mod Builder site.",
)

LANGUAGE_BY_CODE = {lang.code: lang for lang in LANGUAGES}
# Every Markdown source that becomes a page, in any language.
SOURCES: dict[str, Page] = {source: page for lang in LANGUAGES for source, page in lang.docs.items()}
READMES: dict[str, Language] = {lang.readme: lang for lang in LANGUAGES}


def language_of(page: Page) -> Language:
    return LANGUAGE_BY_CODE[page.lang]


def versions(page: Page) -> dict[str, Page]:
    """The page in every language that has it, keyed by language code (English first)."""
    if not page.key:
        return {}
    found = {}
    for lang in LANGUAGES:
        for candidate in [lang.home, *lang.docs.values()]:
            if candidate.key == page.key:
                found[lang.code] = candidate
    return found


def tutorial_of(lang: Language) -> Page:
    for page in lang.docs.values():
        if page.key == "tutorial":
            return page
    return TUTORIAL


# ---------------------------------------------------------------- helpers

def read_text(rel: str) -> str:
    path = ROOT / rel
    if not path.is_file():
        raise BuildError(f"missing source file: {rel}")
    return path.read_text(encoding="utf-8").replace("\r\n", "\n")


def write_text(rel: str, text: str) -> None:
    out = DIST / rel
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(text, encoding="utf-8", newline="\n")


def esc(text: str) -> str:
    return html.escape(text, quote=True)


def load_config() -> dict:
    try:
        config = json.loads((SITE / "site.json").read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        raise BuildError(f"cannot read site/site.json: {error}") from error
    for key in ("name", "baseUrl", "repoUrl", "branch", "googleSiteVerification"):
        if not isinstance(config.get(key), str):
            raise BuildError(f"site/site.json: '{key}' must be a string")
    if not re.fullmatch(r"https://[^/]+/(?:[^/]+/)*", config["baseUrl"]):
        raise BuildError("site/site.json: 'baseUrl' must be an https URL ending in '/'")
    config["repoUrl"] = config["repoUrl"].rstrip("/")
    config["basePath"] = urlsplit(config["baseUrl"]).path  # "/deadlock-mod-builder/"
    repo_name = urlsplit(config["repoUrl"]).path.strip("/")  # "<owner>/<repository>"
    if not re.fullmatch(r"[^/]+/[^/]+", repo_name):
        raise BuildError("site/site.json: 'repoUrl' must end in '<owner>/<repository>'")
    config["repoName"] = repo_name
    config["repoOwner"] = repo_name.split("/")[0]
    return config


def png_size(path: Path) -> tuple[int, int]:
    data = path.read_bytes()[:24]
    if data[:8] != b"\x89PNG\r\n\x1a\n" or data[12:16] != b"IHDR":
        raise BuildError(f"not a PNG file: {path.relative_to(ROOT)}")
    return struct.unpack(">II", data[16:24])


def git_date(*files: str) -> str:
    """Latest commit date (YYYY-MM-DD) of the given files, or today without git history."""
    dates = []
    for name in files:
        try:
            result = subprocess.run(
                ["git", "log", "-1", "--format=%cs", "--", name],
                cwd=ROOT, capture_output=True, text=True, check=True,
            )
            if re.fullmatch(r"\d{4}-\d{2}-\d{2}", result.stdout.strip()):
                dates.append(result.stdout.strip())
        except (OSError, subprocess.CalledProcessError):
            pass
    return max(dates) if dates else datetime.date.today().isoformat()


def rel_url(from_page: str, to: str) -> str:
    """Relative URL from the page at URL path from_page to the site path `to`."""
    from_dir = posixpath.dirname(from_page)  # "guide/" -> "guide", "" -> ""
    target = to.rstrip("/") or "."
    rel = posixpath.relpath(target, from_dir or ".")
    if to == "" or to.endswith("/"):
        return "./" if rel == "." else rel + "/"
    return rel


# ---------------------------------------------------------------- Markdown

def heading_text(inline) -> str:
    return "".join(c.content for c in (inline.children or []) if c.type in ("text", "code_inline"))


def make_markdown(config: dict) -> MarkdownIt:
    """CommonMark with GitHub's tables and strikethrough, GitHub-style heading ids,
    and the site's link rewriting. Raw HTML is escaped (GitHub sanitizes it)."""
    md = MarkdownIt("commonmark", {"html": False}).enable(["table", "strikethrough"])
    md.use(anchors_plugin, min_level=1, max_level=6)

    def rewrite(url: str, env: dict) -> tuple[str, Page | None, bool]:
        """The site URL for a Markdown link, the site page it goes to, and whether
        that target is in another language than the page being built."""
        parts = urlsplit(url)
        if parts.scheme or parts.netloc or url.startswith("#"):
            return url, None, False
        source = env["source"]
        lang: Language = env["lang"]
        target = posixpath.normpath(posixpath.join(posixpath.dirname(source), unquote(parts.path)))
        if target == ".." or target.startswith("../"):
            raise BuildError(f"{source}: link '{url}' leaves the repository")
        fragment = f"#{parts.fragment}" if parts.fragment else ""
        if target in SOURCES:
            page = SOURCES[target]
            foreign = page.lang != lang.code
            if page.path == env["page"] and fragment:
                return fragment, page, foreign
            return rel_url(env["page"], page.path) + fragment, page, foreign
        if target in READMES:
            other = READMES[target]
            return rel_url(env["page"], other.home.path) + fragment, None, other is not lang
        if not (ROOT / target).exists():
            raise BuildError(f"{source}: link '{url}' points to {target}, which is not in the repository")
        if target.startswith(IMAGES_SRC + "/"):
            return rel_url(env["page"], "images/" + posixpath.basename(target)), None, False
        kind = "tree" if (ROOT / target).is_dir() else "blob"
        # Repository Markdown that is not one of this language's sources is English.
        foreign = target.endswith(".md") and lang is not ENGLISH
        return f"{config['repoUrl']}/{kind}/{config['branch']}/{quote(target)}{fragment}", None, foreign

    def site_links(state) -> None:
        env = state.env
        lang: Language = env["lang"]
        stack = list(state.tokens)
        while stack:
            token = stack.pop()
            if token.children:
                children = token.children
                for i, child in enumerate(children):
                    if child.type == "link_open":
                        original = child.attrGet("href") or ""
                        new, page, foreign = rewrite(original, env)
                        child.attrSet("href", new)
                        # A link that shows a file name ("docs/X.md") reads better as the page name.
                        if page and i + 2 < len(children) and children[i + 2].type == "link_close":
                            text = children[i + 1]
                            if text.type == "text" and text.content in (original, posixpath.basename(original)):
                                text.content = page.label
                        if foreign and lang.foreign_note:
                            if page:
                                child.attrSet("hreflang", page.lang)
                            close = next((j for j in range(i + 1, len(children)) if children[j].type == "link_close"), None)
                            after = children[close + 1] if close is not None and close + 1 < len(children) else None
                            if after is None or after.type != "text" or not after.content.startswith(lang.foreign_note):
                                raise BuildError(
                                    f"{env['source']}: the link '{original}' goes to a page in another language; "
                                    f"write '{lang.foreign_note}' right after it"
                                )
                    elif child.type == "softbreak" and lang is not ENGLISH and 0 < i < len(children) - 1:
                        # Browsers show a line break inside a paragraph as a space,
                        # which splits Chinese text: write such paragraphs on one line.
                        before, after = children[i - 1].content[-1:], children[i + 1].content[:1]
                        texts = children[i - 1].type == "text" and children[i + 1].type == "text"
                        if texts and (display_width(before) == 2 or display_width(after) == 2):
                            raise BuildError(
                                f"{env['source']}: a line break inside a paragraph next to '{before}{after}' "
                                "would show as a space; write the paragraph on one line"
                            )
                    elif child.type == "image":
                        src = child.attrGet("src") or ""
                        new, _, _ = rewrite(src, env)
                        child.attrSet("src", new)
                        if not urlsplit(src).scheme:
                            target = posixpath.normpath(posixpath.join(posixpath.dirname(env["source"]), unquote(src)))
                            width, height = png_size(ROOT / target)
                            child.attrSet("width", str(width))
                            child.attrSet("height", str(height))
                        child.attrSet("loading", "lazy")
                        child.attrSet("decoding", "async")

    md.core.ruler.push("site_links", site_links)

    def figures(state) -> None:
        """A paragraph that is only an image, a line break and an italic line (the
        README's screenshot pattern) becomes <figure class="shot"> with a caption."""
        tokens = state.tokens
        for i in range(len(tokens) - 2):
            if tokens[i].type != "paragraph_open" or tokens[i + 2].type != "paragraph_close":
                continue
            kids = tokens[i + 1].children or []
            if (len(kids) < 5 or kids[0].type != "image" or kids[1].type != "softbreak"
                    or kids[2].type != "em_open" or kids[-1].type != "em_close"
                    or any(k.type == "em_close" for k in kids[3:-1])):
                continue
            tokens[i].tag = tokens[i + 2].tag = "figure"
            tokens[i].attrSet("class", "shot")
            tokens[i].hidden = tokens[i + 2].hidden = False
            kids[1] = Token("html_inline", "", 0, content="<figcaption>")
            kids[2] = Token("html_inline", "", 0, content="")
            kids[-1] = Token("html_inline", "", 0, content="</figcaption>")

    md.core.ruler.push("figures", figures)

    def table_open(self, tokens, idx, options, env):
        label = esc(env["lang"].text["table"])
        return f'<div class="table-wrap" tabindex="0" role="region" aria-label="{label}">\n<table>\n'

    def table_close(self, tokens, idx, options, env):
        return "</table>\n</div>\n"

    def heading_close(self, tokens, idx, options, env):
        tag = tokens[idx].tag
        open_token = tokens[idx - 2]
        slug = open_token.attrGet("id")
        if tag == "h1" or not slug:
            return f"</{tag}>\n"
        # Mouse convenience only: the table of contents is the keyboard route.
        return f'<a class="anchor" href="#{esc(slug)}" aria-hidden="true" tabindex="-1">#</a></{tag}>\n'

    md.add_render_rule("table_open", table_open)
    md.add_render_rule("table_close", table_close)
    md.add_render_rule("heading_close", heading_close)
    return md


def make_env(source: str, page: str, lang: Language) -> dict:
    return {"source": source, "page": page, "lang": lang}


def render(md: MarkdownIt, text: str, source: str, page: str, lang: Language = ENGLISH) -> tuple[str, list]:
    env = make_env(source, page, lang)
    tokens = md.parse(text, env)
    return md.renderer.render(tokens, md.options, env), tokens


def render_inline(md: MarkdownIt, text: str, source: str, page: str, lang: Language = ENGLISH) -> str:
    env = make_env(source, page, lang)
    tokens = md.parseInline(text, env)
    return md.renderer.render(tokens, md.options, env)


# ---------------------------------------------------------------- README parsing

def readme_sections(text: str) -> tuple[str, dict[str, str]]:
    intro: list[str] = []
    sections: dict[str, list[str]] = {}
    current = None
    for line in text.split("\n"):
        if line.startswith("## "):
            current = line[3:].strip()
            sections[current] = []
        elif line.startswith("# "):
            continue
        elif current is None:
            intro.append(line)
        else:
            sections[current].append(line)
    return "\n".join(intro), {k: "\n".join(v).strip() + "\n" for k, v in sections.items()}


def require(sections: dict[str, str], name: str, lang: Language = ENGLISH) -> str:
    local = lang.sections.get(name, name)
    if local not in sections:
        if lang is ENGLISH:
            raise BuildError(f"README.md has no '## {name}' section; the home page needs it")
        raise BuildError(
            f"{lang.readme} has no '## {local}' section (README.md's '## {name}'); the home page needs it"
        )
    return sections[local]


def sentence_case(text: str) -> str:
    return text[:1].upper() + text[1:]


FIGURE =re.compile(r"^[ \t]*!\[([^\]]*)\]\(([^)\s]+)\)[ \t]*\n[ \t]*\*(.+?)\*[ \t]*$", re.M)

# The line near the top of each README that links the other languages' READMEs,
# for example "**English** · [简体中文](README.zh-CN.md)". It is not part of the home page.
_LANGUAGE_ITEM = r"(?:\*\*[^*\n]+\*\*|\[[^\]\n]+\]\(README(?:\.[A-Za-z-]+)?\.md\))"
LANGUAGE_LINE = re.compile(rf"{_LANGUAGE_ITEM}(?:[ \t]*·[ \t]*{_LANGUAGE_ITEM})+")


def check_readmes(readmes: dict[str, str]) -> None:
    """Every README has the sections of README.md, in the same order, under the
    names languages.py gives them, the same language line and the same pictures,
    so the languages' home pages cannot drift apart silently."""
    english_intro, english = readme_sections(readmes[ENGLISH.readme])
    listed = list(ENGLISH.sections)
    if list(english) != listed:
        raise BuildError(
            "README.md's '##' sections are " + ", ".join(english)
            + "; site/languages.py lists " + ", ".join(listed)
            + ". Update the section lists of every language in site/languages.py and every README together."
        )
    english_pictures = FIGURE.findall(readmes[ENGLISH.readme])
    for lang in LANGUAGES:
        text = readmes[lang.readme]
        intro, sections = readme_sections(text)
        expected = [lang.sections[name] for name in listed]
        for name, local in zip(listed, expected):
            if local not in sections:
                raise BuildError(f"{lang.readme} has no '## {local}' section, the version of README.md's '## {name}'")
        if list(sections) != expected:
            raise BuildError(
                f"{lang.readme}'s '##' sections are " + ", ".join(sections)
                + "; expected, in README.md's order: " + ", ".join(expected)
            )
        if len(LANGUAGES) > 1 and not any(LANGUAGE_LINE.fullmatch(p.strip()) for p in intro.split("\n\n")):
            raise BuildError(f"{lang.readme}: the language line (for example '**English** · [简体中文](README.zh-CN.md)') is missing")
        for other in LANGUAGES:
            if other is not lang and f"]({other.readme})" not in intro:
                raise BuildError(f"{lang.readme}: the language line does not link {other.readme}")
        pictures = [src for _, src, _ in FIGURE.findall(text)]
        if pictures != [src for _, src, _ in english_pictures]:
            raise BuildError(
                f"{lang.readme} shows the pictures {pictures}; README.md shows {[s for _, s, _ in english_pictures]}"
            )


def figure_html(md, alt, src, caption, page, lazy=True, lang: Language = ENGLISH) -> str:
    target = posixpath.normpath(unquote(src))
    if not target.startswith(IMAGES_SRC + "/") or not (ROOT / target).is_file():
        raise BuildError(f"{lang.readme}: screenshot '{src}' is not a file under {IMAGES_SRC}/")
    width, height = png_size(ROOT / target)
    url = rel_url(page, "images/" + posixpath.basename(target))
    loading = ' loading="lazy" decoding="async"' if lazy else ' fetchpriority="high"'
    return (
        f'<figure class="shot"><img src="{esc(url)}" alt="{esc(alt)}" width="{width}" height="{height}"{loading}>'
        f"<figcaption>{render_inline(md, caption, lang.readme, page, lang)}</figcaption></figure>"
    )


def build_home(md, config, readme: str, version: str, lang: Language = ENGLISH) -> str:
    home = lang.home
    source = lang.readme
    intro, sections = readme_sections(readme)
    paragraphs = [
        p.strip() for p in intro.split("\n\n")
        if p.strip() and not p.strip().startswith("[![") and not LANGUAGE_LINE.fullmatch(p.strip())
    ]
    if len(paragraphs) < 2:
        raise BuildError(f"{source}: expected an opening paragraph and a screenshot before the first '##' section")
    opening = " ".join(paragraphs[0].split())
    pitch, _, lead = opening.partition(lang.sentence_end)
    # The h1 is the app name; avoid repeating it at the start of the pitch.
    prefix = lang.pitch_prefix.format(name=config["name"])
    if pitch.startswith(prefix):
        pitch = sentence_case(pitch[len(prefix):])
    if not lead:
        raise BuildError(f"{source}: the opening paragraph should have more than one sentence")
    hero_match = FIGURE.search(paragraphs[1])
    if not hero_match:
        raise BuildError(f"{source}: expected a screenshot with an italic caption after the opening paragraph")

    def inline(text: str) -> str:
        return render_inline(md, text, source, home.path, lang)

    # Quick start: numbered steps, some with a screenshot and caption.
    quick = require(sections, "Quick start", lang)
    items: list[list[str]] = []
    after: list[str] = []
    for line in quick.split("\n"):
        if re.match(r"^\d+\.\s", line):
            items.append([re.sub(r"^\d+\.\s+", "", line)])
        elif after or (line and not line[0].isspace()):
            after.append(line)
        elif items:
            items[-1].append(line)
    if len(items) != 5:
        raise BuildError(f"{source}: 'Quick start' should list 5 steps, found {len(items)}")
    steps = []
    for item in items:
        text = "\n".join(item)
        figure = FIGURE.search(text)
        body = " ".join(FIGURE.sub("", text).split())
        match = re.fullmatch(lang.step, body)
        if not match:
            raise BuildError(f"{source}: quick start step should start with the step's name in bold and a colon: {body[:60]}")
        name = match.group("name") + (match.groupdict().get("orig") or "")
        fig = ""
        if figure:
            fig = "\n" + figure_html(md, figure.group(1), figure.group(2), figure.group(3), home.path, lang=lang)
        steps.append(
            f"      <li>\n<h3>{inline(name)}</h3>\n"
            f"<p>{inline(sentence_case(match.group('text')))}</p>{fig}\n      </li>"
        )
    first_step = " ".join(items[0][0].split())
    formats = re.search(lang.formats, first_step)
    if not formats:
        raise BuildError(f"{source}: quick start step 1 should name the model formats in parentheses")
    after_html = render(md, "\n".join(after), source, home.path, lang)[0] if "".join(after).strip() else ""

    needs = require(sections, "What you need", lang)
    blender = re.search(r"\|\s*\[Blender\]\([^)]*\)\s*([^|]+?)\s*\|", needs)
    if not blender:
        raise BuildError(f"{source}: the 'What you need' table should have a Blender row")

    def md_block(name: str) -> str:
        return render(md, require(sections, name, lang), source, home.path, lang)[0]

    repo = config["repoUrl"]
    issues = f"{repo}/issues/new/choose"
    english_pages = {page.key: rel_url(home.path, page.path) for page in DOC_PAGES.values()}
    context = {
        "repo": repo,
        "issues": issues,
        "license": f"{repo}/blob/{config['branch']}/LICENSE",
        "repo_name": config["repoName"],
        "formats": sentence_case(formats.group(1)),
        "blender": blender.group(1),
        "privacy": md_block("Privacy"),
        "note": lang.foreign_note,
        "tutorial": rel_url(home.path, tutorial_of(lang).path),
        "guide": english_pages["guide"],
        "troubleshooting": english_pages["troubleshooting"],
        "changelog": english_pages["changelog"],
    }
    faq_html = "\n".join(
        f'    <div class="faq-item">\n<h3>{esc(q)}</h3>\n{a.strip()}\n    </div>' for q, a in lang.faq(context)
    )

    foreign = {context["guide"], context["troubleshooting"], context["changelog"]} if lang is not ENGLISH else set()
    english_link = ' hreflang="en"'
    links_html = "\n".join(
        f'      <li><a href="{esc(href)}"{english_link if href in foreign else ""}>'
        f'<span class="link-title">{esc(title)}</span>'
        f'<span class="link-text">{esc(text)}</span></a></li>'
        for href, title, text in lang.doc_links(context)
    )

    t = lang.text
    hero_figure = figure_html(md, *hero_match.groups(), home.path, lazy=False, lang=lang)
    return Template((TEMPLATES / "home.html").read_text(encoding="utf-8")).substitute(
        pitch=inline(pitch + lang.sentence_end.strip()),
        lead=inline(lead),
        download_url=esc(f"{repo}/releases/latest"),
        repo_url=esc(repo),
        download_label=t["download_windows"],
        github_label=t["view_github"],
        release_meta=t["release_meta"].format(version=esc(version)),
        hero_figure=hero_figure,
        steps_title=t["steps_title"],
        steps="\n".join(steps),
        after_steps=f'<div class="prose">\n{after_html}</div>' if after_html else "",
        tutorial_url=esc(rel_url(home.path, tutorial_of(lang).path)),
        tutorial_label=t["tutorial_button"],
        features_title=t["features_title"],
        features=md_block("Features"),
        requirements_title=t["requirements_title"],
        requirements=md_block("What you need"),
        install_title=t["install_title"],
        install=md_block("Installation"),
        faq_title=t["faq_title"],
        faq=faq_html,
        docs_title=t["docs_title"],
        doc_links=links_html,
    )


# ---------------------------------------------------------------- pages

def build_doc(md, source: str, page: Page) -> tuple[str, str]:
    lang = language_of(page)
    body, tokens = render(md, read_text(source), source, page.path, lang)
    h1 = [tokens[i + 1] for i, t in enumerate(tokens) if t.type == "heading_open" and t.tag == "h1"]
    if len(h1) != 1:
        raise BuildError(f"{source}: expected exactly one '# ' heading, found {len(h1)}")
    if page.toc:
        entries: list[tuple[str, str, list]] = []
        for i, token in enumerate(tokens):
            if token.type == "heading_open" and token.tag in ("h2", "h3"):
                item = (token.attrGet("id"), heading_text(tokens[i + 1]), [])
                if not item[0]:
                    raise BuildError(f"{source}: the heading '{item[1]}' has no id")
                if token.tag == "h2" or not entries:
                    entries.append(item)
                else:
                    entries[-1][2].append(item)

        def toc_list(items, indent):
            pad = "  " * indent
            out = [f"{pad}<ol>"]
            for slug, text, children in items:
                sub = "\n" + toc_list(children, indent + 2) + f"\n{pad}  " if children else ""
                out.append(f'{pad}  <li><a href="#{esc(slug)}">{esc(text)}</a>{sub}</li>')
            out.append(f"{pad}</ol>")
            return "\n".join(out)

        toc = (
            f'<nav class="toc" aria-labelledby="toc-title">\n<p class="toc-title" id="toc-title">{esc(lang.text["toc"])}</p>\n'
            + toc_list(entries, 0) + "\n</nav>\n"
        )
        at = body.find("<h2")
        body = body[:at] + toc + body[at:] if at >= 0 else toc + body
    main = f'<div class="wrap">\n<article class="prose doc">\n{body}</article>\n</div>'
    return main, heading_text(h1[0])


GLOBE = (
    '<svg class="lang-globe" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" '
    'stroke-width="1.6" stroke-linecap="round" aria-hidden="true" focusable="false">'
    '<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c2.5 2.6 3.8 5.6 3.8 9s-1.3 6.4-3.8 9'
    'c-2.5-2.6-3.8-5.6-3.8-9S9.5 5.6 12 3z"/></svg>'
)


def language_menu(config, page: Page, lang: Language, absolute: bool) -> str:
    """The header's language button: a <details> menu, so it works without
    JavaScript. Each language goes to this page in that language, or to that
    language's home page when the page does not exist in it."""
    same = versions(page)
    items = []
    for other in LANGUAGES:
        target = same.get(other.code, other.home)
        href = config["basePath"] + target.path if absolute else rel_url(page.path, target.path)
        current = ' aria-current="true"' if other is lang else ""
        items.append(
            f'        <li><a href="{esc(href)}" lang="{other.code}" hreflang="{other.code}"{current}>{esc(other.name)}</a></li>'
        )
    t = lang.text
    return (
        '    <details class="lang-menu">\n'
        f'      <summary>{GLOBE}<span class="visually-hidden">{esc(t["language_label"])}</span>'
        f'<span class="lang-current">{esc(lang.name)}</span></summary>\n'
        f'      <ul class="lang-list" aria-label="{esc(t["language"])}">\n'
        + "\n".join(items) + "\n"
        "      </ul>\n"
        "    </details>\n"
    )


def page_html(config, page: Page, main: str, *, json_ld: dict | None = None, extra_head: str = "") -> str:
    lang = language_of(page)
    t = lang.text
    absolute = page is NOT_FOUND
    root = config["basePath"] if absolute else rel_url(page.path, "")
    if root == "./":
        root = ""
    # Links to this language's own pages start from its home page.
    lang_root = config["basePath"] + lang.prefix if absolute else rel_url(page.path, lang.home.path)
    if lang_root == "./":
        lang_root = ""
    home_href = config["basePath"] + lang.prefix if absolute else rel_url(page.path, lang.home.path)
    url = config["baseUrl"] + page.path
    og_image = config["baseUrl"] + "og-card.png"
    repo = config["repoUrl"]

    tutorial = tutorial_of(lang)
    foreign = "" if lang is ENGLISH else ' hreflang="en"'
    tutorial_foreign = "" if tutorial.lang == lang.code else ' hreflang="en"'
    nav = [
        ("tutorial", t["nav_tutorial"], f"{lang_root}{tutorial.path[len(lang.prefix):]}"
         if tutorial.lang == lang.code else f"{root}{tutorial.path}", tutorial_foreign),
        ("guide", t["nav_guide"], f"{root}guide/", foreign),
        ("troubleshooting", t["nav_troubleshooting"], f"{root}troubleshooting/", foreign),
        ("changelog", t["nav_changelog"], f"{root}changelog/", foreign),
        ("github", t["nav_github"], repo, ""),
        ("download", t["nav_download"], f"{repo}/releases/latest", ' class="nav-download"'),
    ]
    current = ' aria-current="page"'
    nav_items = "\n".join(
        f'        <li><a href="{esc(href)}"{cls}{current if key == page.nav else ""}>{label}</a></li>'
        for key, label, href, cls in nav
    )

    if absolute:
        social = ""
        extra_head = '<meta name="robots" content="noindex">\n' + extra_head
    else:
        alt = t["og_image_alt"]
        same = versions(page)
        alternates = []
        if len(same) > 1:
            alternates = [
                f'<link rel="alternate" hreflang="{code}" href="{esc(config["baseUrl"] + other.path)}">'
                for code, other in same.items()
            ]
            default = same.get(ENGLISH.code)
            if default:
                alternates.append(
                    f'<link rel="alternate" hreflang="x-default" href="{esc(config["baseUrl"] + default.path)}">'
                )
        locales = [f'<meta property="og:locale" content="{lang.og_locale}">']
        locales += [
            f'<meta property="og:locale:alternate" content="{LANGUAGE_BY_CODE[code].og_locale}">'
            for code in same if code != lang.code
        ]
        social = "\n".join([
            f'<link rel="canonical" href="{esc(url)}">',
            *alternates,
            '<meta property="og:type" content="website">',
            f'<meta property="og:site_name" content="{esc(config["name"])}">',
            *locales,
            f'<meta property="og:title" content="{esc(page.title)}">',
            f'<meta property="og:description" content="{esc(page.description)}">',
            f'<meta property="og:url" content="{esc(url)}">',
            f'<meta property="og:image" content="{esc(og_image)}">',
            '<meta property="og:image:width" content="1200">',
            '<meta property="og:image:height" content="630">',
            f'<meta property="og:image:alt" content="{esc(alt)}">',
            '<meta name="twitter:card" content="summary_large_image">',
            f'<meta name="twitter:title" content="{esc(page.title)}">',
            f'<meta name="twitter:description" content="{esc(page.description)}">',
            f'<meta name="twitter:image" content="{esc(og_image)}">',
            f'<meta name="twitter:image:alt" content="{esc(alt)}">',
        ]) + "\n"
    ld = ""
    if json_ld:
        data = json.dumps(json_ld, indent=2, ensure_ascii=False).replace("</", "<\\/")
        ld = f'<script type="application/ld+json">\n{data}\n</script>\n'

    return Template((TEMPLATES / "base.html").read_text(encoding="utf-8")).substitute(
        lang=lang.code,
        title=esc(page.title),
        description=esc(page.description),
        head_extra=extra_head,
        root=root,
        home_href=esc(home_href),
        social=social,
        json_ld=ld,
        skip_label=t["skip"],
        site_name=esc(config["name"]),
        nav_label=esc(t["nav_label"]),
        nav_items=nav_items,
        language_menu=language_menu(config, page, lang, absolute),
        main=main,
        footer=t["footer"].format(
            license=esc(f"{repo}/blob/{config['branch']}/LICENSE"), repo=esc(repo),
        ),
    )


# ---------------------------------------------------------------- checks

class PageScan(HTMLParser):
    RESOURCE_TAGS = {"img", "script", "link", "source", "iframe", "embed", "object", "video", "audio", "track", "input"}

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.ids: set[str] = set()
        self.refs: list[tuple[str, str, str]] = []  # (tag, attribute, url)
        self.h1 = 0
        self.title = ""
        self._in_title = False
        self.meta: dict[str, str] = {}
        self.canonical = ""
        self.ld_json: list[str] = []
        self._in_ld = False
        self.lang = ""
        self.alternates: dict[str, str] = {}  # hreflang -> URL

    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        if tag == "html":
            self.lang = a.get("lang", "")
        if a.get("id"):
            self.ids.add(a["id"])
        if tag == "a" and a.get("name"):
            self.ids.add(a["name"])
        if tag == "h1":
            self.h1 += 1
        if tag == "title":
            self._in_title = True
        if tag == "script" and a.get("type") == "application/ld+json":
            self._in_ld = True
            self.ld_json.append("")
        if tag == "meta":
            key = a.get("name") or a.get("property")
            if key:
                self.meta[key] = a.get("content", "")
        if tag == "link" and a.get("rel") == "canonical":
            self.canonical = a.get("href", "")
        if tag == "link" and a.get("rel") == "alternate" and a.get("hreflang"):
            self.alternates[a["hreflang"]] = a.get("href", "")
        for attr in ("href", "src", "poster", "data"):
            if a.get(attr) is not None:
                self.refs.append((tag, attr, a[attr]))
        if a.get("srcset"):
            for part in a["srcset"].split(","):
                self.refs.append((tag, "srcset", part.strip().split(" ")[0]))

    def handle_endtag(self, tag):
        if tag == "title":
            self._in_title = False
        if tag == "script":
            self._in_ld = False

    def handle_data(self, data):
        if self._in_title:
            self.title += data
        if self._in_ld:
            self.ld_json[-1] += data


def check_languages(config, scans: dict[str, PageScan], problems: list[str]) -> None:
    """<html lang> by URL prefix, and hreflang links that point at each other."""
    base_url = config["baseUrl"]
    by_prefix = sorted(LANGUAGES, key=lambda lang: len(lang.prefix), reverse=True)

    def file_of(url: str) -> str:
        path = url[len(base_url):] if url.startswith(base_url) else None
        if path is None:
            return ""
        return path + "index.html" if path == "" or path.endswith("/") else path

    for rel, scan in scans.items():
        if rel.startswith("google") and scan.h1 == 0:
            continue
        lang = next(lang for lang in by_prefix if rel.startswith(lang.prefix))
        if scan.lang != lang.code:
            problems.append(f"{rel}: <html lang> is '{scan.lang}', expected '{lang.code}'")
        if not scan.alternates:
            continue
        if scan.alternates.get(lang.code) != scan.canonical:
            problems.append(f"{rel}: its hreflang '{lang.code}' link is not its canonical URL")
        if "x-default" not in scan.alternates:
            problems.append(f"{rel}: hreflang links without x-default")
        for code, url in scan.alternates.items():
            other = scans.get(file_of(url))
            if other is None:
                problems.append(f"{rel}: hreflang '{code}' points to {url}, which is not a page of the site")
            elif other.alternates != scan.alternates:
                problems.append(f"{rel}: hreflang links differ from those of {file_of(url)}")
            elif code != "x-default" and other.lang != code:
                problems.append(f"{rel}: hreflang '{code}' points to a page in '{other.lang}'")


def check_site(config) -> int:
    base_url, base_path = config["baseUrl"], config["basePath"]
    scans: dict[str, PageScan] = {}
    for path in sorted(DIST.rglob("*.html")):
        rel = path.relative_to(DIST).as_posix()
        scan = PageScan()
        scan.feed(path.read_text(encoding="utf-8"))
        scans[rel] = scan

    problems: list[str] = []
    titles: dict[str, str] = {}
    checked = 0
    for rel, scan in scans.items():
        if rel.startswith("google") and scan.h1 == 0:
            continue  # a Search Console verification file from site/static
        page_dir = posixpath.dirname(rel)
        if scan.h1 != 1:
            problems.append(f"{rel}: has {scan.h1} <h1> elements, expected 1")
        if not scan.title.strip():
            problems.append(f"{rel}: has no <title>")
        elif scan.title in titles:
            problems.append(f"{rel}: <title> is the same as in {titles[scan.title]}")
        titles[scan.title] = rel
        if not scan.meta.get("description"):
            problems.append(f"{rel}: has no meta description")
        if rel != "404.html":
            expected = base_url + (rel[: -len("index.html")] if rel.endswith("index.html") else rel)
            if scan.canonical != expected:
                problems.append(f"{rel}: canonical is '{scan.canonical}', expected '{expected}'")
        for block in scan.ld_json:
            try:
                json.loads(block)
            except ValueError as error:
                problems.append(f"{rel}: JSON-LD does not parse: {error}")
        for key in ("og:image", "og:url", "twitter:image"):
            if key in scan.meta:
                scan.refs.append(("meta", key, scan.meta[key]))

        for tag, attr, url in scan.refs:
            parts = urlsplit(url)
            if parts.scheme in ("mailto", "tel"):
                continue
            if url.startswith(base_url):
                path = "/" + url[len(base_url):]
                parts = urlsplit(path)
                target = parts.path.lstrip("/")
            elif parts.scheme or url.startswith("//"):
                if tag != "a":
                    problems.append(f"{rel}: <{tag} {attr}> loads an off-site resource: {url}")
                continue
            elif parts.path.startswith("/"):
                if not parts.path.startswith(base_path):
                    problems.append(f"{rel}: '{url}' is an absolute path outside {base_path}")
                    continue
                target = parts.path[len(base_path):]
            else:
                target = posixpath.join(page_dir, unquote(parts.path)) if parts.path else rel
                if parts.path.endswith("/") or parts.path in (".", ".."):
                    target += "/"
            checked += 1
            norm = posixpath.normpath(target) if target.strip("/") else ""
            if norm == ".":
                norm = ""
            if norm.startswith(".."):
                problems.append(f"{rel}: '{url}' points outside the site")
                continue
            if target.endswith("/") or norm == "":
                norm = posixpath.join(norm, "index.html") if norm else "index.html"
            file = DIST / norm
            if file.is_dir():
                problems.append(f"{rel}: '{url}' points to a folder without a trailing slash")
                continue
            if not file.is_file():
                problems.append(f"{rel}: '{url}' does not resolve to a file in dist/ ({norm})")
                continue
            if parts.fragment:
                ids = scans.get(norm).ids if norm in scans else set()
                if unquote(parts.fragment) not in ids:
                    problems.append(f"{rel}: '{url}' points to a missing anchor in {norm}")
    check_languages(config, scans, problems)
    if problems:
        raise BuildError("site check failed:\n  " + "\n  ".join(problems))
    return checked


# ---------------------------------------------------------------- main

def home_json_ld(config, lang: Language, version: str) -> dict:
    repo = config["repoUrl"]
    return {
        "@context": "https://schema.org",
        "@type": "SoftwareApplication",
        "name": config["name"],
        "description": lang.home.description,
        "url": config["baseUrl"] + lang.home.path,
        "inLanguage": lang.code,
        "image": config["baseUrl"] + "og-card.png",
        "screenshot": config["baseUrl"] + "images/character-and-model.png",
        "operatingSystem": "Windows 11",
        "applicationCategory": "DeveloperApplication",
        "softwareVersion": version,
        "author": {
            "@type": "Person",
            "name": config["repoOwner"],
            "url": repo.rsplit("/", 1)[0],
        },
        "sameAs": [repo],
        "releaseNotes": config["baseUrl"] + "changelog/",
        "downloadUrl": f"{repo}/releases/latest",
        "license": "https://www.gnu.org/licenses/agpl-3.0.html",
        "isAccessibleForFree": True,
        "offers": {"@type": "Offer", "price": "0", "priceCurrency": "USD"},
    }


def build() -> None:
    config = load_config()
    package = json.loads(read_text("package.json"))
    version = package.get("version")
    if not isinstance(version, str) or not version:
        raise BuildError("package.json has no version")

    if DIST.exists():
        shutil.rmtree(DIST)
    DIST.mkdir(parents=True)

    # Static files, icons and screenshots.
    for src in sorted(p for p in STATIC.rglob("*") if p.is_file()):
        dest = DIST / src.relative_to(STATIC)
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(src, dest)
    copies = {
        "src-tauri/icons/icon.svg": "assets/icon.svg",
        "src-tauri/icons/icon.png": "assets/icon.png",
        "src-tauri/icons/icon.ico": "favicon.ico",
    }
    for image in sorted((ROOT / IMAGES_SRC).glob("*.png")):
        copies[f"{IMAGES_SRC}/{image.name}"] = f"images/{image.name}"
    for src, dest in copies.items():
        if not (ROOT / src).is_file():
            raise BuildError(f"missing source file: {src}")
        if (DIST / dest).exists():
            raise BuildError(f"site/static/{dest} clashes with a generated file")
        (DIST / dest).parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT / src, DIST / dest)
    if not (DIST / "og-card.png").is_file():
        raise BuildError("site/static/og-card.png is missing (see site/make_og_card.py)")

    md = make_markdown(config)
    readmes = {lang.readme: read_text(lang.readme) for lang in LANGUAGES}
    check_readmes(readmes)

    verification = config["googleSiteVerification"].strip()
    extra = f'<meta name="google-site-verification" content="{esc(verification)}">\n' if verification else ""
    for lang in LANGUAGES:
        # Home
        home_main = build_home(md, config, readmes[lang.readme], version, lang)
        write_text(
            lang.home.path + "index.html",
            page_html(config, lang.home, home_main, json_ld=home_json_ld(config, lang, version), extra_head=extra),
        )
        # Documentation pages
        for source, page in lang.docs.items():
            main, _ = build_doc(md, source, page)
            write_text(page.path + "index.html", page_html(config, page, main))

    # 404
    base = config["basePath"]
    not_found = (
        '<div class="wrap notfound">\n<h1>Page not found</h1>\n'
        "<p>There is no page at this address. It may have moved.</p>\n"
        f'<p><a class="button primary" href="{esc(base)}">Go to the home page</a></p>\n</div>'
    )
    write_text("404.html", page_html(config, NOT_FOUND, not_found))

    # Sitemap
    # The home page's title, questions and layout live in this script, site/languages.py
    # and the template.
    entries = []
    for lang in LANGUAGES:
        entries.append((lang.home.path, git_date(
            lang.readme, "package.json", "site/build.py", "site/templates/home.html", "site/languages.py",
        )))
        entries += [(page.path, git_date(source)) for source, page in lang.docs.items()]
    urls = "\n".join(
        f"  <url>\n    <loc>{esc(config['baseUrl'] + path)}</loc>\n    <lastmod>{date}</lastmod>\n  </url>"
        for path, date in entries
    )
    write_text(
        "sitemap.xml",
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n' + urls + "\n</urlset>\n",
    )

    for lang in LANGUAGES:
        low, high = lang.description_range
        for page in [lang.home, *lang.docs.values()]:
            if display_width(page.title) > lang.title_width:
                raise BuildError(
                    f"title wider than {lang.title_width} characters ({display_width(page.title)}): {page.title}"
                )
            width = display_width(page.description)
            if not low <= width <= high:
                unit = "characters" if lang is ENGLISH else "wide (a Chinese character counts 2)"
                raise BuildError(
                    f"{lang.code} description should be {low}-{high} {unit} ({width}): {page.description}"
                )

    checked = check_site(config)
    pages = len(list(DIST.rglob("*.html")))
    print(
        f"Built site/dist: {pages} HTML pages in {len(LANGUAGES)} languages, version {version}; "
        f"{checked} internal links and images checked."
    )


def main() -> int:
    try:
        build()
    except BuildError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
