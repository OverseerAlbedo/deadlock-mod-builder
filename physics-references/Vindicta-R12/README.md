# Original Vindicta R12 reference

This byte-preserved resource is retained for regression tests and explicit rollback. The current included-reference action selects the accepted balanced file in `../Vindicta-R12-Balanced/`.

SHA-256: `82e7212cfd93138c663ece73c6d5145f5146bca19f27942c7d769514354ebe7e`

For a matching Vindicta project, Physics supports selecting a tested cloth reference. Only its supported physics is transferred, not visible mesh or animation samples. Hero identity, rest transforms, scale and native validation still apply. This original reference predates the balanced jump/stretch improvements.

Ownership and distribution scope are described in the root LICENSE and `docs/THIRD_PARTY.md`; this compiled game-derived resource is not licensed as application source.

The compiled file itself is a private asset (private repository `deadlock-mod-builder-private-assets`, folder `physics-references/Vindicta-R12/`); `scripts/sync-private-assets.mjs` copies it here for a local build, and the Physics page hides the included reference when it is absent.
