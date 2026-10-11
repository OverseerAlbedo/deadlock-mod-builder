import type { Customization, Project } from "../models";
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
