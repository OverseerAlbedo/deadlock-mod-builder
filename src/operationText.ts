// i18n-area: shell
/**
 * What the status bar says while an operation runs. `run` and `operate` take the
 * operation's failure title as its label (Notifications key on it: `FAILURE_TITLES`),
 * so the running text is looked up here and never shows a failure while work goes well.
 * The running texts are English (marked for the checker) and translated where the status bar shows them.
 */
import { m } from './i18n';

const RUNNING_TEXT: Record<string, string> = {
  // App
  'Build did not complete': m('Building the mod…'),
  'Character asset discovery did not complete': m('Finding the character’s assets…'),
  'Could not check tool paths': m('Checking tool paths…'),
  'Could not choose a free VPK slot': m('Choosing a free VPK slot…'),
  'Could not copy the log': m('Copying the log…'),
  'Could not create a project': m('Creating a project…'),
  'Could not open the project': m('Opening the project…'),
  'Could not open the timing report': m('Opening the timing report…'),
  'Could not save and check tools': m('Saving and checking tools…'),
  'Could not save settings': m('Saving settings…'),
  'Could not save the log': m('Saving the log…'),
  'Could not save the project': m('Saving the project…'),
  'Could not select the character': m('Selecting the character…'),
  'Installation did not complete': m('Installing the mod…'),
  'Material analysis did not complete': m('Analyzing materials…'),
  'Model analysis did not complete': m('Analyzing the model…'),
  'Original model test did not complete': m('Testing the original model…'),
  'Outfit could not be loaded': m('Loading the outfit…'),
  'Outfit could not be saved': m('Saving the outfit…'),
  'Output folder could not be opened': m('Opening the output folder…'),
  'Preview could not be loaded': m('Loading the preview…'),
  'Project analysis did not complete': m('Analyzing…'),
  'Source package import did not complete': m('Importing the source package…'),
  'Source preparation did not complete': m('Preparing source files…'),
  'Test my setup': m('Testing your setup…'),
  'The copy for another hero could not be made': m('Making the copy for another hero…'),
  'Tool detection did not complete': m('Detecting tools…'),
  // Prepare model
  'Automatic mapping did not complete': m('Matching joints…'),
  'Could not choose the source model': m('Choosing the model file…'),
  'Mapping preset could not be saved': m('Saving the mapping preset…'),
  'Model conversion did not complete': m('Preparing your model…'),
  'Preparation review did not complete': m('Checking for problems…'),
  'Prepared model could not be accepted': m('Using the prepared model…'),
  'Prepared model preview did not complete': m('Loading the prepared model…'),
  'Saved preparation could not be resumed': m('Opening the saved preparation…'),
  'Source preview did not complete': m('Loading the model preview…'),
  'Source preparation inspection did not complete': m('Checking your model…'),
  'Your model could not be prepared': m('Preparing your model…'),
  'Your model could not be prepared automatically': m('Preparing your model…'),
  'Blender could not be opened': m('Opening Blender…'),
  'The gun project could not be opened': m('Opening the gun project…'),
  'The gun change could not be applied': m('Applying the gun change…'),
  'The saved preparation could not be opened': m('Opening the saved preparation…'),
  // Settings → Storage
  'Could not clear stored data': m('Clearing stored data…'),
  'Could not measure stored data': m('Measuring stored data…'),
  'Could not search for projects': m('Searching for projects…'),
  // Labels that already name the work
  'Validate Blender': m('Validating Blender…'),
  'Analyze skirt bones': m('Analyzing skirt bones…'),
  'Create automatic cloth preview': m('Creating the cloth preview…'),
  'Create custom-bone skirt preview': m('Creating the skirt preview…'),
  'Export exact manual setup plan': m('Exporting the setup plan…'),
  'Preview manual setup': m('Preparing the setup preview…'),
  'Select model toolset': m('Selecting the model toolset…'),
  'Sign in to Steam': m('Signing in to Steam…'),
  'Test original hero with existing tools': m('Testing the original hero…'),
  'Verify selected tool files': m('Verifying tool files…'),
  'Audit compiler files and mounts': m('Checking compiler files…'),
  'Find model compiler candidates': m('Finding model compilers…'),
  'Scan compiler class evidence': m('Scanning the compiler…'),
  'Read pinned tool manifests': m('Reading tool manifests…'),
  'Download selected model tools': m('Downloading model tools…'),
  'Verify downloaded tool files': m('Verifying downloaded files…'),
};

/** The status bar's text for a running operation labelled `label` (its failure title). */
export function runningText(label: string | null | undefined): string {
  return (label && RUNNING_TEXT[label]) || m('Working…');
}
