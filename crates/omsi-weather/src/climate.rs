use std::path::Path;

use omsi_cfg::{CfgFile, CfgReader};

/// Monthly climate statistics supplied by a map's `openomsi_climate.cfg`.
#[derive(Debug, Clone)]
pub struct ClimateStats {
    /// Mean temperatures in degrees Celsius, January through December.
    pub temp: [f32; 12],
    /// Mean precipitation in millimetres, January through December.
    pub precip: [f32; 12],
}

impl ClimateStats {
    /// Load climate data next to the map's `global.cfg`, if the file exists and is valid.
    pub fn for_map(map: &str, root: &Path) -> Option<Self> {
        let map_dir = Path::new(map).parent()?;
        let relative_dir = map_dir.strip_prefix(root).unwrap_or(map_dir);
        let relative_cfg = relative_dir.join("openomsi_climate.cfg");
        let path = omsi_cfg::find_in_roots(&relative_cfg.to_string_lossy())
            .map(|(_, path)| path)
            .unwrap_or_else(|| {
                if map_dir.is_absolute() {
                    map_dir.join("openomsi_climate.cfg")
                } else {
                    root.join(map_dir).join("openomsi_climate.cfg")
                }
            });
        if !path.exists() {
            return None;
        }
        match CfgFile::read(&path) {
            Ok(file) => Self::from_cfg(&file),
            Err(error) => {
                log::warn!("climate config {}: {error}", path.display());
                None
            }
        }
    }

    fn read_monthly_values(reader: &mut CfgReader<'_>) -> Option<Vec<f32>> {
        reader
            .rest_of_block()
            .iter()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .map(str::parse::<f32>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| log::warn!("openomsi_climate.cfg: invalid number: {error}"))
            .ok()
    }

    /// Parse the `[temperature]` and optional `[precipitation]` monthly blocks.
    pub fn from_cfg(file: &CfgFile) -> Option<Self> {
        let mut reader = file.reader();
        let (mut temperatures, mut precipitation) = (None, None);
        while let Some(keyword) = reader.next_keyword() {
            match keyword.as_str() {
                "temperature" => temperatures = Self::read_monthly_values(&mut reader),
                "precipitation" => precipitation = Self::read_monthly_values(&mut reader),
                _ => log::warn!("openomsi_climate.cfg: unknown section [{keyword}]"),
            }
        }

        let Some(temperatures) = temperatures else {
            log::warn!(
                "openomsi_climate.cfg: missing or invalid [temperature] block; using default climate"
            );
            return None;
        };
        let precipitation = precipitation.unwrap_or_else(|| vec![0.0; 12]);
        let temperatures: [f32; 12] = match temperatures.try_into() {
            Ok(values) => values,
            Err(values) => {
                log::warn!(
                    "openomsi_climate.cfg: [temperature] has {} values; expected 12; using default climate",
                    values.len()
                );
                return None;
            }
        };
        let precipitation: [f32; 12] = match precipitation.try_into() {
            Ok(values) => values,
            Err(values) => {
                log::warn!(
                    "openomsi_climate.cfg: [precipitation] has {} values; expected 12; using default climate",
                    values.len()
                );
                return None;
            }
        };

        if !temperatures
            .iter()
            .all(|value| (-100.0..=100.0).contains(value))
            || !precipitation
                .iter()
                .all(|value| (0.0..=5000.0).contains(value))
        {
            log::warn!(
                "openomsi_climate.cfg: values must be finite and within -100..=100 °C or 0..=5000 mm; using default climate"
            );
            return None;
        }

        Some(Self {
            temp: temperatures,
            precip: precipitation,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(text: &str) -> CfgFile {
        CfgFile::from_str("openomsi_climate.cfg", text)
    }

    #[test]
    fn reads_temperature_and_optional_precipitation() {
        let stats = ClimateStats::from_cfg(&config(
            "[temperature]\n-7\n-6\n-1\n6\n12\n16\n19\n16\n11\n5\n-1\n-5\n[precipitation]\n44\n38\n34\n34\n64\n77\n76\n75\n58\n67\n50\n44\n",
        ))
        .unwrap();
        assert_eq!(stats.temp[0], -7.0);
        assert_eq!(stats.temp[11], -5.0);
        assert_eq!(stats.precip[0], 44.0);
        assert_eq!(stats.precip[11], 44.0);

        let stats = ClimateStats::from_cfg(&config(
            "[temperature]\n1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12\n",
        ))
        .unwrap();
        assert_eq!(stats.precip, [0.0; 12]);
    }

    #[test]
    fn ignores_blank_lines_and_rejects_invalid_temperature_blocks() {
        let stats = ClimateStats::from_cfg(&config(
            "[temperature]\n1\n2\n3\n4\n5\n6\n\n7\n8\n9\n10\n11\n12\n\n",
        ));
        assert!(stats.is_some());

        assert!(ClimateStats::from_cfg(&config("[temperatures]\n1\n")).is_none());
        assert!(ClimateStats::from_cfg(&config("[temperature]\n1\n2\n")).is_none());
        assert!(
            ClimateStats::from_cfg(&config(
                "[temperature]\n1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\nNaN\n",
            ))
            .is_none()
        );
    }
}
