"""Bounded DMX binary 4/5/9 reader and lossless attribute writer.

Type layout follows Tom Edwards' MIT-licensed Blender Source Tools datamodel.py.
See licenses/BlenderSourceTools-DMX.txt. Compiled Source 2 resources are always
handled by Source 2 Viewer / Valve's compiler, never by this module.
"""
from __future__ import annotations
from dataclasses import dataclass, field
from collections.abc import Sequence
from pathlib import Path
import re
import struct
from .safety import BuildError, atomic_write


class PackedArray(Sequence):
    """Read-only, zero-copy numeric DMX array; large meshes must not box every value."""
    def __init__(self, view, base, size):
        self.view, self.base, self.size = view, base, size
        self.scalar = {1: "i", 2: "i", 3: "f", 4: "?", 7: "i"}.get(base)

    def __len__(self):
        return len(self.view) // self.size

    def __getitem__(self, item):
        if isinstance(item, slice):
            return [self[i] for i in range(*item.indices(len(self))) ]
        if item < 0:
            item += len(self)
        if not 0 <= item < len(self):
            raise IndexError(item)
        if self.scalar:
            return struct.unpack_from("<" + self.scalar, self.view, item * self.size)[0]
        return bytes(self.view[item * self.size:(item + 1) * self.size])

    def __iter__(self):
        if self.scalar:
            return (value[0] for value in struct.iter_unpack("<" + self.scalar, self.view))
        return (self[i] for i in range(len(self)))

    def __eq__(self, other):
        if not isinstance(other, Sequence) or len(self) != len(other):
            return False
        return all(a == b for a, b in zip(self, other))


@dataclass
class Attribute:
    name: str
    type: int
    value: object
    raw: bytes
    array: bool = False


@dataclass
class Element:
    type: str
    name: str
    uuid: bytes
    attributes: list[Attribute] = field(default_factory=list)

    def get(self, name, default=None):
        return next((a.value for a in self.attributes if a.name == name), default)


class Reader:
    def __init__(self, data: bytes):
        self.data, self.pos = data, 0

    def take(self, size):
        if size < 0 or self.pos + size > len(self.data):
            raise BuildError(f"Truncated or invalid DMX at byte {self.pos}.")
        result = self.data[self.pos:self.pos + size]
        self.pos += size
        return result

    def integer(self):
        return struct.unpack("<i", self.take(4))[0]

    def count(self):
        n = self.integer()
        if n < 0 or n > 20_000_000 or n > len(self.data):
            raise BuildError("Invalid DMX array count.")
        return n

    def string(self):
        end = self.data.find(b"\0", self.pos)
        if end < 0 or end - self.pos > 1_000_000:
            raise BuildError("Unterminated DMX string.")
        raw = self.take(end - self.pos + 1)
        try:
            return raw[:-1].decode("utf-8")
        except UnicodeDecodeError as ex:
            raise BuildError("Invalid UTF-8 in DMX string.") from ex


class Dmx:
    SIZES = {1: 4, 2: 4, 3: 4, 4: 1, 7: 4, 8: 4, 9: 8, 10: 12,
             11: 16, 12: 12, 13: 16, 14: 64, 15: 8, 16: 1}

    def __init__(self, data: bytes):
        self.data = data
        self.reader = r = Reader(data)
        end = data.find(b"\0", 0, 512)
        if end == -1:
            raise BuildError("DMX must be a Blender binary 4, 5 or 9 export.")
        self.header = r.take(end + 1)
        match = re.search(rb"dmx encoding binary (\d+) format (\w+) (\d+)", self.header)
        if not match or int(match[1]) not in {4, 5, 9}:
            raise BuildError("Unsupported DMX encoding. Export binary DMX 9 / model 22 in Blender Source Tools.")
        self.version = int(match[1])
        self.format = match[2].decode()
        self.format_version = int(match[3])
        if self.version == 9 and r.integer() != 0:
            raise BuildError("DMX prefix attributes are unsupported. Export without prefix metadata.")
        self.strings = [r.string() for _ in range(r.count())]
        self.pool_end = r.pos
        count = r.count()
        self.elements = [Element(self.pool_string(), self.pool_string(), r.take(16)) for _ in range(count)]
        for elem in self.elements:
            for _ in range(r.count()):
                name = self.pool_string()
                type_id = r.take(1)[0]
                array = type_id >= (32 if self.version == 9 else 15)
                base = type_id - (32 if self.version == 9 else 14) if array else type_id
                if base not in {*self.SIZES, 5, 6} or (self.version != 9 and base > 14):
                    raise BuildError(f"Unknown DMX attribute type: {type_id}.")
                start = r.pos
                n = r.count() if array else 1
                if array and base in self.SIZES:
                    length = n * self.SIZES[base]
                    if r.pos + length > len(data):
                        raise BuildError("Truncated DMX numeric array.")
                    values = PackedArray(memoryview(data)[r.pos:r.pos + length], base, self.SIZES[base])
                    r.pos += length
                else:
                    values = [self._read_one(base, array) for _ in range(n)]
                raw = memoryview(data)[start:r.pos]
                elem.attributes.append(Attribute(name, type_id, values if array else values[0], raw, array))
        if r.pos != len(data):
            # Binary writers may append a single newline, but never tolerate unknown payloads.
            if data[r.pos:].strip(b"\0\r\n\t "):
                raise BuildError(f"Unexpected data after DMX element table at {r.pos}.")
        self._validate_refs()

    @classmethod
    def load(cls, path: Path):
        if not path.is_file():
            raise BuildError(f"Replacement DMX not found: {path}")
        if path.stat().st_size > 512 * 1024 * 1024:
            raise BuildError("DMX exceeds this release's 512 MiB input limit.")
        return cls(path.read_bytes())

    def pool_string(self):
        r = self.reader
        index = struct.unpack("<H", r.take(2))[0] if self.version == 4 else r.integer()
        if index == -1 or (self.version == 4 and index == 65535):
            return ""
        if not 0 <= index < len(self.strings):
            raise BuildError(f"Invalid DMX string index {index} at {r.pos}.")
        return self.strings[index]

    def _read_one(self, base, array):
        r = self.reader
        if base == 5:
            return r.string() if array else self.pool_string()
        if base == 6:
            return r.take(r.count())
        if base == 1:
            ref = r.integer()
            if ref == -2:
                # External UUID element references are not safe to reindex.
                raise BuildError("DMX external element references are unsupported.")
            return ref
        raw = r.take(self.SIZES[base])
        if base == 2 or base == 7:
            return struct.unpack("<i", raw)[0]
        if base == 3:
            return struct.unpack("<f", raw)[0]
        if base == 4:
            return bool(raw[0])
        return raw

    def _validate_refs(self):
        for elem in self.elements:
            for a in elem.attributes:
                if a.type in (1, 33 if self.version == 9 else 15):
                    refs = a.value if a.array else [a.value]
                    if any(x < -1 or x >= len(self.elements) for x in refs):
                        raise BuildError("DMX contains an out-of-range element reference.")

    def summary(self):
        meshes = []
        names = set()
        for i, elem in enumerate(self.elements):
            if elem.type != "DmeMesh":
                continue
            if elem.name in names:
                raise BuildError(f"Duplicate source mesh name {elem.name!r}; rename in Blender before exporting.")
            names.add(elem.name)
            ref = elem.get("bindState", elem.get("currentState", -1))
            state = self.elements[ref] if isinstance(ref, int) and ref >= 0 else None
            attributes = {a.name: a for a in state.attributes} if state else {}
            positions = next((a.value for n, a in attributes.items() if n in ("positions", "position$0")), [])
            weights = next((a.value for n, a in attributes.items() if n in ("jointWeights", "blendweights$0")), [])
            indices = next((a.value for n, a in attributes.items() if n in ("jointIndices", "blendindices$0")), [])
            position_indices = next((a.value for n, a in attributes.items() if n in ("positionsIndices", "position$0Indices")), [])
            meshes.append({"name": elem.name, "element": i, "vertices": len(positions),
                           "indexed_vertices": len(position_indices),
                           "weight_values": len(weights), "joint_index_values": len(indices),
                           "has_skin_weights": bool(weights), "vertex_attributes": sorted(attributes)})
        if not meshes:
            raise BuildError("Replacement DMX contains no DmeMesh elements.")
        identities = [{"name": e.name, "uuid": e.uuid.hex(), "path": str(e.get("mtlName", e.name)).replace("\\", "/")}
                      for e in self.elements if e.type == "DmeMaterial"]
        materials = sorted({m["path"] for m in identities})
        bones = [e.name for e in self.elements if e.type == "DmeJoint"]
        return {"encoding": f"binary {self.version}", "format": self.format,
                "format_version": self.format_version, "meshes": meshes,
                "materials": materials, "material_identities": identities, "bones": bones, "elements": len(self.elements)}

    def geometry_counts(self):
        """Count polygon corners from face-set terminators; do not triangulate or edit."""
        results = []
        for mesh in self.summary()["meshes"]:
            e = self.elements[mesh["element"]]
            polygons = triangles = corners = 0
            for ref in e.get("faceSets", []):
                if ref < 0:
                    continue
                face = self.elements[ref]
                n = 0
                for index in face.get("faces", []):
                    if index == -1:
                        if n < 3:
                            raise BuildError("DMX contains a face with fewer than three corners.")
                        polygons += 1
                        triangles += n - 2
                        corners += n
                        n = 0
                    elif index < 0:
                        raise BuildError("Invalid DMX face index.")
                    else:
                        n += 1
                if n:
                    raise BuildError("Unterminated DMX face set.")
            results.append({"name": mesh["name"], "vertices": mesh["vertices"], "polygons": polygons,
                            "triangles": triangles, "corners": corners})
        return {"meshes": results, "source_triangles": sum(m["triangles"] for m in results)}

    def single_mesh(self, name: str) -> bytes:
        """Remove other mesh elements; preserve bones, transforms, skinning and opaque attributes.

        An isolated source avoids reliance on undocumented ModelDoc checkbox schemas.
        Every element reference is reindexed. Null DAG shapes are legal in DMX.
        """
        if name not in [m["name"] for m in self.summary()["meshes"]]:
            raise BuildError(f"Replacement DMX does not contain mapped mesh: {name}")
        keep = [i for i, e in enumerate(self.elements) if e.type != "DmeMesh" or e.name == name]
        remap = {old: new for new, old in enumerate(keep)}
        out = bytearray(self.data[:self.pool_end])
        out.extend(struct.pack("<i", len(keep)))
        index = {s: i for i, s in enumerate(self.strings)}
        def pool(value):
            return struct.pack("<H" if self.version == 4 else "<i", index.get(value, 65535 if self.version == 4 else -1))
        for old in keep:
            e = self.elements[old]
            out.extend(pool(e.type) + pool(e.name) + e.uuid)
        for old in keep:
            e = self.elements[old]
            out.extend(struct.pack("<i", len(e.attributes)))
            for a in e.attributes:
                out.extend(pool(a.name) + bytes([a.type]))
                if a.type == 1:
                    out.extend(struct.pack("<i", remap.get(a.value, -1)))
                elif a.type == (33 if self.version == 9 else 15):
                    refs = [remap.get(x, -1) for x in a.value]
                    out.extend(struct.pack("<i", len(refs)))
                    out.extend(struct.pack(f"<{len(refs)}i", *refs))
                else:
                    out.extend(a.raw)
        return bytes(out)

    def write_mesh(self, name: str, path: Path):
        data = self.single_mesh(name)
        check = Dmx(data)
        if [m["name"] for m in check.summary()["meshes"]] != [name]:
            raise BuildError("Isolated DMX validation failed.")
        atomic_write(path, data)

    def rewritten_binary9(self, attributes=None, names=None, identities=None):
        """Keep element indices and opaque attribute bytes; replace explicit payloads only.

        Attribute overrides are (element index, attribute name) -> raw payload.
        No graph pruning, value conversion, float formatting or bone reindexing.
        """
        if self.version != 9:
            raise BuildError("Generated split sources currently require binary DMX 9.")
        attributes, names, identities = attributes or {}, names or {}, identities or {}
        strings = list(self.strings)
        for name in names.values():
            if not name or '\0' in name:
                raise BuildError("Invalid generated DMX name.")
            if name not in strings:
                strings.append(name)
        pool = {s: i for i, s in enumerate(strings)}
        out = bytearray(self.header + struct.pack('<ii', 0, len(strings)))
        for value in strings:
            out.extend(value.encode('utf-8') + b'\0')
        out.extend(struct.pack('<i', len(self.elements)))
        for i, e in enumerate(self.elements):
            out.extend(struct.pack('<ii', pool[e.type], pool[names.get(i, e.name)]))
            out.extend(identities.get(i, e.uuid))
        used = set()
        for i, e in enumerate(self.elements):
            out.extend(struct.pack('<i', len(e.attributes)))
            for a in e.attributes:
                out.extend(struct.pack('<iB', pool[a.name], a.type))
                key = (i, a.name)
                out.extend(attributes.get(key, a.raw))
                if key in attributes:
                    used.add(key)
        if used != set(attributes):
            raise BuildError("Split writer attempted to replace an unknown attribute.")
        return bytes(out)
