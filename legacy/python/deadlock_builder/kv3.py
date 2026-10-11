"""Lossless text KV3 editing: retain every token outside deliberately edited spans.

Unknown scalar types remain opaque. No regex replacement over nested VMDL blocks.
"""
from __future__ import annotations
from dataclasses import dataclass
import json
import re
from .safety import BuildError

TOKEN = re.compile(r'\s+|<!--.*?-->|//[^\n]*|/\*.*?\*/|""".*?"""|"(?:\\.|[^"\\])*"|[{}\[\]=,:]|[^\s{}\[\]=,:]+', re.S)


@dataclass
class Node:
    start: int
    end: int
    kind: str
    value: object
    members: dict | None = None

    def get(self, key, default=None):
        n = (self.members or {}).get(key)
        return n.value if n else default


class Document:
    def __init__(self, text: str):
        self.text = text
        self.tokens = [(m.group(), m.start(), m.end()) for m in TOKEN.finditer(text)
                       if not m.group().isspace() and not m.group().startswith(("<!--", "//", "/*"))]
        self.i = 0
        self.root = self._value()
        if self.i != len(self.tokens) or self.root.kind != "object":
            raise BuildError("Unsupported or malformed text KV3 document.")

    def _take(self, expected=None):
        if self.i >= len(self.tokens):
            raise BuildError("Unexpected end of KV3 document.")
        token = self.tokens[self.i]
        self.i += 1
        if expected and token[0] != expected:
            raise BuildError(f"Expected {expected} in KV3, found {token[0]}.")
        return token

    def _peek(self):
        return self.tokens[self.i][0] if self.i < len(self.tokens) else None

    def _value(self):
        t, a, b = self._take()
        if t == "#":
            # Source2Viewer emits binary collision/physics fields as #[hex].
            # Consume the complete literal; treating '#' as a scalar makes
            # its '[' look like the next object key. Keep bytes typed and
            # retain the original span so unrelated edits remain lossless.
            self._take("[")
            data = bytearray()
            while self._peek() != "]":
                if self._peek() is None:
                    raise BuildError("Unterminated KV3 binary blob.")
                token = self._take()[0]
                if not re.fullmatch(r"(?:[0-9a-fA-F]{2})+", token):
                    raise BuildError(f"Invalid hexadecimal bytes in KV3 binary blob: {token!r}.")
                data.extend(bytes.fromhex(token))
            _, _, b = self._take("]")
            return Node(a, b, "blob", bytes(data))
        if t == "{":
            members = {}
            while self._peek() != "}":
                k = self._take()[0]
                if k == ",":
                    continue
                k = json.loads(k) if k.startswith('"') else k
                self._take("=")
                if k in members:
                    raise BuildError(f"Duplicate KV3 key: {k}")
                members[k] = self._value()
            _, _, b = self._take("}")
            return Node(a, b, "object", {k: v.value for k, v in members.items()}, members)
        if t == "[":
            values = []
            while self._peek() != "]":
                if self._peek() == ",":
                    self._take()
                else:
                    values.append(self._value())
            _, _, b = self._take("]")
            return Node(a, b, "array", values)
        if self._peek() == ":":
            self._take(":")
            sub = self._value()
            return Node(a, sub.end, "typed", (t, sub.value))
        value = t[3:-3] if t.startswith('"""') else (json.loads(t) if t.startswith('"') else t)
        return Node(a, b, "scalar", value)

    def objects(self, class_name=None):
        def walk(node):
            if node.kind == "object":
                if class_name is None or node.get("_class") == class_name:
                    yield node
                for v in node.members.values():
                    yield from walk(v)
            elif node.kind == "array":
                for v in node.value:
                    yield from walk(v)
        return list(walk(self.root))

    def apply(self, edits: list[tuple[int, int, str]]):
        out = self.text
        last = len(out) + 1
        for start, end, value in sorted(edits, reverse=True):
            if end > last:
                raise BuildError("Overlapping source edits refused.")
            out = out[:start] + value + out[end:]
            last = start
        Document(out)  # Validate the result before any write.
        return out


def field_edit(node: Node, key: str, value: str):
    encoded = json.dumps(value, ensure_ascii=False)
    old = node.members.get(key)
    if old:
        return old.start, old.end, encoded
    return node.end - 1, node.end - 1, f"\n\t\t{key} = {encoded}\n\t"
