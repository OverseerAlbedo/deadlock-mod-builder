import type { Customization, Project, SecondaryMotionChoices } from "../models";
/** A change of the automatic motion choices. It switches a tested cloth reference off only when
 * one is on: a reference that is already off, or was never chosen, is left exactly as it is. */
export function changeSecondaryMotion(project: Project, values: Partial<SecondaryMotionChoices>): Project {
  const physics = project.customization.physics;
  return changeCustomization(project, {
    secondary_motion: { ...project.customization.secondary_motion, ...values },
    ...(physics?.enabled ? { physics: { ...physics, enabled: false } } : {}),
  });
}
export function changeCustomization(
  project: Project,
  changes: Partial<Customization>,
  activate = true,
): Project {
  return {
    ...project,
    customization: {
      ...project.customization,
      ...changes,
      ...(activate ? { enabled: true } : {}),
      schema: project.customization.schema ?? 1,
    },
  };
}
