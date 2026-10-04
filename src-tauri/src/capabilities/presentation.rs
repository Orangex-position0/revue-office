pub mod error;
pub mod model;
pub mod ports;
pub mod progress;
pub mod service;

pub use error::PresentationCapabilityError;
pub use model::{
    PresentationElement, PresentationExport, PresentationGenerateRequest, PresentationOutput,
    PresentationPlan, PresentationPlanRequest, PresentationProject, PresentationProjectUpdate,
    PresentationSlide, PresentationSlidePlan,
};
pub use ports::{
    PresentationExportError, PresentationExporter, PresentationPlanner, PresentationPlannerError,
    PresentationStore, PresentationStoreError,
};
pub use progress::{PresentationProgress, PresentationProgressError, PresentationProgressSink};
pub use service::PresentationCapability;
