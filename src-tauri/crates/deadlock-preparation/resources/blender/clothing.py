"""Bind checks for a separately authored, already fitted garment.

No fitting, added bones or physics are inferred. A supplied garment armature
must describe the same source rest skeleton before its meshes can join it.
"""
import numpy as np


def validate_garment_binding(source, garment, weighted_names):
    source_by_name = {bone['name']: bone for bone in source}
    if not source_by_name:
        raise ValueError('Weighted clothing needs a source character armature.')
    missing = sorted(set(weighted_names) - set(source_by_name))
    if missing:
        raise ValueError('Clothing weights reference bones outside the character rig: '+', '.join(missing[:12]))
    if garment:
        garment_by_name = {bone['name']: bone for bone in garment}
        if set(source_by_name) != set(garment_by_name):
            raise ValueError('Clothing uses a different rest skeleton. Fit and weight the garment on the same character rig before importing.')
        for name, bone in garment_by_name.items():
            expected = source_by_name[name]
            actual_matrix = np.asarray(bone['matrix'],dtype=float)
            expected_matrix = np.asarray(expected['matrix'],dtype=float)
            # This tolerance covers float storage only, not arbitrary fitting.
            if (bone.get('parent') != expected.get('parent') or actual_matrix.shape != (4,4)
                    or not np.isfinite(actual_matrix).all()
                    or not np.allclose(actual_matrix,expected_matrix,rtol=1e-7,atol=1e-6)):
                raise ValueError('Clothing rest transform differs from the character at '+name+
                                 '. Export both in the same rest pose, coordinates and scale.')
