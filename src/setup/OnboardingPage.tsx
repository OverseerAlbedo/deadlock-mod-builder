// i18n-area: guide
import { useEffect, useState } from 'react';
import type { FieldCheck, Project, Settings, SetupStatus, SetupTest } from '../models';
import { basename } from '../api';
import { PageTitle, Section } from '../components/Primitives';
import { PathValue } from '../components/Paths';
import { SettingsPage } from '../components/SettingsPage';
import { PrepIcon } from '../preparation/PreparationChrome';
import { setupReadiness } from './setupReadiness';
import { m, useT, type Language } from '../i18n';
import { T } from '../i18n/T';
import { sentences } from '../build/words';
import { LanguageCard } from '../i18n/LanguagePicker';
import '../preparation/preparation.css';

export type SetupDestination = 'settings' | 'setup' | 'project' | 'appearance' | 'physics' | 'build';
type Props = {
  project: Project; settings: Settings; busy: boolean; dirty: boolean; active: boolean;
  status: SetupStatus | null; updateSettings: (settings: Settings) => void;
  saveSettings: () => void; detect: (extraRoot?: boolean) => void; check: () => void;
  verify: () => void;
  /** Test my setup (compiles one test material and probes the model compiler). */
  testSetup?: () => Promise<SetupTest | undefined>;
  fail: (title: string, error: unknown) => void; go: (page: SetupDestination) => void;
  openProject: () => void; chooseModel: () => void; analyze: () => void;
  /** Step to show when the guide opens by itself (1 = tools, when a path is wrong). */
  startStep?: number;
  /** Done on the last step: the guide leaves the sidebar (Settings reopens it). */
  finish?: () => void;
  /** Expert shows the compiler backend and format choices in the embedded Settings. */
  expert?: boolean;
  /** The live field checks of the draft settings, when the app runs them (the embedded Settings then does not). */
  fields?: FieldCheck[] | null;
  /** Applies and saves the interface language (the language card on the first step is shown when given). */
  chooseLanguage?: (language: Language) => void;
};
/** Step labels (the test harness and screen readers use "N. Label"; the label is translated where it is drawn). */
const steps: [string, string][] = [
  [m('Welcome'), m('Your project')],
  [m('Game and tools'), m('Find and save programs')],
  [m('Character and model'), m('Your first mod')],
  [m('Hair and cloth'), m('Optional movement setup')],
  [m('Build'), m('Build and install')],
];
/** The five main steps of the app. */
const FLOW: [string, string][] = [
  [m('Character & model'), m('Pick the hero and your model file; preparation starts by itself.')],
  [m('Prepare model'), m('Usually finishes on its own; stops only where a choice is needed.')],
  [m('Appearance & weapons'), m('Check the gun fit and the materials.')],
  [m('Physics & preview'), m('Switch movement on or off and preview the character.')],
  [m('Build & install'), m('Analyze, build and install, then test in Deadlock.')],
];

/** The five main steps of the app, as a short map for new users. */
function FlowMap() {
  const t = useT();
  return <ol className="setup-flow">{FLOW.map(([title, text], index) => <li key={title}><b>{index + 1}</b><div><strong>{t(title)}</strong><span>{t(text)}</span></div></li>)}</ol>;
}

export function OnboardingPage(props: Props) {
  const t = useT();
  const [step, setStep] = useState(props.startStep ?? 0);
  useEffect(() => { if (props.startStep !== undefined) setStep(props.startStep); }, [props.startStep]);
  if (!props.active) return null;
  const { project, settings, busy, go } = props;
  const readiness = setupReadiness(settings, props.status, props.dirty);
  const done = (index: number) => (index === 1 && readiness.ready) || (index === 2 && Boolean(project.original_model && (project.dmx || project.preparation_source)));
  return <div className="prep-workspace setup-workspace">
    <PageTitle eyebrow={t('Getting started')} title={t('New-user setup')} subtitle={t('Steps 1–2 set up the app once. Steps 3–5 walk you through your first mod; you can skip them and use the sidebar.')} action={<button className="prep-secondary" disabled={busy} onClick={() => go('settings')}>{t('Back to Settings')}</button>} />
    <nav className="prep-steps" aria-label={t('Setup steps')}>
      {steps.map(([label, hint], index) => <span key={label} className="prep-step-slot">{index > 0 && <i/>}<button type="button" disabled={busy} aria-label={`${index + 1}. ${t(label)}`} aria-current={step === index ? 'step' : undefined} onClick={() => setStep(index)}><b>{done(index) ? <PrepIcon name="check" size={14}/> : index + 1}</b><span>{t(label)}<small>{t(hint)}</small></span></button></span>)}
    </nav>
    {step === 0 && props.chooseLanguage && <LanguageCard choose={props.chooseLanguage} disabled={busy}/>}
    {step === 0 && <Section title={t('Start with your project')} className="prep-card">
      <p className="prep-section-intro">{t('Already have a mod project? Open it to keep your model, materials, mappings and movement choices. Otherwise continue with the project you have open: this guide never resets it or your tool paths.')}</p>
      <dl className="summary-list"><dt>{t('Current project')}</dt><dd>{project.name}</dd><dt>{t('Replacement model')}</dt><dd>{project.dmx ? basename(project.dmx) : project.preparation_source ? basename(project.preparation_source) : t('Not selected')}</dd></dl>
      <div className="row"><button className="prep-primary" disabled={busy} onClick={() => setStep(1)}>{t('Set up programs')}<PrepIcon name="arrow" size={18}/></button><button className="prep-secondary" disabled={busy} onClick={props.openProject}>{t('Open existing project…')}</button></div>
      <p className="hint">{t('This guide stays in the sidebar under Getting started until you press Done on its last step; Settings → Open new-user setup brings it back.')}</p>
    </Section>}
    {step === 1 && <>
      <Section title={t('Find and save your tools')} className="prep-card">
        <p className="prep-section-intro">{t('Do these in order. Every path field below checks itself as you go: green is right, red says what is wrong and offers the right path when it can find it.')}</p>
        <ol className="setup-list setup-order">
          <li><T text="<strong>Deadlock</strong> installed through Steam."/></li>
          <li><T text="<strong>Blender 5.2 LTS</strong> installed with its installer."/></li>
          <li><T text="<strong>Source 2 Viewer CLI</strong>: download <code>cli-windows-x64.zip</code>, right-click → Extract All, for example to <code>{folder}</code>." values={{folder: String.raw`C:\Tools\Source2Viewer`}}/></li>
          <li><T text="<strong>Reduced CSDK 13</strong>: extract the whole ZIP to a short folder outside OneDrive, such as <code>{folder}</code>. The right folder directly contains <code>game</code> and <code>content</code> (the ZIP puts it inside a second <code>Reduced_CSDK_13</code> folder; Detect tools finds it)." values={{folder: String.raw`C:\CSDK13`}}/></li>
          <li><T text="Click <strong>Detect tools</strong>. It fills in what it finds and corrects common mix-ups."/></li>
          <li><T text="Click <strong>Save and check tools</strong>, then <strong>Test my setup</strong>. Setup is finished when nothing is red."/></li>
        </ol>
        <p className="hint">{t('Reduced CSDK 13 compiles the textures, materials and the character. Older guides also list CSWIN64, Reduced CSDK 12, DepotDownloader or Counter-Strike 2 tools: none of them is needed with Reduced CSDK 13.')}</p>
      </Section>
      <div className="prep-card setup-embedded-settings"><SettingsPage embedded expert={props.expert ?? true} verify={props.verify} testSetup={props.testSetup} go={page => go(page as Parameters<typeof go>[0])} modelSetup={() => go('setup')} settings={settings} update={props.updateSettings} persist={props.saveSettings} busy={busy} dirty={props.dirty} fail={props.fail} detect={props.detect} status={props.status} check={props.check} fields={props.fields} /></div>
      <section className={`prep-card setup-next ${readiness.ready ? 'setup-ready' : ''}`} aria-label={t('Finish app setup')}>
        <div role="status" className="setup-next-status"><span className="prep-status-icon"><PrepIcon name={readiness.ready ? 'check' : 'review'} size={22}/></span><div><h2>{t(readiness.title)}</h2><p>{sentences(t(readiness.sentence), readiness.blenderNote ? t(readiness.blenderNote) : '')}</p>{props.status && readiness.missing.length > 0 && <p className="hint">{t('Still needed: {items}.', {items: readiness.missing.join(', ')})}</p>}</div></div>
        <div className="row"><button className="prep-primary" disabled={busy || props.dirty} onClick={() => setStep(2)}>{t('Continue to first project')}<PrepIcon name="arrow" size={18}/></button><button className="prep-secondary" disabled={busy || props.dirty} onClick={() => go('project')}>{t('Done for now')}</button></div>
        <p className="hint">{t('Done for now returns to step 1 of the mod flow and keeps your current project.')}</p>
      </section>
    </>}
    {step === 2 && <Section title={t('Choose a character and replacement model')} className="prep-card">
      <p className="prep-section-intro">{t('A mod is made in five steps, shown in the sidebar. The first one picks the hero your mod replaces and your model file; everything after that is prepared automatically where it can be.')}</p>
      <FlowMap/>
      <dl className="summary-list"><dt>{t('Original character')}</dt><dd><PathValue path={project.original_model}/></dd><dt>{t('Replacement model')}</dt><dd><PathValue path={project.dmx || project.preparation_source}/></dd></dl>
      <div className="row"><button className="prep-primary" disabled={busy} onClick={() => go('project')}>{t('Choose character')}<PrepIcon name="arrow" size={18}/></button><button className="prep-secondary" disabled={busy} onClick={props.chooseModel}>{t('Choose replacement model…')}</button><button className="prep-secondary" disabled={busy} onClick={() => go('appearance')}>{t('Review textures and materials')}</button></div>
      <p className="hint">{t('Use the exact hero from the game archive. Blender, GLB, glTF, FBX and MMD (.pmx, .pmd) files are prepared for the hero automatically (MMD files need the MMD Tools add-on in Blender); a prepared DMX goes straight to the build.')}</p>
    </Section>}
    {step === 3 && <Section title={t('Hair, cloth and movement')} className="prep-card">
      <p className="prep-section-intro">{t('Hair, skirts, cloth, tails, ears and gun charms get movement by themselves when the model is prepared. Nothing here is required for a first build.')}</p>
      <ul className="setup-list"><li><T text="<strong>Physics & preview</strong> lists every moving part: switch parts on or off, and set breast or butt motion if you want it."/></li><li><T text="<strong>Fine-tune movement</strong> sets how stiff, damped, heavy and wide each kind of part swings, and the <strong>Movement</strong> preview plays the hero standing, running and jumping with your parts moving."/></li><li>{t("The hero's own cloth from the game is kept in every build.")}</li></ul>
      <div className="row"><button className="prep-primary" disabled={busy} onClick={() => go('physics')}>{t('Open Physics')}<PrepIcon name="arrow" size={18}/></button></div>
      <p className="hint">{t("The preview approximates the game's cloth: check the real movement in Deadlock after the first build.")}</p>
    </Section>}
    {step === 4 && <Section title={t('Analyze, review, and build')} className="prep-card">
      <ol className="setup-list"><li>{t('Save your settings and project.')}</li><li><T text="Click <strong>Analyze project</strong> and resolve any mapping, material or compatibility issue it lists."/></li><li><T text="In <strong>Build & install</strong>, pick a free VPK slot and whether to install, then click <strong>Build mod</strong>."/></li><li>{t('Open Step timings and Logs for the full details of a run.')}</li></ol>
      <div className="row"><button className="prep-primary" disabled={busy} onClick={() => go('build')}>{t('Open Build & install')}<PrepIcon name="arrow" size={18}/></button><button className="prep-secondary" disabled={busy || !project.original_model || (project.customization.model_enabled !== false && !project.dmx)} onClick={props.analyze}>{t('Analyze project')}</button></div>
      <p className="hint">{t('The build checks the current inputs and the compiled output before packaging or installing.')}</p>
    </Section>}
    <footer className="onboarding-footer"><button className="prep-secondary" disabled={busy || step === 0} onClick={() => setStep(step - 1)}>{t('Back')}</button><span className="muted">{step > 1 ? t('Step {step} of {count} · Optional guide', {step: step + 1, count: steps.length}) : t('Step {step} of {count} · App setup', {step: step + 1, count: steps.length})}</span>{step === steps.length - 1 ? <button className="prep-primary" disabled={busy} onClick={() => props.finish ? props.finish() : go('project')}>{t('Done')}</button> : step !== 1 ? <button className="prep-primary" disabled={busy} onClick={() => setStep(step + 1)}>{t('Next')}<PrepIcon name="arrow" size={18}/></button> : <span/>}</footer>
  </div>;
}
