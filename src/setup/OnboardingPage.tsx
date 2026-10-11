import { useState } from 'react';
import type { Project, Settings, SetupStatus } from '../models';
import { basename } from '../api';
import { PageTitle, Section } from '../components/Primitives';
import { PathValue } from '../components/Paths';
import { SettingsPage } from '../components/SettingsPage';
import { setupReadiness } from './setupReadiness';

export type SetupDestination = 'settings' | 'setup' | 'project' | 'appearance' | 'physics' | 'build';
type Props = {
  project: Project; settings: Settings; busy: boolean; dirty: boolean; active: boolean;
  status: SetupStatus | null; updateSettings: (settings: Settings) => void;
  saveSettings: () => void; detect: (extraRoot?: boolean) => void; check: () => void;
  verify: () => void;
  fail: (title: string, error: unknown) => void; go: (page: SetupDestination) => void;
  openProject: () => void; chooseModel: () => void; analyze: () => void;
};
const steps = ['Welcome', 'Game and tools', 'Character and model', 'Hair and cloth', 'Build'];

export function OnboardingPage(props: Props) {
  const [step, setStep] = useState(0);
  if (!props.active) return null;
  const { project, settings, busy, go } = props;
  const physics = project.customization.physics;
  const readiness = setupReadiness(settings, props.status, props.dirty);
  const testedEnabled = project.customization.enabled === true && project.customization.model_enabled !== false && physics?.enabled === true;
  return <>
    <PageTitle title="New-user setup" subtitle="Set up the app in steps 1–2. Steps 3–5 are an optional first-project guide." action={<button disabled={busy} onClick={() => go('settings')}>Back to Settings</button>} />
    <nav className="onboarding-steps" aria-label="Setup steps">
      {steps.map((label, index) => <button key={label} disabled={busy} aria-current={step === index ? 'step' : undefined} onClick={() => setStep(index)}>{index + 1}. {label}</button>)}
    </nav>
    {step === 0 && <Section title="Start with your project">
      <p>Already have a working mod project? Open it to keep your model, materials, mappings, and cloth choices.</p>
      <div className="row"><button disabled={busy} onClick={props.openProject}>Open existing project…</button><button className="primary" disabled={busy} onClick={() => setStep(1)}>Set up programs</button></div>
      <p className="hint">You can reopen this guide any time from Settings → Open new-user setup. Visiting the guide does not reset your project or tool paths.</p>
      <dl className="summary-list"><dt>Current project</dt><dd>{project.name}</dd><dt>Replacement model</dt><dd>{project.dmx ? basename(project.dmx) : 'Not selected'}</dd></dl>
    </Section>}
    {step === 1 && <>
      <Section title="Find and save your tools">
        <p>Get any missing programs below, choose Detect tools, then <strong>Save and check tools</strong>. You can finish setup here; choosing a character and building your first mod are optional next steps.</p>
        <p className="hint">The material compiler builds textures and materials. The model compiler builds the character and its animation references. Blender is optional when you already have a prepared DMX.</p>
      </Section>
      <SettingsPage embedded verify={props.verify} modelSetup={() => go('setup')} settings={settings} update={props.updateSettings} persist={props.saveSettings} busy={busy} dirty={props.dirty} fail={props.fail} detect={props.detect} status={props.status} check={props.check} />
      <section className={`setup-next ${readiness.ready ? 'setup-ready' : ''}`} aria-label="Finish app setup">
        <div role="status"><h2>{readiness.title}</h2><p>{readiness.description}</p>{props.status && readiness.missing.length > 0 && <p className="hint">Still needed: {readiness.missing.join(', ')}.</p>}</div>
        <div className="row"><button disabled={busy || props.dirty} onClick={() => go('project')}>Done for now</button><button className="primary" disabled={busy || props.dirty} onClick={() => setStep(2)}>Continue to first project</button></div>
        <p className="hint">Done for now returns to Overview and keeps your current project. You can reopen this guide from Settings at any time.</p>
      </section>
    </>}
    {step === 2 && <Section title="Choose a character and replacement model">
      <dl className="summary-list"><dt>Original character</dt><dd><PathValue path={project.original_model}/></dd><dt>Replacement model</dt><dd><PathValue path={project.dmx}/></dd></dl>
      <div className="row"><button disabled={busy} onClick={() => go('project')}>Choose character</button><button disabled={busy} onClick={props.chooseModel}>Choose replacement model…</button><button disabled={busy} onClick={() => go('appearance')}>Review textures and materials</button></div>
      <p className="hint">Use the exact hero from the game archive. A prepared DMX goes directly into the build workflow. Blender, GLB, glTF, and FBX sources open Model preparation for conversion.</p>
    </Section>}
    {step === 3 && <Section title="Choose hair and cloth behavior">
      <p>Normal builds preserve the original character’s supported runtime physics. For the matching Vindicta R12 replacement, open Physics → Configure replacement cloth and press <strong>Use included Vindicta cloth setup</strong>.</p>
      <p>This selects the accepted balanced skirt and hair reference and enables <strong>Use tested hair and cloth physics</strong>. The normal Build mod action imports and validates it after compilation.</p>
      <dl className="summary-list"><dt>Tested reference</dt><dd>{testedEnabled ? 'Enabled in this project' : 'Not enabled'}</dd><dt>Selected file</dt><dd><PathValue path={physics?.source} empty="None"/></dd></dl>
      <button disabled={busy} onClick={() => go('physics')}>Open Physics</button>
      <p className="hint">Reference selection is not compatibility approval. The replacement must have matching bones, rest poses, and cloth bindings. The experimental cloth workspace tools create new candidates that require separate testing.</p>
    </Section>}
    {step === 4 && <Section title="Analyze, review, and build">
      <ol><li>Save your settings and project.</li><li>Choose Analyze project and resolve any mapping, material, or compatibility issues.</li><li>In Review &amp; build, choose your output slot and installation option, then press Build mod.</li><li>Open Step timings and Logs for the full operation details.</li></ol>
      <div className="row"><button disabled={busy || !project.original_model || (project.customization.model_enabled !== false && !project.dmx)} onClick={props.analyze}>Analyze project</button><button className="primary" disabled={busy} onClick={() => go('build')}>Open Review &amp; build</button></div>
      <p className="hint">The build checks current inputs and compiled outputs before publishing or installing a VPK. New source authoring and a successful preview do not establish in-game compatibility.</p>
    </Section>}
    <footer className="onboarding-footer"><button disabled={busy || step === 0} onClick={() => setStep(step - 1)}>Back</button><span className="muted">Step {step + 1} of {steps.length}{step > 1 ? ' · Optional guide' : ' · App setup'}</span>{step === steps.length - 1 ? <button disabled={busy} onClick={() => go('project')}>Done</button> : step !== 1 ? <button disabled={busy} onClick={() => setStep(step + 1)}>Next</button> : <span/>}</footer>
  </>;
}
