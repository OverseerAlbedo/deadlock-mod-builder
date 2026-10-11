//! Preview-only cloth authoring commands. Generated sources confer no build authority.
use crate::app::{Application, Result};
use deadlock_pipeline::{
    cloth_authoring::{AnalysisResult, AuthoringReceipt, ClothAuthoringService},
    inspect::ModelPipeline,
};
use deadlock_project::Project;
impl Application {
    fn cloth_operation<T>(
        &self,
        name: &str,
        project: Project,
        run: impl FnOnce(&ClothAuthoringService<'_, '_>, &Project) -> deadlock_core::Result<T>,
    ) -> Result<T> {
        self.operation(name,|clock,cancel|{
            let settings=self.settings()?;
            let(runner,_logs)=self.create_runner(clock,cancel)?;
            let reader_root=self.native_reader_root();
            let log=|message:&str|{let _=self.append_log("info",message);};
            let model=ModelPipeline{settings:&settings,runner:&runner,cancel,clock,log:&log,reader_root:&reader_root};
            // The service owns project and distinct authoring-workspace leases.
            let value=run(&ClothAuthoringService{model:&model},&project)?;
            self.append_log("info","Cloth preview evidence saved. Native solver/render binding, original hair preservation, and in-game qualification remain required.")?;
            Ok(value)
        }).map(|(value,_)|value)
    }
    pub fn skirt_analyze(&self, project: Project) -> Result<AnalysisResult> {
        self.cloth_operation(
            "Analyze skirt bones and collisions",
            project,
            |service, p| service.analyze(p),
        )
    }
    pub fn skirt_author(&self, project: Project) -> Result<AuthoringReceipt> {
        self.cloth_operation("Create custom-bone skirt preview", project, |service, p| {
            service.create_workspace(p, false)
        })
    }
    pub fn cloth_setup(&self, project: Project) -> Result<AuthoringReceipt> {
        self.cloth_operation("Create automatic cloth preview", project, |service, p| {
            service.create_workspace(p, true)
        })
    }
}
#[cfg(feature = "desktop")]
pub mod commands {
    use super::*;
    use crate::app::AppError;
    use std::sync::Arc;
    use tauri::State;
    type AppState<'a> = State<'a, Arc<Application>>;
    async fn background<T: Send + 'static>(
        work: impl FnOnce() -> Result<T> + Send + 'static,
    ) -> Result<T> {
        tauri::async_runtime::spawn_blocking(work)
            .await
            .map_err(|e| AppError::new("internal", "Cloth authoring stopped unexpectedly", e))?
    }
    #[tauri::command]
    pub async fn skirt_analyze(state: AppState<'_>, project: Project) -> Result<AnalysisResult> {
        let app = state.inner().clone();
        background(move || app.skirt_analyze(project)).await
    }
    #[tauri::command]
    pub async fn skirt_author(state: AppState<'_>, project: Project) -> Result<AuthoringReceipt> {
        let app = state.inner().clone();
        background(move || app.skirt_author(project)).await
    }
    #[tauri::command]
    pub async fn cloth_setup(state: AppState<'_>, project: Project) -> Result<AuthoringReceipt> {
        let app = state.inner().clone();
        background(move || app.cloth_setup(project)).await
    }
}
