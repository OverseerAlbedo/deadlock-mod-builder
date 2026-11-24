"""Tell the IndexNow search engines (Bing and others) which site pages changed.

    python site/indexnow.py            # the pages with the newest date in the sitemap
    python site/indexnow.py --all      # every page in the sitemap
    python site/indexnow.py --dry-run  # show the request, send nothing

The Website workflow runs this after a deployment. It reads the published
sitemap, so the pages and the key file are online when the engines check them.
The key is public by design: it is the name and the content of the one .txt
file in site/static/ (https://www.indexnow.org/documentation). A submission
asks for a crawl; it does not guarantee one.
"""

from __future__ import annotations

import json
import re
import sys
import time
import urllib.error
import urllib.request
import xml.etree.ElementTree as ET
from pathlib import Path
from urllib.parse import urlsplit

SITE = Path(__file__).resolve().parent
ENDPOINT = "https://api.indexnow.org/indexnow"
SITEMAP_NS = "{http://www.sitemaps.org/schemas/sitemap/0.9}"
ANSWERS = {
    200: "submitted",
    202: "received; the key file is still being checked",
    400: "invalid format",
    403: "key not valid (key file not found, or the key is not in it)",
    422: "the pages do not belong to the host, or the key does not match",
    429: "too many requests",
}


class NotifyError(Exception):
    pass


def fetch(url: str) -> bytes:
    request = urllib.request.Request(url, headers={"User-Agent": "site-indexnow"})
    with urllib.request.urlopen(request, timeout=30) as response:
        return response.read()


def find_key() -> str:
    keys = [
        path.stem
        for path in sorted((SITE / "static").glob("*.txt"))
        if re.fullmatch(r"[A-Za-z0-9-]{8,128}", path.stem)
        and path.read_text(encoding="utf-8").strip() == path.stem
    ]
    if len(keys) != 1:
        raise NotifyError(f"expected one IndexNow key file in site/static/, found {len(keys)}")
    return keys[0]


def changed_pages(base_url: str, every_page: bool) -> list[str]:
    try:
        root = ET.fromstring(fetch(base_url + "sitemap.xml"))
    except (urllib.error.URLError, OSError, ET.ParseError) as error:
        raise NotifyError(f"cannot read the published sitemap: {error}") from error
    pages = []
    for url in root.iter(SITEMAP_NS + "url"):
        loc = (url.findtext(SITEMAP_NS + "loc") or "").strip()
        date = (url.findtext(SITEMAP_NS + "lastmod") or "").strip()
        if loc.startswith(base_url):
            pages.append((loc, date))
    if not pages:
        raise NotifyError("the published sitemap lists no pages of this site")
    if not every_page:
        newest = max(date for _, date in pages)
        pages = [page for page in pages if page[1] == newest]
    return [loc for loc, _ in pages]


def key_is_online(key_location: str, key: str) -> bool:
    for attempt in range(6):
        if attempt:
            time.sleep(10)
        try:
            if fetch(key_location).decode("utf-8").strip() == key:
                return True
        except (urllib.error.URLError, OSError, UnicodeDecodeError):
            pass
    return False


def notify(args: set[str]) -> int:
    unknown = args - {"--all", "--dry-run"}
    if unknown:
        raise NotifyError(f"unknown option: {' '.join(sorted(unknown))}")
    try:
        base_url = json.loads((SITE / "site.json").read_text(encoding="utf-8"))["baseUrl"]
    except (OSError, ValueError, KeyError) as error:
        raise NotifyError(f"cannot read 'baseUrl' from site/site.json: {error}") from error
    key = find_key()
    key_location = f"{base_url}{key}.txt"
    body = {
        "host": urlsplit(base_url).netloc,
        "key": key,
        "keyLocation": key_location,
        "urlList": changed_pages(base_url, "--all" in args),
    }
    if "--dry-run" in args:
        print(f"Would send to {ENDPOINT}:")
        print(json.dumps(body, indent=2))
        return 0
    if not key_is_online(key_location, key):
        raise NotifyError(f"the key file is not online at {key_location}")

    request = urllib.request.Request(
        ENDPOINT,
        data=json.dumps(body).encode("utf-8"),
        headers={"Content-Type": "application/json; charset=utf-8", "User-Agent": "site-indexnow"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            status = response.status
    except urllib.error.HTTPError as error:
        status = error.code
    except (urllib.error.URLError, OSError) as error:
        raise NotifyError(f"cannot reach {ENDPOINT}: {error}") from error
    print(f"IndexNow answered {status} ({ANSWERS.get(status, 'unexpected answer')}) for:")
    for loc in body["urlList"]:
        print(f"  {loc}")
    return 0 if status in (200, 202) else 1


def main() -> int:
    try:
        return notify(set(sys.argv[1:]))
    except NotifyError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
