use crate::process::RuntimeConfig;
use gpu_detection::model::Gpu;

use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::BTreeMap;
use gpu_detection::GpuDetectionError;

#[derive(Deserialize, Serialize)]
struct OfferTemplate {
    properties: BTreeMap<String, serde_json::Value>,
    constraints: String,
}
pub(crate) fn gpu_detection<CONFIG: RuntimeConfig>(config: &CONFIG) -> anyhow::Result<Option<Gpu>> {
    match gpu_detection::GpuDetection::init() {
        Ok(gpu_detection) => {
            let gpu = gpu_detection.detect(config.gpu_uuid())?;
            return Ok(Some(gpu));
        }
        Err(GpuDetectionError::LibloadingError(_)) => {
            return Ok(None);
        },
        Err(e) => Err(e.into())
    }
}

pub(crate) fn template<CONFIG: RuntimeConfig>(
    config: &CONFIG,
) -> anyhow::Result<Cow<'static, [u8]>> {
    let offer_template = include_bytes!("offer-template.json");
    let mut template: OfferTemplate = serde_json::from_slice(offer_template.as_ref())?;

    if let Some(gpu) = gpu_detection(config)? {
        let gpu = serde_json::value::to_value(gpu)?;
        template
            .properties
            .insert("golem.!exp.gap-35.v1.inf.gpu".into(), gpu);
    }

    Ok(Cow::Owned(serde_json::to_vec_pretty(&template)?))
}
