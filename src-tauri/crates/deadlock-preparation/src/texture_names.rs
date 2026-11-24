//! Base-color images matched to materials by file name, for materials the
//! model gives no image (staging, `source.rs`; the manifest's `materials`).
//!
//! A material name and an image file name are compared by their words (runs
//! of letters and digits, without case). A material keeps its words after a
//! leading material prefix (`MI_`, `M_`, `Mat_` ...) and before a trailing
//! one (`_Mat` ...); an image keeps its words after a leading texture prefix
//! (`T_`, `Tex_` ...) and before trailing base-color words (`_D`, `_Diffuse`,
//! `_BaseColor`, `_Color`, `_Albedo` ..., the words `material_textures.py`
//! reads as base color). An image with any other last word (`_N`, `_M`,
//! `_FTM` ...) never matches, so masks and other channels are never taken
//! for base color. The two word lists must then be equal, either as they are
//! or after a shared stem is taken off the front of one or both: the model
//! file's name, or the words every image (or every material) begins with.
//! "MI_Hair" matches "T_Model_Hair" beside "T_Model_Body"; "MI_ModelBangs"
//! matches "T_ModelBangs_D" but not "T_ModelBangs_N" or "T_ModelBangs1_D".
//! The compared words must hold a word of two letters or more, so "MI_01"
//! never matches "T_01".
use std::collections::BTreeSet;

const MATERIAL_PREFIXES: [&str; 7] = ["mi", "mic", "m", "mat", "mtl", "mt", "material"];
const MATERIAL_SUFFIXES: [&str; 4] = ["mat", "mtl", "mt", "material"];
const IMAGE_PREFIXES: [&str; 4] = ["t", "tx", "tex", "texture"];
/// Base-color words, as `material_textures.py ALIASES` reads them.
const COLOR_WORDS: [&str; 10] = ["d", "diff", "diffuse", "albedo", "basecolor", "color", "colour", "col", "bc", "base"];

/// The words of a name: runs of letters and digits, lowercase.
pub fn words(name: &str) -> Vec<String> {
    name.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// The words of a material name that tell it apart.
pub fn material_core(name: &str) -> Vec<String> {
    let mut words = words(name);
    if words.len() > 1 && MATERIAL_PREFIXES.contains(&words[0].as_str()) {
        words.remove(0);
    }
    while words.len() > 1 && MATERIAL_SUFFIXES.contains(&words[words.len() - 1].as_str()) {
        words.pop();
    }
    words
}

/// The words of an image file's stem that tell it apart, without its
/// texture prefix and base-color words.
pub fn image_core(stem: &str) -> Vec<String> {
    let mut words = words(stem);
    if words.len() > 1 && IMAGE_PREFIXES.contains(&words[0].as_str()) {
        words.remove(0);
    }
    // At most two words ("Base_Color"), and at least one word stays.
    for _ in 0..2 {
        if words.len() > 1 && COLOR_WORDS.contains(&words[words.len() - 1].as_str()) {
            words.pop();
        }
    }
    words
}

/// The words all of `cores` begin with, when there are two or more and every
/// one keeps at least one word after them.
fn common_start(cores: &[Vec<String>]) -> Option<Vec<String>> {
    if cores.len() < 2 {
        return None;
    }
    let shortest = cores.iter().map(Vec::len).min()?;
    let mut length = 0;
    while length + 1 < shortest && cores.iter().all(|c| c[length] == cores[0][length]) {
        length += 1;
    }
    (length > 0).then(|| cores[0][..length].to_vec())
}

fn distinctive(words: &[String]) -> bool {
    words.iter().any(|w| w.chars().filter(|c| c.is_alphabetic()).count() >= 2)
}

/// Matches image stems to material names of one model.
#[derive(Debug, Clone)]
pub struct Matcher {
    stems: Vec<Vec<String>>,
}

impl Matcher {
    /// `model`: the model file's stem; `images`: the stems of the images of
    /// the model (named by it or found beside it); `materials`: every
    /// material name of the model.
    pub fn new<'a>(model: &str, images: impl IntoIterator<Item = &'a str>, materials: impl IntoIterator<Item = &'a str>) -> Self {
        let images: BTreeSet<Vec<String>> = images.into_iter().map(image_core).filter(|c| !c.is_empty()).collect();
        let materials: BTreeSet<Vec<String>> = materials.into_iter().map(material_core).filter(|c| !c.is_empty()).collect();
        let mut stems = vec![words(model)];
        stems.extend(common_start(&images.into_iter().collect::<Vec<_>>()));
        stems.extend(common_start(&materials.into_iter().collect::<Vec<_>>()));
        stems.retain(|s| !s.is_empty());
        stems.sort();
        stems.dedup();
        Matcher { stems }
    }
    /// The word lists `core` stands for: itself and itself without each
    /// shared stem it begins with.
    fn variants(&self, core: Vec<String>) -> Vec<Vec<String>> {
        let mut out = vec![];
        for stem in &self.stems {
            if core.len() > stem.len() && core.starts_with(stem) {
                out.push(core[stem.len()..].to_vec());
            }
        }
        out.push(core);
        out
    }
    /// Whether the image with stem `image` can be the base color of the
    /// material `material` by name.
    pub fn fits(&self, material: &str, image: &str) -> bool {
        let (material, image) = (material_core(material), image_core(image));
        if material.is_empty() || image.is_empty() || !Self::may_fit_cores(&material, &image) {
            return false;
        }
        let images = self.variants(image);
        self.variants(material).iter().any(|m| distinctive(m) && images.contains(m))
    }
    fn may_fit_cores(material: &[String], image: &[String]) -> bool {
        material.last().is_some() && material.last() == image.last()
    }
    /// A quick test that `fits` can be true: the same last word. Used to keep
    /// a folder search to the images that may match.
    pub fn may_fit(material: &str, image: &str) -> bool {
        Self::may_fit_cores(&material_core(material), &image_core(image))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cores_drop_prefixes_and_base_color_words() {
        assert_eq!(material_core("MI_Body"), ["body"]);
        assert_eq!(material_core("M_Skin_Mat"), ["skin"]);
        assert_eq!(material_core("Mat"), ["mat"]);
        assert_eq!(image_core("T_Body_D"), ["body"]);
        assert_eq!(image_core("Hair_Base_Color"), ["hair"]);
        assert_eq!(image_core("T_Body_N"), ["body", "n"]);
        assert_eq!(image_core("T_D"), ["d"]);
        assert_eq!(words("Лицо-01 Face"), ["лицо", "01", "face"]);
    }

    #[test]
    fn a_shared_stem_may_stand_in_front_of_either_name() {
        let images = ["T_Sample_body", "T_sample_Clothes", "T_Sample_Face", "T_Sample_Hair", "T_Sample_Weapon"];
        let materials = ["MI_Body", "MI_Face", "MI_Hair", "MI_Clothes", "MI_eyewhite", "MI_Sword"];
        let matcher = Matcher::new("Character", images, materials);
        assert!(matcher.fits("MI_Hair", "T_Sample_Hair"));
        assert!(matcher.fits("MI_Clothes", "T_sample_Clothes"));
        assert!(!matcher.fits("MI_Hair", "T_Sample_Face"));
        assert!(!matcher.fits("MI_Sword", "T_Sample_Weapon"));
        assert!(!matcher.fits("MI_eyewhite", "T_Sample_Face"));
        // The model's own name is a stem too, with a single image.
        let matcher = Matcher::new("Sample", ["T_Sample_Hair"], ["MI_Hair"]);
        assert!(matcher.fits("MI_Hair", "T_Sample_Hair"));
        // Without any shared stem, other words in front do not match.
        let matcher = Matcher::new("Character", ["T_Rifle_Hair"], ["MI_Hair"]);
        assert!(!matcher.fits("MI_Hair", "T_Rifle_Hair"));
        // The stem on the material's side.
        let matcher = Matcher::new("Sample", ["Hair_D"], ["Sample_Hair"]);
        assert!(matcher.fits("Sample_Hair", "Hair_D"));
    }

    #[test]
    fn names_without_separators_match_whole() {
        let images = ["T_AB12345Bangs_D", "T_AB12345Bangs_FTM", "T_AB12345Bangs_HN", "T_AB12345Up_D", "T_AB12345Up1_D", "T_AB12345Up_N", "T_AB12345_Skin", "T_AB12345Eye_250000", "T_AB12345Eye_D"];
        let materials = ["MI_AB12345Bangs", "MI_AB12345Up", "MI_AB12345Up1", "MI_AB12345Eye"];
        let matcher = Matcher::new("AB12345", images, materials);
        let fits = |m: &str| images.iter().filter(|i| matcher.fits(m, i)).copied().collect::<Vec<_>>();
        assert_eq!(fits("MI_AB12345Bangs"), ["T_AB12345Bangs_D"]);
        assert_eq!(fits("MI_AB12345Up"), ["T_AB12345Up_D"]);
        assert_eq!(fits("MI_AB12345Up1"), ["T_AB12345Up1_D"]);
        assert_eq!(fits("MI_AB12345Eye"), ["T_AB12345Eye_D"]);
    }

    #[test]
    fn other_channels_and_numbers_never_match() {
        let matcher = Matcher::new("Sample", ["Body_M", "Body_S", "Body_Normal", "01"], ["Body", "MI_01"]);
        for image in ["Body_M", "Body_S", "Body_Normal"] {
            assert!(!matcher.fits("Body", image), "{image}");
        }
        assert!(!matcher.fits("MI_01", "01"));
        assert!(matcher.fits("Body", "Body"));
        assert!(Matcher::may_fit("MI_Hair", "T_X_Hair_D") && !Matcher::may_fit("MI_Hair", "T_Hair_N"));
    }
}
