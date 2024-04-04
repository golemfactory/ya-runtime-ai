use crate::process::RuntimeConfig;

use gpu_detection::model::Gpu;
use gpu_detection::GpuDetectionError;
use ya_agreement_utils::OfferTemplate;

pub(crate) fn gpu_detection<CONFIG: RuntimeConfig>(config: &CONFIG) -> anyhow::Result<Option<Gpu>> {
    match gpu_detection::GpuDetection::init() {
        Ok(gpu_detection) => {
            let gpu = gpu_detection.detect(config.gpu_uuid())?;
            Ok(Some(gpu))
        }
        Err(GpuDetectionError::LibloadingError(_)) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub(crate) fn template<CONFIG: RuntimeConfig>(_config: &CONFIG) -> anyhow::Result<OfferTemplate> {
    let offer_template = include_bytes!("offer-template.json");
    let template: OfferTemplate = serde_json::from_slice(offer_template.as_ref())?;
    Ok(template)
}
