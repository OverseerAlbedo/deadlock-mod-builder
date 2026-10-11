"""Descriptive bone-name differences; these helpers do not relax validation."""
from __future__ import annotations

from collections import Counter


def identity_difference(actual_names, reference_names):
    """Return the complete, deterministic name difference for audit reports.

    Counts include repeated entries. Lists contain each affected name once,
    sorted for readable comparison independently of compiler bone ordering.
    Validation remains the responsibility of the caller.
    """
    actual, reference = Counter(actual_names), Counter(reference_names)
    return {
        "original_count": sum(reference.values()),
        "rebuilt_count": sum(actual.values()),
        "missing": sorted(reference.keys() - actual.keys()),
        "extra": sorted(actual.keys() - reference.keys()),
        "duplicates": sorted(name for name, count in actual.items() if count > 1),
        "reference_duplicates": sorted(name for name, count in reference.items() if count > 1),
    }


def format_difference(difference, *, actual_label="rebuilt", limit=24):
    """Format a bounded message while retaining full lists in ``difference``."""
    def names(label, values):
        shown = values[:max(0, limit)]
        text = ", ".join(repr(name) for name in shown) if values else "none"
        if len(shown) < len(values):
            text += (", " if text else "") + f"... (+{len(values) - len(shown)} more)"
        return f"{label} ({len(values)}): {text}"

    parts = [f"original {difference['original_count']}, {actual_label} {difference['rebuilt_count']}",
             names("missing", difference["missing"]),
             names("extra", difference["extra"]),
             names("duplicates", difference["duplicates"])]
    if difference["reference_duplicates"]:
        parts.append(names("original duplicates", difference["reference_duplicates"]))
    return "; ".join(parts)
