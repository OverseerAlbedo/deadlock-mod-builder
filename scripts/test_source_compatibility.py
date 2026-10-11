"""Deterministic synthetic coverage for incomplete rest-skeleton recognition."""
import copy
import json
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT/'legacy/python'), str(ROOT/'src-tauri/crates/deadlock-preparation/resources/blender')]
from tests.preparation_fixture import humanoid
from algorithms import identify, name_role
from source_compatibility import plan


def rename(bones, mapping):
    for b in bones:
        b['name'] = mapping.get(b['name'], b['name'])
        b['parent'] = mapping.get(b['parent'], b['parent'])
    return bones


def incomplete():
    bones = humanoid()
    names = {r:'Bip001_'+r for r in ['pelvis','spine','head']}
    names['chest'] = 'Bip001_Spine1'
    for role in ['neck','upperarm_L','upperarm_R','thigh_L','thigh_R']:
        base,_,side = role.partition('_')
        names[role] = 'Bip001_B_'+(side+'_' if side else '')+base+'_Twist'
    for b in bones:
        if b['name'] in ['neck','head','upperarm_L','upperarm_R','forearm_L','forearm_R','thigh_L','thigh_R','shin_L','shin_R']:
            b['parent'] = None
    for role in ['upperarm_L','upperarm_R','thigh_L','thigh_R']:
        b = next(b for b in bones if b['name']==role)
        bones.append(dict(b, name='Bip001_B_'+role[-1]+('_Deltoid' if role.startswith('upperarm') else '_Hip'), parent=None))
    return rename(bones,names)


class RecoveryTests(unittest.TestCase):
    def check(self,bones):
        return plan(bones,[b['name'] for b in bones])

    def test_healthy_rigs_are_unchanged(self):
        for style in ['target','mixamo','bip']:
            with self.subTest(style=style):
                bones=humanoid(style);before=copy.deepcopy(bones)
                self.assertIsNone(self.check(bones));self.assertEqual(bones,before)

    def test_complete_recovery_is_order_independent_and_read_only(self):
        bones=incomplete();before=copy.deepcopy(bones);p=self.check(bones)
        self.assertIsNotNone(p);self.assertEqual(p,self.check(list(reversed(bones))))
        self.assertEqual(bones,before);self.assertEqual(len(p['promoted']),5)
        recovered=[dict(b,parent=p['parents'].get(b['name'],b['parent'])) for b in bones]
        self.assertFalse(identify(recovered,p['roles'])['errors'])

    def test_partial_or_ambiguous_evidence_never_repairs(self):
        for change in ['duplicate','missing-pivot','wrong-side','wrong-parent','missing-neck','upside-down']:
            bones=incomplete();name='Bip001_B_L_upperarm_Twist';joint=next(b for b in bones if b['name']==name)
            if change=='duplicate':bones.append(dict(joint,name='other:'+name))
            elif change=='missing-pivot':bones=[b for b in bones if b['name']!='Bip001_B_L_Deltoid']
            elif change=='wrong-side':joint['position']=[-.3,0,1.5]
            elif change=='wrong-parent':joint['parent']='foot_R'
            elif change=='missing-neck':bones=[b for b in bones if b['name']!='Bip001_B_neck_Twist']
            elif change=='upside-down':
                for b in bones:b['position'][2]*=-1
            with self.subTest(change=change):self.assertIsNone(self.check(bones))

    def test_unknown_joints_require_an_exact_anchored_chain(self):
        bones=rename(humanoid(),{'forearm_L':'joint_42','shin_R':'joint_90','neck':'joint_17'})
        p=self.check(bones);self.assertEqual(p['roles']['forearm_L'],'joint_42');self.assertFalse(p['parents'])
        # Two possible segments instead of one cannot pick a joint by distance.
        hand=next(b for b in bones if b['name']=='hand_L')
        bones.append(dict(hand,name='another_joint',parent='joint_42',position=[.75,0,1.5]))
        hand['parent']='another_joint';self.assertIsNone(self.check(bones))

    def test_biped_two_spines_use_hierarchy(self):
        bones=rename(humanoid(),{'spine':'Bip001_Spine','chest':'Bip001_Spine1'})
        p=self.check(bones);self.assertEqual(p['roles']['spine'],'Bip001_Spine');self.assertEqual(p['roles']['chest'],'Bip001_Spine1')

    def test_namespaces_are_exact_and_do_not_capture_helpers(self):
        for name,role in [('ValveBiped.Bip01_L_UpperArm','upperarm_L'),('DEF-forearm.L','forearm_L'),
                          ('J_Bip_C_Hips','pelvis'),('J_Bip_L_UpperArm','upperarm_L'),('J_Bip_R_Foot','foot_R'),
                          ('MCH-upperarm.L',None),('DEF-upperarm.L.001',None),('Bip001_B_NeckTwist',None)]:
            self.assertEqual(name_role(name),role,name)


if __name__=='__main__':
    unittest.main()
