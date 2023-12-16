use std::path::Path;

/// YOLO label.
#[derive(Debug)]
pub struct Label {
    pub label_index: i8,
    pub x_centre: f32,
    pub y_centre: f32,
    pub width: f32,
    pub height: f32,
    pub probability: Option<f32>,
    pub object_id: Option<i8>,
}

pub trait Unnormaliser {
    fn unnormalise(&self, dimensions: (u32, u32)) -> Self;
}

impl From<&str> for Label {
    fn from(s: &str) -> Self {
        let split: Vec<&str> = s.split(|x| x == ' ' || x == '\t').collect();

        let probability = if split.len() > 5 {
            Some(split[5].parse().unwrap())
        } else {
            None
        };

        let object_id = if split.len() > 6 {
            Some(split[6].parse().unwrap())
        } else {
            None
        };

        Self {
            label_index: split[0].parse().unwrap(),
            x_centre: split[1].parse().unwrap(),
            y_centre: split[2].parse().unwrap(),
            width: split[3].parse().unwrap(),
            height: split[4].parse().unwrap(),
            probability,
            object_id,
        }
    }
}

impl Unnormaliser for Label {
    fn unnormalise(&self, dimensions: (u32, u32)) -> Self {
        let width = dimensions.0 as f32;
        let height = dimensions.1 as f32;
        Self {
            label_index: self.label_index,
            x_centre: self.x_centre * width,
            y_centre: self.y_centre * height,
            width: self.width * width,
            height: self.height * height,
            probability: self.probability,
            object_id: self.object_id,
        }
    }
}

pub struct Labels {
    // We have to use a nested item because we can't implement From<String>
    // directly on Vec<Label>
    pub labels: Vec<Label>,
}

impl From<&str> for Labels {
    fn from(s: &str) -> Self {
        Labels {
            labels: s
                .split("\n")
                .filter(|x| !x.is_empty())
                .map(Label::from)
                .collect::<Vec<Label>>(),
        }
    }
}

impl Labels {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, std::io::Error> {
        let string = std::fs::read_to_string(path)?;
        Ok(Labels::from(string.as_str()))
    }
}

// Enables `labels.iter()`
impl std::ops::Deref for Labels {
    type Target = [Label];

    fn deref(&self) -> &Self::Target {
        &self.labels[..]
    }
}

impl Unnormaliser for Labels {
    fn unnormalise(&self, dimensions: (u32, u32)) -> Self {
        Labels {
            labels: self
                .labels
                .iter()
                .map(|x| x.unnormalise(dimensions))
                .collect(),
        }
    }
}
