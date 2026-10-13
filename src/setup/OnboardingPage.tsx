import { useState } from 'react';
import type { Project, Settings, SetupStatus } from '../models';
import { basename } from '../api';
import { PageTitle, Section } from '../components/Primitives';
import { PathValue } from '../components/Paths';
import { SettingsPage } from '../components/SettingsPage';
import { PrepIcon } from '../preparation/PreparationChrome';
import { setupReadiness } from './setupReadiness';
import '../preparation/preparation.css';

export type SetupDestination = 'settings' | 'setup' | 'project' | 'appearance' | 'physics' | 'build';
type Props = {
  project: Project; settings: Settings; busy: boolean; dirty: boolean; active: boolean;
  status: SetupStatus | null; updateSettings: (settings: Settings) => void;
  saveSettings: () => void; detect: (extraRoot?: boolean) => void; check: () => void;
  verify: () => void;
  fail: (title: string, error: unknown) => void; go: (page: SetupDestination) => void;
  openProject: () => void; chooseModel: () => void; analyze: () => void;
};
/** Step labels (the test harness and screen readers use "N. Label"). */
const steps: [string, string][] = [
  ['Welcome', 'Your project'],
  ['Game and tools', 'Find and save programs'],
  ['Character and model', 'Your first mod'],
  ['Hair and cloth', 'Optional movement setup'],
  ['Build', 'Build and install'],
];

/** The five main steps of the app, as a short map for new users. */
function FlowMap() {
  const items = [['Character & model', 'Pick the hero and your model file; preparation starts by itself.'], ['Prepare model', 'Usually finishes on its own; stops only where a choice is needed.'], ['Appearance & weapons', 'Check the gun fit and the materials.'], ['Physics & preview', 'Switch movement on or off and preview the character.'], ['Build & install', 'Analyze, build and install, then test in Deadlock.']];
  return <ol className="setup-flow">{items.map(([title, text], index) => <li key={title}><b>{index + 1}</b><div><strong>{title}</strong><span>{text}</span></div></li>)}</ol>;
}

export function OnboardingPage(props: Props) {
  const [step, setStep] = useState(0);
  if (!props.active) return null;
  const { project, settings, busy, go } = props;
  const physics = project.customization.physics;
  const readiness = setupReadiness(settings, props.status, props.dirty);
  const testedEnabled = project.customization.enabled === true && project.customization.model_enabled !== false && physics?.enabled === true;
  const done = (index: number) => (index === 1 && readiness.ready) || (index === 2 && Boolean(project.original_model && (project.dmx || project.preparation_source)));
  return <div className="prep-workspace setup-workspace">
    <PageTitle eyebrow="Getting started" title="New-user setup" subtitle="Steps 1–2 set up the app once. Steps 3–5 walk you through your first mod; you can skip them and use the sidebar." action={<button className="prep-secondary" disabled={busy} onClick={() => go('settings')}>Back to Settings</button>} />
    <nav className="prep-steps" aria-label="Setup steps">
      {steps.map(([label, hint], index) => <span key={label} className="prep-step-slot">{index > 0 && <i/>}<button type="button" disabled={busy} aria-label={`${index + 1}. ${label}`} aria-current={step === index ? 'step' : undefined} onClick={() => setStep(index)}><b>{done(index) ? <PrepIcon name="check" size={14}/> : index + 1}</b><span>{label}<small>{hint}</small></span></button></span>)}
    </nav>
    {step === 0 && <Section title="Start with your project" className="prep-card">
      <p className="prep-section-intro">Already have a mod project? Open it to keep your model, materials, mappings and movement choices. Otherwise continue with the project you have open: this guide never resets it or your tool paths.</p>
      <dl className="summary-list"><dt>Current project</dt><dd>{project.name}</dd><dt>Replacement model</dt><dd>{project.dmx ? basename(project.dmx) : project.preparation_source ? basename(project.preparation_source) : 'Not selected'}</dd></dl>
      <div className="row"><button className="prep-primary" disabled={busy} onClick={() => setStep(1)}>Set up programs<PrepIcon name="arrow" size={18}/></button><button className="prep-secondary" disabled={busy} onClick={props.openProject}>Open existing project…</button></div>
      <p className="hint">You can reopen this guide any time from Settings → Open new-user setup.</p>
    </Section>}
    {step === 1 && <>
      <Section title="Find and save your tools" className="prep-card">
        <p className="prep-section-intro">Get any missing programs below, click <strong>Detect tools</strong>, then <strong>Save and check tools</strong>. App setup is finished once the check passes; your first mod is an optional next step.</p>
        <p className="hint">The material compiler builds textures and materials; the model compiler builds the character and its animation references. Blender is only needed to prepare Blender, FBX or glTF models (a prepared DMX does not need it).</p>
      </Section>
      <div className="prep-card setup-embedded-settings"><SettingsPage embedded verify={props.verify} modelSetup={() => go('setup')} settings={settings} update={props.updateSettings} persist={props.saveSettings} busy={busy} dirty={props.dirty} fail={props.fail} detect={props.detect} status={props.status} check={props.check} /></div>
      <section className={`prep-card setup-next ${readiness.ready ? 'setup-ready' : ''}`} aria-label="Finish app setup">
        <div role="status" className="setup-next-status"><span className="prep-status-icon"><PrepIcon name={readiness.ready ? 'check' : 'review'} size={22}/></span><div><h2>{readiness.title}</h2><p>{readiness.description}</p>{props.status && readiness.missing.length > 0 && <p className="hint">Still needed: {readiness.missing.join(', ')}.</p>}</div></div>
        <div className="row"><button className="prep-primary" disabled={busy || props.dirty} onClick={() => setStep(2)}>Continue to first project<PrepIcon name="arrow" size={18}/></button><button className="prep-secondary" disabled={busy || props.dirty} onClick={() => go('project')}>Done for now</button></div>
        <p className="hint">Done for now returns to step 1 of the mod flow and keeps your current project.</p>
      </section>
    </>}
    {step === 2 && <Section title="Choose a character and replacement model" className="prep-card">
      <p className="prep-section-intro">A mod is made in five steps, shown in the sidebar. The first one picks the hero your mod replaces and your model file; everything after that is prepared automatically where it can be.</p>
      <FlowMap/>
      <dl className="summary-list"><dt>Original character</dt><dd><PathValue path={project.original_model}/></dd><dt>Replacement model</dt><dd><PathValue path={project.dmx || project.preparation_source}/></dd></dl>
      <div className="row"><button className="prep-primary" disabled={busy} onClick={() => go('project')}>Choose character<PrepIcon name="arrow" size={18}/></button><button className="prep-secondary" disabled={busy} onClick={props.chooseModel}>Choose replacement model…</button><button className="prep-secondary" disabled={busy} onClick={() => go('appearance')}>Review textures and materials</button></div>
      <p className="hint">Use the exact hero from the game archive. Blender, GLB, glTF and FBX files are prepared for the hero automatically; a prepared DMX goes straight to the build.</p>
    </Section>}
    {step === 3 && <Section title="Choose hair and cloth behavior" className="prep-card">
      <p className="prep-section-intro">Automatic motion gives hair, skirts, cloth, tails and gun charms movement after preparation; switch parts on or off in <strong>Physics & preview</strong>. The original character's own runtime physics is preserved in normal builds.</p>
      <p>For the matching Vindicta R12 replacement, open Physics → Configure replacement cloth and press <strong>Use included Vindicta cloth setup</strong>. This selects the accepted skirt and hair reference and enables <strong>Use tested hair and cloth physics</strong>; Build imports and validates it after compilation.</p>
      <dl className="summary-list"><dt>Tested reference</dt><dd>{testedEnabled ? 'Enabled in this project' : 'Not enabled'}</dd><dt>Selected file</dt><dd><PathValue path={physics?.source} empty="None"/></dd></dl>
      <div className="row"><button className="prep-primary" disabled={busy} onClick={() => go('physics')}>Open Physics<PrepIcon name="arrow" size={18}/></button></div>
      <p className="hint">A reference is not a compatibility approval: the replacement must have matching bones, rest poses and cloth bindings. Movement amounts are checked in game.</p>
    </Section>}
    {step === 4 && <Section title="Analyze, review, and build" className="prep-card">
      <ol className="setup-list"><li>Save your settings and project.</li><li>Click <strong>Analyze project</strong> and resolve any mapping, material or compatibility issue it lists.</li><li>In <strong>Build &amp; install</strong>, pick a free VPK slot and whether to install, then click <strong>Build mod</strong>.</li><li>Open Step timings and Logs for the full details of a run.</li></ol>
      <div className="row"><button className="prep-primary" disabled={busy} onClick={() => go('build')}>Open Build &amp; install<PrepIcon name="arrow" size={18}/></button><button className="prep-secondary" disabled={busy || !project.original_model || (project.customization.model_enabled !== false && !project.dmx)} onClick={props.analyze}>Analyze project</button></div>
      <p className="hint">The build checks the current inputs and the compiled output before packaging or installing. A successful preview does not establish in-game compatibility; test in Deadlock.</p>
    </Section>}
    <footer className="onboarding-footer"><button className="prep-secondary" disabled={busy || step === 0} onClick={() => setStep(step - 1)}>Back</button><span className="muted">Step {step + 1} of {steps.length}{step > 1 ? ' · Optional guide' : ' · App setup'}</span>{step === steps.length - 1 ? <button className="prep-primary" disabled={busy} onClick={() => go('project')}>Done</button> : step !== 1 ? <button className="prep-primary" disabled={busy} onClick={() => setStep(step + 1)}>Next<PrepIcon name="arrow" size={18}/></button> : <span/>}</footer>
  </div>;
}
