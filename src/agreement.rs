use std::fs::File;
use std::io::BufReader;
use std::path::Path;

pub struct AgreementDesc {
    pub counters: Vec<String>,
}

impl AgreementDesc {
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let agreement: serde_json::Value =
            serde_json::from_reader(BufReader::new(File::open(path)?))?;
        let counters =
            if let Some(v) = agreement.pointer("/offer/properties/golem/com/usage/vector") {
                serde_json::from_value(v.clone())?
            } else {
                anyhow::bail!("invalid agreement. missing usage counters")
            };

        Ok(AgreementDesc { counters })
    }

    pub fn resolve_counter(&self, counter_id: &str) -> Option<usize> {
        self.counters
            .iter()
            .enumerate()
            .find_map(|(idx, name)| if name == counter_id { Some(idx) } else { None })
    }

    pub fn clean_usage_vector(&self) -> Vec<f64> {
        self.counters.iter().map(|_| 0f64).collect()
    }
}
