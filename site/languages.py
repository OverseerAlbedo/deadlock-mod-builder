"""The site's languages: pages, sources and every piece of interface text.

site/build.py reads only this table to know which languages exist. English is
the site's main language: its pages live at the site root and are the
x-default of every page that exists in more than one language. Another
language lives below its own URL prefix ("zh-cn/") and needs:

- a Language entry below, added to LANGUAGES: URL prefix, BCP 47 code (used for
  <html lang> and hreflang), og:locale, its README and the README's section
  names, the punctuation its README uses, the title and description bounds,
  all interface text, the home page's questions and documentation links;
- its README (README.<lang>.md at the repository root, same structure as
  README.md) and its tutorial (docs/<lang>/FIRST_MOD.md).

Pages a language does not have stay English; a link to one from that language's
pages must be followed by `foreign_note` (the build checks it).
"""

from __future__ import annotations

import html
import unicodedata
from dataclasses import dataclass, field


def esc(text: str) -> str:
    return html.escape(text, quote=True)


@dataclass(frozen=True)
class Page:
    path: str  # URL path below the site root: "" for home, "guide/" ...
    title: str  # <title>
    description: str
    nav: str = ""  # nav key, for aria-current
    label: str = ""  # used as link text when Markdown links show a file name
    toc: bool = False
    key: str = ""  # the same page in every language has the same key
    lang: str = "en"  # Language.code


@dataclass(frozen=True)
class Language:
    code: str  # BCP 47: <html lang>, hreflang
    prefix: str  # URL path prefix of this language's pages
    name: str  # the language's own name, shown in the language menu
    og_locale: str
    readme: str  # repository path of this language's README
    home: Page
    docs: dict  # repository path of a Markdown source -> Page
    # README section names the home page reads, keyed by the English name.
    # Every section of README.md must be listed here, in README.md's order.
    sections: dict
    pitch_prefix: str  # "<name> is " at the start of the README's first sentence
    sentence_end: str  # what ends the README's first sentence
    step: str  # regex for a quick start step: groups name, orig (optional), text
    formats: str  # regex for the model formats in parentheses in step 1
    # Bounds checked by the build, in display width: a wide (CJK) character
    # counts 2, everything else 1, so for English this is the character count.
    title_width: int
    description_range: tuple  # (shortest, longest) description
    foreign_note: str  # follows every link to a page that is not in this language
    text: dict = field(default_factory=dict)  # interface text (may hold HTML)
    faq: object = None  # (context dict) -> list of (question, answer HTML)
    doc_links: object = None  # (context dict) -> list of (href, title, text)


# ---------------------------------------------------------------- English

def english_faq(c: dict) -> list:
    repo, issues = c["repo"], c["issues"]
    return [
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
         f'<p>Yes. It is open source under the <a href="{c["license"]}">GNU Affero General Public License v3.0 or later</a>, '
         f'and the source code is on <a href="{repo}">GitHub</a>.</p>'),
        ("Where is the official download?",
         f'<p>Installers are published only on the <a href="{repo}/releases/latest">Releases page</a> of '
         f'the GitHub repository <a href="{repo}">{esc(c["repo_name"])}</a>. '
         "Other Deadlock tools with similar names are separate projects.</p>"),
        ("Which model files can I use?",
         f"<p>{esc(c['formats'])}. You can also add a gun from a separate file.</p>"),
        ("Which Blender versions work?",
         f"<p>Blender {esc(c['blender'])}.</p>"),
        ("Do I need to install Python?",
         "<p>No. Blender runs in the background to prepare the model; no separate Python installation is needed.</p>"),
        ("Does it include game files or character models?",
         "<p>No. The app does not include Valve tools, game files or third-party character models. "
         "You install Deadlock and the tools listed above yourself, and bring your own model.</p>"),
        ("Does the app send any data?", c["privacy"]),
        ("Is this an official Valve tool?",
         "<p>No. This is an unofficial tool, not affiliated with or endorsed by Valve. "
         "Deadlock and its assets belong to their respective owners.</p>"),
        ("Where do I report a problem?",
         f'<p>Use the <a href="{issues}">issue templates</a> on GitHub. Include the app version '
         "(Settings → About this build), the model's file format, the hero, the steps, and the text from "
         f"<strong>Copy for support</strong>. The <a href=\"{c['troubleshooting']}\">troubleshooting page</a> "
         "covers common problems.</p>"),
    ]


def english_doc_links(c: dict) -> list:
    repo = c["repo"]
    return [
        (c["tutorial"], "Step-by-step tutorial",
         "How to make your first Deadlock character mod, from installing the tools to checking it in game."),
        (c["guide"], "Model preparation guide",
         "The five steps, joint matches, textures, moving parts, the movement preview, gun customization "
         "and the Extras (sounds, effects, items, ability objects, colors)."),
        (c["troubleshooting"], "Troubleshooting", "Compatibility limits and fixes for common failures."),
        (c["changelog"], "Changelog", "What changed in each release."),
        (c["issues"], "Report a problem", "Bug reports and feature requests on GitHub."),
        (f"{repo}/releases", "All releases", "Installers and checksums for every version."),
        (repo, "Source code", "The app's source, licensed under the GNU AGPL v3.0 or later."),
    ]


ENGLISH = Language(
    code="en",
    prefix="",
    name="English",
    og_locale="en_US",
    readme="README.md",
    home=Page(
        path="",
        title="Deadlock Mod Builder – make Deadlock model swap mods",
        description=(
            "Free, open-source Windows app for Deadlock model swaps: it turns a Blender, FBX, glTF or MMD "
            "character into a hero replacement mod and installs it as a VPK."
        ),
        key="home",
    ),
    docs={
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
            key="tutorial",
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
            key="guide",
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
            key="troubleshooting",
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
            key="changelog",
        ),
    },
    sections={name: name for name in (
        "Features", "What you need", "Installation", "Quick start", "Troubleshooting", "Privacy",
        "Documentation", "Building from source", "Contributing and support", "Security", "Credits", "License",
    )},
    pitch_prefix="{name} is ",
    sentence_end=". ",
    step=r"\*\*(?P<name>.+?):\*\*\s*(?P<text>.+)",
    formats=r"\(([^()]*DMX[^()]*)\)",
    title_width=60,
    description_range=(70, 160),  # what search results show of an English description
    foreign_note="",
    text={
        "skip": "Skip to content",
        "nav_label": "Main",
        "nav_tutorial": "Tutorial",
        "nav_guide": "Guide",
        "nav_troubleshooting": "Troubleshooting",
        "nav_changelog": "Changelog",
        "nav_github": "GitHub",
        "nav_download": "Download",
        "language": "Language",
        "language_label": "Language: ",  # read before the language name on the button
        "footer": (
            '<p>Deadlock Mod Builder is open source under the <a href="{license}">GNU Affero General Public License v3.0 or later</a>. '
            'Source code and releases are on <a href="{repo}">GitHub</a>.</p>\n'
            "    <p>Unofficial tool, not affiliated with or endorsed by Valve. Deadlock and its assets belong to their respective owners.</p>"
        ),
        "toc": "On this page",
        "table": "Table",
        "og_image_alt": "Deadlock Mod Builder: the app's Character & model step next to its name",
        "download_windows": "Download for Windows",
        "view_github": "View on GitHub",
        "release_meta": "Version {version} &middot; Windows x64 installer &middot; free and open source (AGPL-3.0-or-later)",
        "steps_title": "A Deadlock model swap in five steps",
        "tutorial_button": "Step-by-step tutorial",
        "features_title": "Features",
        "requirements_title": "What you need",
        "install_title": "Installation",
        "faq_title": "Questions",
        "docs_title": "Documentation and help",
    },
    faq=english_faq,
    doc_links=english_doc_links,
)


# ---------------------------------------------------------------- Simplified Chinese

def chinese_faq(c: dict) -> list:
    repo, issues, note = c["repo"], c["issues"], c["note"]
    return [
        ("Deadlock Mod Builder 能做什么？",
         "<p>它为 Deadlock 制作模型替换：用你自己的角色替换游戏中某个英雄的模型，包括它的枪械、材质，"
         "以及会摆动的头发和布料。这类模组也被称为英雄替换模组或自定义皮肤。制作结果是一个 VPK 文件，"
         "应用会把它安装到游戏的 <code>addons</code> 文件夹中。</p>"),
        ("它是模组管理器吗？",
         "<p>不是。模组管理器用来下载和安装别人制作的模组。Deadlock Mod Builder "
         "用你自己的模型文件制作模组，并安装你用它构建的模组。</p>"),
        ("除了角色模型，还能改别的吗？",
         "<p>可以。附加内容（Extras）可以替换英雄图标、技能图片和声音，为技能和武器特效重新着色，"
         "把你自己的模型放到技能物体上，还能为商店中的物品换上新的图标和名称。"
         "动作（Motion）可以替换英雄的动画片段。</p>"),
        ("Deadlock Mod Builder 免费吗？",
         f'<p>免费。它是开源软件，采用 <a href="{c["license"]}">GNU Affero 通用公共许可证 v3.0 或更高版本</a>'
         f'（AGPL-3.0-or-later），源代码在 <a href="{repo}">GitHub</a> 上。</p>'),
        ("官方下载在哪里？",
         f'<p>安装程序只发布在 GitHub 仓库 <a href="{repo}">{esc(c["repo_name"])}</a> 的 '
         f'<a href="{repo}/releases/latest">Releases 页面</a>上。名称相似的其他 Deadlock 工具都是独立的项目。</p>'),
        ("可以使用哪些模型文件？",
         f"<p>{esc(c['formats'])}。还可以从单独的文件添加枪械。</p>"),
        ("支持哪些 Blender 版本？",
         f"<p>Blender {esc(c['blender'])}。</p>"),
        ("需要安装 Python 吗？",
         "<p>不需要。Blender 在后台运行来准备模型，无需另外安装 Python。</p>"),
        ("它包含游戏文件或角色模型吗？",
         "<p>不包含。本应用不包含 Valve 的工具、游戏文件或第三方角色模型。"
         "Deadlock 和上面列出的工具需要你自己安装，模型也需要你自备。</p>"),
        ("应用会发送数据吗？", c["privacy"]),
        ("这是 Valve 的官方工具吗？",
         "<p>不是。这是一个非官方工具，与 Valve 无关，也未获得 Valve 的认可。"
         "Deadlock 及其资源归各自的所有者所有。</p>"),
        ("在哪里报告问题？",
         f'<p>请使用 GitHub 上的<a href="{issues}">问题模板</a>。请附上应用版本'
         "（设置（Settings）→ 关于此版本（About this build））、模型的文件格式、英雄、操作步骤，"
         "以及<strong>复制支持信息</strong>（Copy for support）得到的文本。"
         f'<a href="{c["troubleshooting"]}" hreflang="en">故障排除页面</a>{note}介绍了常见问题。</p>'),
    ]


def chinese_doc_links(c: dict) -> list:
    repo, note = c["repo"], c["note"]
    return [
        (c["tutorial"], "分步教程",
         "如何制作你的第一个 Deadlock 角色模组：从安装工具到在游戏中检查。"),
        (c["guide"], "模型准备指南" + note,
         "五个步骤、关节匹配、贴图、活动部件、动态预览、枪械自定义，以及附加内容"
         "（声音、特效、物品、技能物体、颜色）。"),
        (c["troubleshooting"], "故障排除" + note, "兼容性限制和常见故障的解决方法。"),
        (c["changelog"], "更新日志" + note, "每个版本的更改内容。"),
        (c["issues"], "报告问题", "在 GitHub 上提交错误报告和功能请求。"),
        (f"{repo}/releases", "所有版本", "每个版本的安装程序和校验值。"),
        (repo, "源代码", "应用的源代码，采用 GNU AGPL v3.0 或更高版本授权。"),
    ]


CHINESE = Language(
    code="zh-Hans",
    prefix="zh-cn/",
    name="简体中文",
    og_locale="zh_CN",
    readme="README.zh-CN.md",
    home=Page(
        path="zh-cn/",
        title="Deadlock Mod Builder – 制作 Deadlock 模型替换模组",
        description=(
            "免费开源的 Windows 应用，用于制作 Deadlock 模型替换：把 Blender、FBX、glTF 或 MMD "
            "角色做成英雄替换模组，并以 VPK 形式安装。"
        ),
        key="home",
        lang="zh-Hans",
    ),
    docs={
        "docs/zh-CN/FIRST_MOD.md": Page(
            path="zh-cn/make-a-deadlock-character-mod/",
            title="如何制作 Deadlock 角色模组（模型替换教程）",
            description=(
                "Deadlock 模型替换分步教程：用你自己的 Blender、FBX、glTF 或 MMD 角色替换英雄的模型，"
                "并以 VPK 形式安装模组。"
            ),
            nav="tutorial",
            label="分步教程",
            toc=True,
            key="tutorial",
            lang="zh-Hans",
        ),
    },
    sections={
        "Features": "功能",
        "What you need": "所需环境",
        "Installation": "安装",
        "Quick start": "快速上手",
        "Troubleshooting": "故障排除",
        "Privacy": "隐私",
        "Documentation": "文档",
        "Building from source": "从源代码构建",
        "Contributing and support": "参与贡献与支持",
        "Security": "安全",
        "Credits": "致谢",
        "License": "许可证",
    },
    pitch_prefix="{name} 是",
    sentence_end="。",
    # **角色与模型**（Character & model）：…  The bold ends before the full-width
    # parenthesis and colon: CommonMark does not close ** right after punctuation
    # that is followed by a letter.
    step=r"\*\*(?P<name>.+?)\*\*(?P<orig>（[^（）]*）)?：\s*(?P<text>.+)",
    formats=r"（([^（）]*DMX[^（）]*)）",
    title_width=60,
    # Search results show roughly 80 Chinese characters of a description, each
    # about twice as wide as a Latin letter: 40 to 80 Chinese characters, that
    # is a width of 80 to 160 (Latin letters and digits in it count 1).
    description_range=(80, 160),
    foreign_note="（英文）",
    text={
        "skip": "跳到正文",
        "nav_label": "主导航",
        "nav_tutorial": "教程",
        "nav_guide": '指南<span class="nav-note">（英文）</span>',
        "nav_troubleshooting": '故障排除<span class="nav-note">（英文）</span>',
        "nav_changelog": '更新日志<span class="nav-note">（英文）</span>',
        "nav_github": "GitHub",
        "nav_download": "下载",
        "language": "语言",
        "language_label": "语言：",
        "footer": (
            '<p>Deadlock Mod Builder 是开源软件，采用 <a href="{license}">GNU Affero 通用公共许可证 v3.0 或更高版本</a>'
            '（AGPL-3.0-or-later）。源代码和各个版本发布在 <a href="{repo}">GitHub</a> 上。</p>\n'
            "    <p>非官方工具，与 Valve 无关，也未获得 Valve 的认可。Deadlock 及其资源归各自的所有者所有。</p>"
        ),
        "toc": "本页内容",
        "table": "表格",
        "og_image_alt": "Deadlock Mod Builder：应用名称旁是应用的“角色与模型”步骤",
        "download_windows": "下载 Windows 版",
        "view_github": "在 GitHub 上查看",
        "release_meta": "版本 {version} &middot; Windows x64 安装程序 &middot; 免费开源（AGPL-3.0-or-later）",
        "steps_title": "五步完成 Deadlock 模型替换",
        "tutorial_button": "分步教程",
        "features_title": "功能",
        "requirements_title": "所需环境",
        "install_title": "安装",
        "faq_title": "常见问题",
        "docs_title": "文档与帮助",
    },
    faq=chinese_faq,
    doc_links=chinese_doc_links,
)

LANGUAGES = [ENGLISH, CHINESE]


def display_width(text: str) -> int:
    """Width in Latin letters: East Asian wide characters count 2."""
    return sum(2 if unicodedata.east_asian_width(ch) in ("W", "F") else 1 for ch in text)
