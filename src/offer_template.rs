use crate::process;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::process::{Command, Stdio};

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct OclResponse {
    error_string: String,
    platforms: Vec<OclPlatform>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct OclPlatform {
    devices: Vec<BTreeMap<String, serde_json::Value>>,
}

fn extract_device_info(device_info: BTreeMap<String, serde_json::Value>) -> Option<(String, u64)> {
    match (
        device_info.get("_CL_DEVICE_NAME"),
        device_info.get("_CL_DEVICE_GLOBAL_MEM_SIZE"),
    ) {
        (Some(serde_json::Value::String(name)), Some(serde_json::Value::Number(mem))) => {
            Some((name.clone(), mem.as_u64().unwrap_or_default()))
        }
        _ => None,
    }
}

pub fn parse_devices_info() -> anyhow::Result<Vec<(String, u64)>> {
    if let Ok(exe) = process::find_exe("device_detection.exe") {
        let output = Command::new(exe)
            .arg("ocl")
            .stdin(Stdio::null())
            .stderr(Stdio::inherit())
            .stdout(Stdio::piped())
            .output()?;
        let response: OclResponse = serde_json::from_slice(output.stdout.as_ref())?;
        if !response.error_string.is_empty() {
            eprintln!("detection error: {}", response.error_string);
        }
        Ok(response
            .platforms
            .into_iter()
            .flat_map(|d| d.devices.into_iter().filter_map(extract_device_info))
            .collect())
    } else {
        eprintln!("not found device detection");
        Ok(Vec::new())
    }
}

#[derive(Deserialize, Serialize)]
struct OfferTemplate {
    properties: BTreeMap<String, serde_json::Value>,
    constraints: String,
}

pub fn template(runtime_name: String) -> anyhow::Result<Cow<'static, [u8]>> {
    let offer_template = include_bytes!("offer-template.json");
    let mut template: OfferTemplate = serde_json::from_slice(offer_template.as_ref())?;
    template.properties.insert(
        "golem.inf.ai.runtime".to_string(),
        serde_json::Value::String(runtime_name),
    );
    let devices = parse_devices_info()?;
    if devices.is_empty() {
        return Ok(Cow::Owned(serde_json::to_vec_pretty(&template)?));
    }
    template.properties.insert(
        "golem.inf.gpu.card".to_string(),
        serde_json::Value::Array(
            devices
                .iter()
                .map(|(name, _)| serde_json::Value::from(name.as_str()))
                .collect(),
        ),
    );
    template.properties.insert(
        "golem.inf.gpu.mem".to_string(),
        serde_json::Value::Array(devices.iter().map(|&(_, mem)| mem.into()).collect()),
    );
    Ok(Cow::Owned(serde_json::to_vec_pretty(&template)?))
}
