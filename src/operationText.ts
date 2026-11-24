/**
 * What the status bar says while an operation runs. `run` and `operate` take the
 * operation's failure title as its label (Notifications key on it: `FAILURE_TITLES`),
 * so the running text is looked up here and never shows a failure while work goes well.
 */
const RUNNING_TEXT: Record<string, string> = {
  // App
  'Build did not complete': 'Building the mod…',
  'Character asset discovery did not complete': 'Finding the character’s assets…',
  'Could not check tool paths': 'Checking tool paths…',
  'Could not choose a free VPK slot': 'Choosing a free VPK slot…',
  'Could not copy the log': 'Copying the log…',
  'Could not create a project': 'Creating a project…',
  'Could not open the project': 'Opening the project…',
  'Could not open the timing report': 'Opening the timing report…',
  'Could not save and check tools': 'Saving and checking tools…',
  'Could not save settings': 'Saving settings…',
  'Could not save the log': 'Saving the log…',
  'Could not save the project': 'Saving the project…',
  'Could not select the character': 'Selecting the character…',
  'Installation did not complete': 'Installing the mod…',
  'Material analysis did not complete': 'Analyzing materials…',
  'Model analysis did not complete': 'Analyzing the model…',
  'Original model test did not complete': 'Testing the original model…',
  'Outfit could not be loaded': 'Loading the outfit…',
  'Outfit could not be saved': 'Saving the outfit…',
  'Output folder could not be opened': 'Opening the output folder…',
  'Preview could not be loaded': 'Loading the preview…',
  'Project analysis did not complete': 'Analyzing…',
  'Source package import did not complete': 'Importing the source package…',
  'Source preparation did not complete': 'Preparing source files…',
  'Test my setup': 'Testing your setup…',
  'The copy for another hero could not be made': 'Making the copy for another hero…',
  'Tool detection did not complete': 'Detecting tools…',
  // Prepare model
  'Automatic mapping did not complete': 'Matching joints…',
  'Could not choose the source model': 'Choosing the model file…',
  'Mapping preset could not be saved': 'Saving the mapping preset…',
  'Model conversion did not complete': 'Preparing your model…',
  'Preparation review did not complete': 'Checking for problems…',
  'Prepared model could not be accepted': 'Using the prepared model…',
  'Prepared model preview did not complete': 'Loading the prepared model…',
  'Saved preparation could not be resumed': 'Opening the saved preparation…',
  'Source preview did not complete': 'Loading the model preview…',
  'Source preparation inspection did not complete': 'Checking your model…',
  'Your model could not be prepared': 'Preparing your model…',
  'Your model could not be prepared automatically': 'Preparing your model…',
  'Blender could not be opened': 'Opening Blender…',
  'The gun project could not be opened': 'Opening the gun project…',
  'The gun change could not be applied': 'Applying the gun change…',
  'The saved preparation could not be opened': 'Opening the saved preparation…',
  // Settings → Storage
  'Could not clear stored data': 'Clearing stored data…',
  'Could not measure stored data': 'Measuring stored data…',
  'Could not search for projects': 'Searching for projects…',
  // Labels that already name the work
  'Validate Blender': 'Validating Blender…',
  'Analyze skirt bones': 'Analyzing skirt bones…',
  'Create automatic cloth preview': 'Creating the cloth preview…',
  'Create custom-bone skirt preview': 'Creating the skirt preview…',
  'Export exact manual setup plan': 'Exporting the setup plan…',
  'Preview manual setup': 'Preparing the setup preview…',
  'Select model toolset': 'Selecting the model toolset…',
  'Sign in to Steam': 'Signing in to Steam…',
  'Test original hero with existing tools': 'Testing the original hero…',
  'Verify selected tool files': 'Verifying tool files…',
  'Audit compiler files and mounts': 'Checking compiler files…',
  'Find model compiler candidates': 'Finding model compilers…',
  'Scan compiler class evidence': 'Scanning the compiler…',
  'Read pinned tool manifests': 'Reading tool manifests…',
  'Download selected model tools': 'Downloading model tools…',
  'Verify downloaded tool files': 'Verifying downloaded files…',
};

/** The status bar's text for a running operation labelled `label` (its failure title). */
export function runningText(label: string | null | undefined): string {
  return (label && RUNNING_TEXT[label]) || 'Working…';
}
