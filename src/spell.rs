//! The Windows spell checker as the `Lexicon` for wrong-layout detection: local, no word lists in the app. A
//! language needs its "Basic typing" feature (Windows language settings).

use windows::Win32::Foundation::S_OK;
use windows::Win32::Globalization::{
    ISpellChecker, ISpellCheckerFactory, ISpellingError, SpellCheckerFactory,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::core::HSTRING;

use crate::detect::Lexicon;
use crate::lang;

pub struct Spelling {
    en: ISpellChecker,
    ru: ISpellChecker,
}

impl Spelling {
    /// Both checkers, or which one is missing. Call on the main thread, which then uses them.
    pub fn new() -> Result<Self, String> {
        // SAFETY: COM is set up for this thread before the factory is created; a second call only counts up.
        let factory: ISpellCheckerFactory = unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            CoCreateInstance(&SpellCheckerFactory, None, CLSCTX_INPROC_SERVER)
        }
        .map_err(|e| format!("no spell checker: {e}"))?;
        Ok(Self {
            en: checker(&factory, "en-US")?,
            ru: checker(&factory, "ru-RU")?,
        })
    }
}

fn checker(factory: &ISpellCheckerFactory, tag: &str) -> Result<ISpellChecker, String> {
    let tag = HSTRING::from(tag);
    let missing =
        || format!("detection needs the {tag} spell checker (language settings, Basic typing)");
    // SAFETY: plain COM calls on the factory's thread.
    unsafe {
        if !factory.IsSupported(&tag).is_ok_and(|s| s.as_bool()) {
            return Err(missing());
        }
        factory.CreateSpellChecker(&tag).map_err(|_| missing())
    }
}

impl Lexicon for Spelling {
    fn is_word(&self, language: u16, text: &str) -> Option<bool> {
        let checker = match language {
            lang::EN => &self.en,
            lang::RU => &self.ru,
            _ => return None,
        };
        // SAFETY: plain COM calls on the thread that created the checker.
        unsafe {
            let errors = checker.Check(&HSTRING::from(text)).ok()?;
            let mut first: Option<ISpellingError> = None;
            let found = errors.Next(&mut first);
            // S_OK: a spelling error came back; S_FALSE: none
            found.is_ok().then_some(found != S_OK)
        }
    }
}
