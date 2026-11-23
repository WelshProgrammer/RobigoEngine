//! The game's texts, from code page 437 into UTF-8 files under `texts/`: a folder a language,
//! then the file's path below `DATA`, lower case, without the extension. `DATA/TXT/ONGAME.ENG`
//! goes to `texts/en/txt/ongame` and a `.GER` file to `texts/de`. `README.TXT`, which has no
//! language, goes to `texts/readme`.
//!
//! The game reads a text in the stack's language and falls back to English: a string table slot
//! by slot, a page whole.
//!
//! A string table becomes a TOML: `slots`, how many slots it has, then a row a filled slot, keyed
//! by its number, holding the string with its `@` line breaks. A page becomes a `.txt` holding
//! every byte of the original, its line ends and end-of-file marker included, which the reading
//! windows treat as the game does. The game turns both back into code page 437 to draw them, and
//! refuses a character code page 437 does not have.

use std::collections::BTreeMap;

use s2_data::DataSet;
use s2_formats::text::{MAGIC, StringTable, cp437};

use crate::sprites::{Sheets, row_error};
use crate::{Error, ModStack, Result};

/// Where the texts go in a mod.
pub const TEXTS: &str = "texts";

/// The folders of the original data that hold texts.
const FOLDERS: [&str; 4] = ["DATA/TXT", "DATA/TXT2", "DATA/TXT3", "DATA/ONLINE"];

/// The language every copy of the game has texts in, which the others fall back to.
pub const ENGLISH: &str = "en";

/// The endings S2 gives its text files, each with the folder its language bakes into.
const LANGUAGES: [(&str, &str); 2] = [("ENG", ENGLISH), ("GER", "de")];

/// The text file outside those folders.
const README: &str = "README.TXT";

/// The key in a string table's TOML that holds how many slots it has.
const SLOTS: &str = "slots";

/// A text as the game reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Text {
    /// A table of numbered strings.
    Table(StringTable),
    /// A page's bytes, in code page 437.
    Page(Vec<u8>),
}

/// Where the text file at data path `path` goes in a mod, without the ending its kind adds.
/// `DATA/TXT/ONGAME.ENG` is `texts/en/txt/ongame`, and `README.TXT` is `texts/readme`.
#[must_use]
pub fn text_path(path: &str) -> String {
    let (stem, language) = parts(path);
    language.map_or_else(
        || format!("{TEXTS}/{stem}"),
        |language| format!("{TEXTS}/{language}/{stem}"),
    )
}

/// Where the text file at data path `path` goes in a mod for `language` in place of its own.
/// A file with no language stays where it is.
#[must_use]
pub fn text_path_in(path: &str, language: &str) -> String {
    let (stem, own) = parts(path);
    own.map_or_else(
        || format!("{TEXTS}/{stem}"),
        |_| format!("{TEXTS}/{language}/{stem}"),
    )
}

/// The data path `path` below `DATA`, lower case, without its ending, and the language the
/// ending names.
fn parts(path: &str) -> (String, Option<&'static str>) {
    let below = path.strip_prefix("DATA/").unwrap_or(path);
    let (stem, ending) = below.rsplit_once('.').unwrap_or((below, ""));
    let language = LANGUAGES
        .iter()
        .find(|(theirs, _)| theirs.eq_ignore_ascii_case(ending))
        .map(|(_, language)| *language);
    (stem.to_ascii_lowercase(), language)
}

impl Sheets<'_> {
    /// The text file at data path `path`, in the stack's language where a mod has it: a string
    /// table's slots over the slots of the language `path` names, a page in its place.
    ///
    /// # Errors
    ///
    /// [`Error::NotInMods`] when the stack has no such text, [`Error::Row`] for a baked text
    /// with a character code page 437 does not have, the stack's errors for one that does not
    /// load, and [`Error::Data`] for an original that cannot be read.
    pub fn text(&self, path: &str) -> Result<Text> {
        let Some(stack) = self.stack else {
            return Ok(original(&self.data.read(path)?));
        };
        let base = text_path(path);
        let translated = Some(text_path_in(path, stack.language())).filter(|other| *other != base);
        let table_path = format!("{base}.toml");
        if stack.file(&table_path).is_some() {
            let (mut len, mut strings) = slots(stack, &table_path)?;
            let over = translated.map(|base| format!("{base}.toml"));
            if let Some(over) = over.filter(|over| stack.file(over).is_some()) {
                let (theirs, translations) = slots(stack, &over)?;
                len = len.max(theirs);
                strings.extend(translations);
            }
            return Ok(Text::Table(StringTable::from_slots(len, strings)));
        }
        let page_path = translated
            .map(|base| format!("{base}.txt"))
            .filter(|page| stack.file(page).is_some())
            .unwrap_or_else(|| format!("{base}.txt"));
        let bytes = stack
            .read(&page_path)?
            .ok_or(Error::NotInMods(page_path.clone()))?;
        let text = String::from_utf8(bytes).map_err(|_| row_error(&page_path, "", "not UTF-8"))?;
        Ok(Text::Page(
            encode(&text).map_err(|reason| row_error(&page_path, "", reason))?,
        ))
    }
}

/// The slot count and the filled slots of the baked string table at `table_path`, in code page
/// 437.
fn slots(stack: &ModStack, table_path: &str) -> Result<(usize, BTreeMap<usize, Vec<u8>>)> {
    let mut rows = stack.table(table_path)?;
    let len = rows
        .remove(SLOTS)
        .and_then(|value| value.as_integer())
        .and_then(|len| usize::try_from(len).ok())
        .ok_or_else(|| row_error(table_path, SLOTS, "a slot count from 0 up"))?;
    let strings = rows
        .into_iter()
        .map(|(key, value)| {
            let slot = key
                .parse()
                .map_err(|_| row_error(table_path, &key, "the key is a slot number"))?;
            let string = value
                .as_str()
                .ok_or_else(|| row_error(table_path, &key, "a slot holds a string"))?;
            Ok((
                slot,
                encode(string).map_err(|reason| row_error(table_path, &key, reason))?,
            ))
        })
        .collect::<Result<_>>()?;
    Ok((len, strings))
}

/// An original text file's bytes as the game reads them.
fn original(bytes: &[u8]) -> Text {
    let tagged = bytes.get(..2) == Some(&MAGIC.to_le_bytes()[..]);
    match StringTable::parse(bytes) {
        Ok(table) if tagged => Text::Table(table),
        _ => Text::Page(bytes.to_vec()),
    }
}

/// `text` in code page 437.
fn encode(text: &str) -> std::result::Result<Vec<u8>, String> {
    text.chars()
        .map(|c| cp437::encode_char(c).ok_or_else(|| format!("code page 437 has no {c:?}")))
        .collect()
}

/// Every text file the data set holds, by data path.
pub(crate) fn text_files(data: &DataSet) -> Vec<String> {
    let mut found: Vec<String> = FOLDERS
        .iter()
        .flat_map(|folder| {
            data.list_dir(folder)
                .unwrap_or_default()
                .into_iter()
                .filter(|entry| entry.kind == s2_data::EntryKind::File)
                .map(move |entry| format!("{folder}/{}", entry.name.to_ascii_uppercase()))
        })
        .collect();
    if data.exists(README) {
        found.push(README.into());
    }
    found.sort();
    found
}

/// The text file at data path `path` to bake, by its path inside the mod.
pub(crate) fn bake_text(data: &DataSet, path: &str) -> Result<(String, Vec<u8>)> {
    let base = text_path(path);
    match original(&data.read(path)?) {
        Text::Page(bytes) => Ok((format!("{base}.txt"), cp437::decode(&bytes).into_bytes())),
        Text::Table(table) => {
            let mut rows = toml::Table::new();
            let len = i64::try_from(table.len()).unwrap_or(i64::MAX);
            rows.insert(SLOTS.into(), toml::Value::Integer(len));
            for (slot, bytes) in table.iter() {
                rows.insert(slot.to_string(), toml::Value::String(cp437::decode(bytes)));
            }
            let file = format!("{base}.toml");
            let text = toml::to_string(&rows).map_err(|source| Error::Write {
                path: file.clone(),
                source,
            })?;
            Ok((file, text.into_bytes()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_goes_below_texts_without_its_ending() {
        assert_eq!(text_path("DATA/TXT/ONGAME.ENG"), "texts/en/txt/ongame");
        assert_eq!(text_path("DATA/ONLINE/HELP1000.ENG"), "texts/en/online/help1000");
        assert_eq!(text_path("README.TXT"), "texts/readme");
    }

    #[test]
    fn each_language_bakes_into_its_own_folder() {
        assert_eq!(text_path("DATA/TXT/ONGAME.GER"), "texts/de/txt/ongame");
        assert_eq!(text_path_in("DATA/TXT/ONGAME.ENG", "fr"), "texts/fr/txt/ongame");
        assert_eq!(text_path_in("README.TXT", "fr"), "texts/readme");
    }

    #[test]
    fn every_code_page_437_byte_comes_back() {
        let bytes: Vec<u8> = (0..=u8::MAX).collect();
        assert_eq!(encode(&cp437::decode(&bytes)).unwrap(), bytes);
        assert!(encode("snow \u{2744}").is_err());
    }
}
