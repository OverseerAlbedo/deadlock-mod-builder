"""Build the project website into site/dist/.

    python -m pip install -r site/requirements.txt
    python site/build.py

The home page is site/templates/home.html filled with sections of README.md;
the guide, troubleshooting and changelog pages are rendered from
docs/MODEL_PREPARATION.md, docs/TROUBLESHOOTING.md and CHANGELOG.md. Relative
links in the Markdown are rewritten to site pages, copied images or GitHub
URLs. After writing, every internal link, image and #anchor in the output is
checked, and the build fails (exit code 1) on any problem.
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
from dataclasses import dataclass
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

SITE = Path(__file__).resolve().parent
ROOT = SITE.parent
DIST = SITE / "dist"
TEMPLATES = SITE / "templates"
STATIC = SITE / "static"
IMAGES_SRC = "docs/images"


class BuildError(Exception):
    pass


@dataclass(frozen=True)
class Page:
    path: str  # URL path below the site root: "" for home, "guide/" ...
    title: str  # <title>
    description: str
    nav: str = ""  # nav key, for aria-current
    label: str = ""  # used as link text when Markdown links show a file name
    toc: bool = False


HOME = Page(
    path="",
    title="Deadlock Mod Builder – make Deadlock model swap mods",
    description=(
        "Free, open-source Windows app for Deadlock model swaps: it turns a Blender, FBX, glTF or MMD "
        "character into a hero replacement mod and installs it as a VPK."
    ),
)

# Markdown sources that become site pages, keyed by repository path.
DOC_PAGES = {
    "docs/FIRST_MOD.md": Page(
        path="make-a-deadlock-character-mod/",
        title="How to make a Deadlock character mod (model swap tutorial)",
        description=(
            "Step-by-step Deadlock model swap tutorial: replace a hero's model with your own Blender, "
            "FBX, glTF or MMD character and install the mod as a VPK."
        ),
        nav="tutorial",
        label="the step-by-step tutorial",
        toc=True,
    ),
    "docs/MODEL_PREPARATION.md": Page(
        path="guide/",
        title="Model preparation guide – Deadlock Mod Builder",
        description=(
            "How Deadlock Mod Builder turns a character model into a Deadlock hero mod: the five steps, "
            "joint matches, textures, moving parts, your own gun and the Extras."
        ),
        nav="guide",
        label="the model preparation guide",
        toc=True,
    ),
    "docs/TROUBLESHOOTING.md": Page(
        path="troubleshooting/",
        title="Troubleshooting – Deadlock Mod Builder",
        description=(
            "Fixes for common Deadlock Mod Builder problems: tool setup, Blender versions, joint matches, "
            "guns, physics, disk space, logs and the October 2026 camera fix."
        ),
        nav="troubleshooting",
        label="the troubleshooting page",
        toc=True,
    ),
    "CHANGELOG.md": Page(
        path="changelog/",
        title="Changelog – Deadlock Mod Builder",
        description=(
            "Release history of Deadlock Mod Builder, the Windows app that builds Deadlock hero model "
            "replacement mods: what is new and fixed in each version."
        ),
        nav="changelog",
        label="the changelog",
    ),
}

TUTORIAL = DOC_PAGES["docs/FIRST_MOD.md"]

NOT_FOUND = Page(
    path="404.html",
    title="Page not found – Deadlock Mod Builder",
    description="This page does not exist on the Deadlock Mod Builder site.",
)


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

    def rewrite(url: str, env: dict) -> tuple[str, Page | None]:
        parts = urlsplit(url)
        if parts.scheme or parts.netloc or url.startswith("#"):
            return url, None
        source = env["source"]
        target = posixpath.normpath(posixpath.join(posixpath.dirname(source), unquote(parts.path)))
        if target == ".." or target.startswith("../"):
            raise BuildError(f"{source}: link '{url}' leaves the repository")
        fragment = f"#{parts.fragment}" if parts.fragment else ""
        if target in DOC_PAGES:
            page = DOC_PAGES[target]
            if page.path == env["page"] and fragment:
                return fragment, page
            return rel_url(env["page"], page.path) + fragment, page
        if target == "README.md":
            return rel_url(env["page"], HOME.path) + fragment, None
        if not (ROOT / target).exists():
            raise BuildError(f"{source}: link '{url}' points to {target}, which is not in the repository")
        if target.startswith(IMAGES_SRC + "/"):
            return rel_url(env["page"], "images/" + posixpath.basename(target)), None
        kind = "tree" if (ROOT / target).is_dir() else "blob"
        return f"{config['repoUrl']}/{kind}/{config['branch']}/{quote(target)}{fragment}", None

    def site_links(state) -> None:
        env = state.env
        stack = list(state.tokens)
        while stack:
            token = stack.pop()
            if token.children:
                children = token.children
                for i, child in enumerate(children):
                    if child.type == "link_open":
                        original = child.attrGet("href") or ""
                        new, page = rewrite(original, env)
                        child.attrSet("href", new)
                        # A link that shows a file name ("docs/X.md") reads better as the page name.
                        if page and i + 2 < len(children) and children[i + 2].type == "link_close":
                            text = children[i + 1]
                            if text.type == "text" and text.content in (original, posixpath.basename(original)):
                                text.content = page.label
                    elif child.type == "image":
                        src = child.attrGet("src") or ""
                        new, _ = rewrite(src, env)
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
        return '<div class="table-wrap" tabindex="0" role="region" aria-label="Table">\n<table>\n'

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


def render(md: MarkdownIt, text: str, source: str, page: str) -> tuple[str, list]:
    env = {"source": source, "page": page}
    tokens = md.parse(text, env)
    return md.renderer.render(tokens, md.options, env), tokens


def render_inline(md: MarkdownIt, text: str, source: str, page: str) -> str:
    env = {"source": source, "page": page}
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


def require(sections: dict[str, str], name: str) -> str:
    if name not in sections:
        raise BuildError(f"README.md has no '## {name}' section; the home page needs it")
    return sections[name]


def sentence_case(text: str) -> str:
    return text[:1].upper() + text[1:]


FIGURE =re.compile(r"^[ \t]*!\[([^\]]*)\]\(([^)\s]+)\)[ \t]*\n[ \t]*\*(.+?)\*[ \t]*$", re.M)


def figure_html(md, alt, src, caption, page, lazy=True) -> str:
    target = posixpath.normpath(unquote(src))
    if not target.startswith(IMAGES_SRC + "/") or not (ROOT / target).is_file():
        raise BuildError(f"README.md: screenshot '{src}' is not a file under {IMAGES_SRC}/")
    width, height = png_size(ROOT / target)
    url = rel_url(page, "images/" + posixpath.basename(target))
    loading = ' loading="lazy" decoding="async"' if lazy else ' fetchpriority="high"'
    return (
        f'<figure class="shot"><img src="{esc(url)}" alt="{esc(alt)}" width="{width}" height="{height}"{loading}>'
        f"<figcaption>{render_inline(md, caption, 'README.md', page)}</figcaption></figure>"
    )


def build_home(md, config, readme: str, version: str) -> str:
    intro, sections = readme_sections(readme)
    paragraphs = [p.strip() for p in intro.split("\n\n") if p.strip() and not p.strip().startswith("[![")]
    if len(paragraphs) < 2:
        raise BuildError("README.md: expected an opening paragraph and a screenshot before the first '##' section")
    opening = " ".join(paragraphs[0].split())
    pitch, _, lead = opening.partition(". ")
    # The h1 is the app name; avoid repeating it at the start of the pitch.
    prefix = f"{config['name']} is "
    if pitch.startswith(prefix):
        pitch = sentence_case(pitch[len(prefix):])
    if not lead:
        raise BuildError("README.md: the opening paragraph should have more than one sentence")
    hero_match = FIGURE.search(paragraphs[1])
    if not hero_match:
        raise BuildError("README.md: expected a screenshot with an italic caption after the opening paragraph")

    # Quick start: numbered steps, some with a screenshot and caption.
    quick = require(sections, "Quick start")
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
        raise BuildError(f"README.md: 'Quick start' should list 5 steps, found {len(items)}")
    steps = []
    for item in items:
        text = "\n".join(item)
        figure = FIGURE.search(text)
        body = " ".join(FIGURE.sub("", text).split())
        match = re.fullmatch(r"\*\*(.+?):\*\*\s*(.+)", body)
        if not match:
            raise BuildError(f"README.md: quick start step should start with '**Name:**': {body[:60]}")
        fig = ""
        if figure:
            fig = "\n" + figure_html(md, figure.group(1), figure.group(2), figure.group(3), HOME.path)
        steps.append(
            f"      <li>\n<h3>{render_inline(md, match.group(1), 'README.md', HOME.path)}</h3>\n"
            f"<p>{render_inline(md, sentence_case(match.group(2)), 'README.md', HOME.path)}</p>{fig}\n      </li>"
        )
    first_step = " ".join(items[0][0].split())
    formats = re.search(r"\(([^()]*DMX[^()]*)\)", first_step)
    if not formats:
        raise BuildError("README.md: quick start step 1 should name the model formats in parentheses")
    after_html = render(md, "\n".join(after), "README.md", HOME.path)[0] if "".join(after).strip() else ""

    needs = require(sections, "What you need")
    blender = re.search(r"\|\s*\[Blender\]\([^)]*\)\s*([^|]+?)\s*\|", needs)
    if not blender:
        raise BuildError("README.md: the 'What you need' table should have a Blender row")

    def md_block(name: str) -> str:
        return render(md, require(sections, name), "README.md", HOME.path)[0]

    repo = config["repoUrl"]
    issues = f"{repo}/issues/new/choose"
    faq = [
        ("What does Deadlock Mod Builder do?",
         "<p>It makes a model swap for Deadlock: your own character replaces a hero's model in the game, "
         "with its guns, materials and moving hair and cloth. People also call this a hero replacement mod "
         "or a custom skin. The result is a VPK file that the app installs in the game's "
         "<code>addons</code> folder.</p>"),
        ("Is it a mod manager?",
         "<p>No. A mod manager downloads and installs mods that other people made. Deadlock Mod Builder "
         "makes a mod from your own model file and installs the mods you build with it.</p>"),
        ("Can it change more than the character model?",
         "<p>Yes. The Extras replace hero icons, ability images and sounds, recolor ability and weapon "
         "effects, put your own models on ability objects and give the shop's items new icons and names. "
         "Motion replaces a hero's animation clips.</p>"),
        ("Is Deadlock Mod Builder free?",
         f'<p>Yes. It is open source under the <a href="{repo}/blob/{config["branch"]}/LICENSE">GNU Affero General Public License v3.0 or later</a>, '
         f'and the source code is on <a href="{repo}">GitHub</a>.</p>'),
        ("Where is the official download?",
         f'<p>Installers are published only on the <a href="{repo}/releases/latest">Releases page</a> of '
         f'the GitHub repository <a href="{repo}">{esc(config["repoName"])}</a>. '
         "Other Deadlock tools with similar names are separate projects.</p>"),
        ("Which model files can I use?",
         f"<p>{esc(sentence_case(formats.group(1)))}. You can also add a gun from a separate file.</p>"),
        ("Which Blender versions work?",
         f"<p>Blender {esc(blender.group(1))}.</p>"),
        ("Do I need to install Python?",
         "<p>No. Blender runs in the background to prepare the model; no separate Python installation is needed.</p>"),
        ("Does it include game files or character models?",
         "<p>No. The app does not include Valve tools, game files or third-party character models. "
         "You install Deadlock and the tools listed above yourself, and bring your own model.</p>"),
        ("Does the app send any data?", md_block("Privacy")),
        ("Is this an official Valve tool?",
         "<p>No. This is an unofficial tool, not affiliated with or endorsed by Valve. "
         "Deadlock and its assets belong to their respective owners.</p>"),
        ("Where do I report a problem?",
         f'<p>Use the <a href="{issues}">issue templates</a> on GitHub. Include the app version '
         "(Settings → About this build), the model's file format, the hero, the steps, and the text from "
         "<strong>Copy for support</strong>. The <a href=\"troubleshooting/\">troubleshooting page</a> "
         "covers common problems.</p>"),
    ]
    faq_html = "\n".join(
        f'    <div class="faq-item">\n<h3>{esc(q)}</h3>\n{a.strip()}\n    </div>' for q, a in faq
    )

    links = [
        (TUTORIAL.path, "Step-by-step tutorial",
         "How to make your first Deadlock character mod, from installing the tools to checking it in game."),
        ("guide/", "Model preparation guide",
         "The five steps, joint matches, textures, moving parts, the movement preview, gun customization "
         "and the Extras (sounds, effects, items, ability objects, colors)."),
        ("troubleshooting/", "Troubleshooting", "Compatibility limits and fixes for common failures."),
        ("changelog/", "Changelog", "What changed in each release."),
        (issues, "Report a problem", "Bug reports and feature requests on GitHub."),
        (f"{repo}/releases", "All releases", "Installers and checksums for every version."),
        (repo, "Source code", "The app's source, licensed under the GNU AGPL v3.0 or later."),
    ]
    links_html = "\n".join(
        f'      <li><a href="{esc(href)}"><span class="link-title">{esc(title)}</span>'
        f'<span class="link-text">{esc(text)}</span></a></li>'
        for href, title, text in links
    )

    hero_figure = figure_html(md, *hero_match.groups(), HOME.path, lazy=False)
    return Template((TEMPLATES / "home.html").read_text(encoding="utf-8")).substitute(
        pitch=render_inline(md, pitch + ".", "README.md", HOME.path),
        lead=render_inline(md, lead, "README.md", HOME.path),
        download_url=esc(f"{repo}/releases/latest"),
        repo_url=esc(repo),
        version=esc(version),
        hero_figure=hero_figure,
        steps="\n".join(steps),
        after_steps=f'<div class="prose">\n{after_html}</div>' if after_html else "",
        tutorial_url=esc(rel_url(HOME.path, TUTORIAL.path)),
        features=md_block("Features"),
        requirements=md_block("What you need"),
        install=md_block("Installation"),
        faq=faq_html,
        doc_links=links_html,
    )


# ---------------------------------------------------------------- pages

def build_doc(md, source: str, page: Page) -> tuple[str, str]:
    body, tokens = render(md, read_text(source), source, page.path)
    h1 = [tokens[i + 1] for i, t in enumerate(tokens) if t.type == "heading_open" and t.tag == "h1"]
    if len(h1) != 1:
        raise BuildError(f"{source}: expected exactly one '# ' heading, found {len(h1)}")
    if page.toc:
        entries: list[tuple[str, str, list]] = []
        for i, token in enumerate(tokens):
            if token.type == "heading_open" and token.tag in ("h2", "h3"):
                item = (token.attrGet("id"), heading_text(tokens[i + 1]), [])
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
            '<nav class="toc" aria-labelledby="toc-title">\n<p class="toc-title" id="toc-title">On this page</p>\n'
            + toc_list(entries, 0) + "\n</nav>\n"
        )
        at = body.find("<h2")
        body = body[:at] + toc + body[at:] if at >= 0 else toc + body
    main = f'<div class="wrap">\n<article class="prose doc">\n{body}</article>\n</div>'
    return main, heading_text(h1[0])


def page_html(config, page: Page, main: str, *, json_ld: dict | None = None, extra_head: str = "") -> str:
    absolute = page is NOT_FOUND
    root = config["basePath"] if absolute else rel_url(page.path, "")
    if root == "./":
        root = ""
    home_href = config["basePath"] if absolute else rel_url(page.path, "")
    url = config["baseUrl"] + page.path
    og_image = config["baseUrl"] + "og-card.png"
    repo = config["repoUrl"]

    nav = [
        ("tutorial", "Tutorial", f"{root}{TUTORIAL.path}", ""),
        ("guide", "Guide", f"{root}guide/", ""),
        ("troubleshooting", "Troubleshooting", f"{root}troubleshooting/", ""),
        ("changelog", "Changelog", f"{root}changelog/", ""),
        ("github", "GitHub", repo, ""),
        ("download", "Download", f"{repo}/releases/latest", ' class="nav-download"'),
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
        alt = "Deadlock Mod Builder: the app's Character & model step next to its name"
        social = "\n".join([
            f'<link rel="canonical" href="{esc(url)}">',
            '<meta property="og:type" content="website">',
            f'<meta property="og:site_name" content="{esc(config["name"])}">',
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
        title=esc(page.title),
        description=esc(page.description),
        head_extra=extra_head,
        root=root,
        home_href=esc(home_href),
        social=social,
        json_ld=ld,
        site_name=esc(config["name"]),
        nav_items=nav_items,
        main=main,
        repo_url=esc(repo),
        license_url=esc(f"{repo}/blob/{config['branch']}/LICENSE"),
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

    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
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
    if problems:
        raise BuildError("site check failed:\n  " + "\n  ".join(problems))
    return checked


# ---------------------------------------------------------------- main

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

    # Home
    home_main = build_home(md, config, read_text("README.md"), version)
    repo = config["repoUrl"]
    json_ld = {
        "@context": "https://schema.org",
        "@type": "SoftwareApplication",
        "name": config["name"],
        "description": HOME.description,
        "url": config["baseUrl"],
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
    verification = config["googleSiteVerification"].strip()
    extra = f'<meta name="google-site-verification" content="{esc(verification)}">\n' if verification else ""
    write_text("index.html", page_html(config, HOME, home_main, json_ld=json_ld, extra_head=extra))

    # Documentation pages
    for source, page in DOC_PAGES.items():
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
    # The home page's title, questions and layout live in this script and its template.
    entries = [(HOME.path, git_date("README.md", "package.json", "site/build.py", "site/templates/home.html"))]
    entries += [(page.path, git_date(source)) for source, page in DOC_PAGES.items()]
    urls = "\n".join(
        f"  <url>\n    <loc>{esc(config['baseUrl'] + path)}</loc>\n    <lastmod>{date}</lastmod>\n  </url>"
        for path, date in entries
    )
    write_text(
        "sitemap.xml",
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n' + urls + "\n</urlset>\n",
    )

    for page in [HOME, *DOC_PAGES.values()]:
        if len(page.title) > 60:
            raise BuildError(f"title longer than 60 characters: {page.title}")
        if not 70 <= len(page.description) <= 160:
            raise BuildError(f"description should be 70-160 characters ({len(page.description)}): {page.description}")

    checked = check_site(config)
    pages = len(list(DIST.rglob("*.html")))
    print(f"Built site/dist: {pages} HTML pages, version {version}; {checked} internal links and images checked.")


def main() -> int:
    try:
        build()
    except BuildError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
