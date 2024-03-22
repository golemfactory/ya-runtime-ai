use crate::process::RuntimeConfig;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::BTreeMap;

#[derive(Deserialize, Serialize)]
struct OfferTemplate {
    properties: BTreeMap<String, serde_json::Value>,
    constraints: String,
}

pub fn template<CONFIG: RuntimeConfig + 'static>(
    config: &CONFIG,
) -> anyhow::Result<Cow<'static, [u8]>> {
    let offer_template = include_bytes!("offer-template.json");
    let mut template: OfferTemplate = serde_json::from_slice(offer_template.as_ref())?;

    if CONFIG::uses_gpu() {
        let gpu_info = gpu_info::GpuInfo::init()?;
        let gpu = gpu_info.info(config.gpu_uuid())?;
        let gpu = serde_json::value::to_value(gpu)?;
        template
            .properties
            .insert("golem.!exp.gap-35.v1.inf.gpu".into(), gpu);
    }

    Ok(Cow::Owned(serde_json::to_vec_pretty(&template)?))
}
