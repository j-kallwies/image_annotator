use crate::shortcuts::*;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::{fs::File, collections::HashSet, path::PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct PersistentSettings {
    /// The UI accent color
    pub accent_color: [u8; 3],
    /// The BG color
    pub background_color: [u8; 3],
    /// Should we sync to monitor rate? This makes the app snappier, but also more resource intensive.
    pub vsync: bool,
    /// Keyboard map to actions
    pub shortcuts: Shortcuts,
    /// Do not reset view when receiving a new image
    pub keep_view: bool,
    /// How many images to keep in cache
    pub max_cache: usize,
    pub show_scrub_bar: bool,
    pub wrap_folder: bool,
    /// Whether to keep the image edit stack
    pub keep_edits: bool,
    pub favourite_images: HashSet<PathBuf>,
    pub recent_images: Vec<PathBuf>,
}

impl Default for PersistentSettings {
    fn default() -> Self {
        PersistentSettings {
            accent_color: [255, 0, 75],
            background_color: [51, 51, 51],
            vsync: true,
            shortcuts: Shortcuts::default_keys(),
            keep_view: Default::default(),
            max_cache: 30,
            show_scrub_bar: Default::default(),
            wrap_folder: true,
            keep_edits: Default::default(),
            favourite_images: Default::default(),
            recent_images: Default::default(),
        }
    }
}

impl PersistentSettings {
    pub fn load() -> Result<Self> {
        let local_dir = dirs::data_local_dir().ok_or(anyhow!("Can't get local dir"))?;
        let f = File::open(local_dir.join(".oculante"))?;
        Ok(serde_json::from_reader::<_, PersistentSettings>(f)?)
    }

    // save settings in a thread so we don't block
    pub fn save(&self) {
        let settings = self.clone();
        std::thread::spawn( move || { 
            _ = save(&settings);
        });
      
    }
}

fn save(s: &PersistentSettings) -> Result<()> {
    let local_dir = dirs::data_local_dir().ok_or(anyhow!("Can't get local dir"))?;
    let f = File::create(local_dir.join(".oculante"))?;
    Ok(serde_json::to_writer_pretty(f, s)?)
}
